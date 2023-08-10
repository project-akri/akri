use super::credential_store::CredentialStore;
use super::discovery_impl::util;
use super::discovery_utils::{
    OnvifQuery, OnvifQueryImpl, ONVIF_DEVICE_IP_ADDRESS_LABEL_ID,
    ONVIF_DEVICE_MAC_ADDRESS_LABEL_ID, ONVIF_DEVICE_SERVICE_URL_LABEL_ID,
    ONVIF_DEVICE_UUID_LABEL_ID,
};
use akri_discovery_utils::{
    discovery::{
        discovery_handler::{deserialize_discovery_details, DISCOVERED_DEVICES_CHANNEL_CAPACITY},
        v0::{
            discovery_handler_server::DiscoveryHandler, Device, DiscoverRequest, DiscoverResponse,
        },
        DiscoverStream,
    },
    filtering::FilterList,
};
use async_trait::async_trait;
use log::{error, info, trace};
use std::{collections::HashMap, time::Duration};
use tokio::{sync::mpsc, time::sleep};
use tonic::{Response, Status};

// TODO: make this configurable
pub const DISCOVERY_INTERVAL_SECS: u64 = 10;

/// This defines the ONVIF data stored in the Configuration
/// CRD
///
/// The ONVIF discovery handler is structured to store a filter list for
/// ip addresses, mac addresses, and ONVIF scopes.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct OnvifDiscoveryDetails {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ip_addresses: Option<FilterList>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mac_addresses: Option<FilterList>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scopes: Option<FilterList>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uuids: Option<FilterList>,
    #[serde(default = "default_discovery_timeout_seconds")]
    pub discovery_timeout_seconds: i32,
}

fn default_discovery_timeout_seconds() -> i32 {
    1
}

/// `DiscoveryHandlerImpl` discovers the onvif instances as described by the `OnvifDiscoveryDetails` filters `ip_addresses`,
/// `mac_addresses`, and `scopes`.
/// The instances it discovers are always shared.
pub struct DiscoveryHandlerImpl {
    register_sender: Option<mpsc::Sender<()>>,
}

impl DiscoveryHandlerImpl {
    pub fn new(register_sender: Option<mpsc::Sender<()>>) -> Self {
        DiscoveryHandlerImpl { register_sender }
    }
}

#[async_trait]
impl DiscoveryHandler for DiscoveryHandlerImpl {
    type DiscoverStream = DiscoverStream;
    async fn discover(
        &self,
        request: tonic::Request<DiscoverRequest>,
    ) -> Result<Response<Self::DiscoverStream>, Status> {
        info!("discover - called for ONVIF protocol");
        let register_sender = self.register_sender.clone();
        let discover_request = request.get_ref();
        let (discovered_devices_sender, discovered_devices_receiver) =
            mpsc::channel(DISCOVERED_DEVICES_CHANNEL_CAPACITY);
        let discovery_handler_config: OnvifDiscoveryDetails =
            deserialize_discovery_details(&discover_request.discovery_details)
                .map_err(|e| tonic::Status::new(tonic::Code::InvalidArgument, format!("{}", e)))?;
        let credential_store = CredentialStore::new(&discover_request.discovery_properties);
        let onvif_query = OnvifQueryImpl::new(credential_store);
        tokio::spawn(async move {
            let mut previous_cameras = HashMap::new();
            let mut filtered_camera_devices = HashMap::new();
            loop {
                // Before each iteration, check if receiver has dropped
                if discovered_devices_sender.is_closed() {
                    error!("discover - channel closed ... attempting to re-register with Agent");
                    if let Some(sender) = register_sender {
                        sender.send(()).await.unwrap();
                    }
                    break;
                }
                let mut changed_camera_list = false;

                trace!("discover - filters:{:?}", &discovery_handler_config,);
                let mut socket = util::get_discovery_response_socket().await.unwrap();
                let latest_cameras = util::simple_onvif_discover(
                    &mut socket,
                    discovery_handler_config.scopes.as_ref(),
                    Duration::from_secs(discovery_handler_config.discovery_timeout_seconds as u64),
                )
                .await
                .unwrap();
                trace!("discover - discovered:{:?}", &latest_cameras);
                // Remove cameras that have gone offline
                previous_cameras.keys().for_each(|c| {
                    if !latest_cameras.contains_key(c) {
                        changed_camera_list = true;
                        filtered_camera_devices.remove(c);
                    }
                });

                let futures: Vec<_> = latest_cameras
                    .iter()
                    .filter(|(k, _)| !previous_cameras.contains_key(*k))
                    .map(|(uri, uuid)| {
                        apply_filters(&discovery_handler_config, uri, uuid, &onvif_query)
                    })
                    .collect();
                let options = futures_util::future::join_all(futures).await;
                // Insert newly discovered camera that are not filtered out
                options.into_iter().for_each(|o| {
                    if let Some((service_url, d)) = o {
                        changed_camera_list = true;
                        filtered_camera_devices.insert(service_url, d);
                    }
                });

                if changed_camera_list {
                    info!("discover - sending updated device list");
                    previous_cameras = latest_cameras;
                    if let Err(e) = discovered_devices_sender
                        .send(Ok(DiscoverResponse {
                            devices: filtered_camera_devices.values().cloned().collect(),
                        }))
                        .await
                    {
                        error!(
                            "discover - for ONVIF failed to send discovery response with error {}",
                            e
                        );
                        if let Some(sender) = register_sender {
                            sender.send(()).await.unwrap();
                        }
                        break;
                    }
                }
                sleep(Duration::from_secs(DISCOVERY_INTERVAL_SECS)).await;
            }
        });
        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(
            discovered_devices_receiver,
        )))
    }
}

async fn apply_filters(
    discovery_handler_config: &OnvifDiscoveryDetails,
    device_service_uri: &str,
    device_uuid: &str,
    onvif_query: &impl OnvifQuery,
) -> Option<(String, Device)> {
    info!(
        "apply_filters - device service url {}, uuid {}",
        device_service_uri, device_uuid
    );
    // Evaluate device uuid against uuids filter if provided
    if util::execute_filter(
        discovery_handler_config.uuids.as_ref(),
        Some(vec![device_uuid.to_string()]).as_ref(),
        |uuid, pattern| uuid.to_lowercase() == pattern.to_lowercase(),
    ) {
        return None;
    }

    let ip_and_mac = onvif_query
        .get_device_ip_and_mac_address(device_service_uri, device_uuid)
        .await
        .map_err(|e| {
            error!("apply_filters - error getting ip and mac address: {}", e);
            e
        })
        .ok();
    // Evaluate camera ip address against ip filter if provided
    // use case-insensitive comparison in case of IPv6 is used
    let ip_address_as_vec = ip_and_mac.as_ref().map(|(ip, _)| vec![ip.clone()]);
    if util::execute_filter(
        discovery_handler_config.ip_addresses.as_ref(),
        ip_address_as_vec.as_ref(),
        |ip, pattern| ip.to_lowercase() == pattern.to_lowercase(),
    ) {
        return None;
    }

    // Evaluate camera mac address against mac filter if provided
    let mac_address_as_vec = ip_and_mac.as_ref().map(|(_, mac)| vec![mac.clone()]);
    if util::execute_filter(
        discovery_handler_config.mac_addresses.as_ref(),
        mac_address_as_vec.as_ref(),
        |mac, pattern| mac.to_lowercase() == pattern.to_lowercase(),
    ) {
        return None;
    }

    let service_uri_and_uuid_joined = format!("{}-{}", device_service_uri, device_uuid);
    let mut properties = HashMap::new();
    properties.insert(
        ONVIF_DEVICE_SERVICE_URL_LABEL_ID.to_string(),
        device_service_uri.to_string(),
    );
    properties.insert(ONVIF_DEVICE_UUID_LABEL_ID.into(), device_uuid.to_string());
    if let Some((ip_address, mac_address)) = ip_and_mac {
        properties.insert(ONVIF_DEVICE_IP_ADDRESS_LABEL_ID.into(), ip_address);
        properties.insert(ONVIF_DEVICE_MAC_ADDRESS_LABEL_ID.into(), mac_address);
    }

    Some((
        device_service_uri.to_string(),
        Device {
            id: service_uri_and_uuid_joined,
            properties,
            mounts: Vec::default(),
            device_specs: Vec::default(),
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::super::discovery_utils::MockOnvifQuery;
    use super::*;
    use akri_discovery_utils::filtering::FilterType;

    #[derive(Clone)]
    struct IpAndMac {
        ip: &'static str,
        mac: &'static str,
    }

    fn configure_scenario(
        mock: &mut MockOnvifQuery,
        uri: &'static str,
        mock_result: Result<IpAndMac, String>,
    ) {
        configure_get_device_ip_and_mac_address(
            mock,
            uri,
            mock_result.map(|ip_and_mac| (ip_and_mac.ip.to_string(), ip_and_mac.mac.to_string())),
        )
    }

    fn configure_get_device_ip_and_mac_address(
        mock: &mut MockOnvifQuery,
        uri: &'static str,
        result: Result<(String, String), String>,
    ) {
        mock.expect_get_device_ip_and_mac_address()
            .times(1)
            .withf(move |u, _uuid| u == uri)
            .returning(move |_, _| result.clone().map_err(|e| anyhow::format_err!(e)));
    }

    fn expected_device(uri: &str, uuid: &str, ip_and_mac: Option<IpAndMac>) -> (String, Device) {
        let mut properties = HashMap::new();
        properties.insert(
            ONVIF_DEVICE_SERVICE_URL_LABEL_ID.to_string(),
            uri.to_string(),
        );
        properties.insert(ONVIF_DEVICE_UUID_LABEL_ID.into(), uuid.to_string());
        if let Some(ip_and_mac) = ip_and_mac {
            properties.insert(
                ONVIF_DEVICE_IP_ADDRESS_LABEL_ID.into(),
                ip_and_mac.ip.to_string(),
            );
            properties.insert(
                ONVIF_DEVICE_MAC_ADDRESS_LABEL_ID.into(),
                ip_and_mac.mac.to_string(),
            );
        }

        (
            uri.to_string(),
            Device {
                id: format!("{}-{}", uri, uuid),
                properties,
                mounts: Vec::default(),
                device_specs: Vec::default(),
            },
        )
    }

    #[test]
    fn test_deserialize_discovery_details() {
        let _ = env_logger::builder().is_test(true).try_init();

        let dh_config: OnvifDiscoveryDetails = deserialize_discovery_details("{}").unwrap();
        let serialized = serde_json::to_string(&dh_config).unwrap();
        let expected_deserialized = r#"{"discoveryTimeoutSeconds":1}"#;
        assert_eq!(expected_deserialized, serialized);
    }

    #[tokio::test]
    async fn test_apply_filters_no_filters() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mock_uri = "device_uri";
        let mock_uuid = "device_uuid";
        let mock_ip_and_mac = IpAndMac {
            ip: "mock.ip",
            mac: "mock:mac",
        };

        let mut mock = MockOnvifQuery::new();
        configure_scenario(&mut mock, mock_uri, Ok(mock_ip_and_mac.clone()));

        let onvif_config = OnvifDiscoveryDetails {
            ip_addresses: None,
            mac_addresses: None,
            scopes: None,
            uuids: None,
            discovery_timeout_seconds: 1,
        };
        let instance = apply_filters(&onvif_config, mock_uri, mock_uuid, &mock)
            .await
            .unwrap();

        assert_eq!(
            expected_device(mock_uri, mock_uuid, Some(mock_ip_and_mac)),
            instance
        );
    }

    #[tokio::test]
    async fn test_apply_filters_no_filters_get_ip_mac_address_fail() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mock_uri = "device_uri";
        let mock_uuid = "device_uuid";

        let mut mock = MockOnvifQuery::new();
        configure_scenario(
            &mut mock,
            mock_uri,
            Err(String::from("mock get_device_ip_and_mac_address failure")),
        );

        let onvif_config = OnvifDiscoveryDetails {
            ip_addresses: None,
            mac_addresses: None,
            scopes: None,
            uuids: None,
            discovery_timeout_seconds: 1,
        };
        let instance = apply_filters(&onvif_config, mock_uri, mock_uuid, &mock)
            .await
            .unwrap();

        assert_eq!(expected_device(mock_uri, mock_uuid, None), instance);
    }

    #[tokio::test]
    async fn test_apply_filters_ip_filter_get_ip_mac_address_fail() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mock_uri = "device_uri";
        let mock_uuid = "device_uuid";
        let mock_ip = "mock.ip";

        let mut mock = MockOnvifQuery::new();
        configure_scenario(
            &mut mock,
            mock_uri,
            Err(String::from("mock get_device_ip_and_mac_address failure")),
        );

        let onvif_config = OnvifDiscoveryDetails {
            ip_addresses: Some(FilterList {
                action: FilterType::Include,
                items: vec![mock_ip.to_string()],
            }),
            mac_addresses: None,
            scopes: None,
            uuids: None,
            discovery_timeout_seconds: 1,
        };
        assert!(apply_filters(&onvif_config, mock_uri, mock_uuid, &mock)
            .await
            .is_none());
    }

    #[tokio::test]
    async fn test_apply_filters_include_ip_exist() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mock_uri = "device_uri";
        let mock_uuid = "device_uuid";
        let mock_ip = "mock.ip";
        let mock_ip_and_mac = IpAndMac {
            ip: mock_ip,
            mac: "mock:mac",
        };

        let mut mock = MockOnvifQuery::new();
        configure_scenario(&mut mock, mock_uri, Ok(mock_ip_and_mac.clone()));

        let onvif_config = OnvifDiscoveryDetails {
            ip_addresses: Some(FilterList {
                action: FilterType::Include,
                items: vec![mock_ip.to_string()],
            }),
            mac_addresses: None,
            scopes: None,
            uuids: None,
            discovery_timeout_seconds: 1,
        };
        let instance = apply_filters(&onvif_config, mock_uri, mock_uuid, &mock)
            .await
            .unwrap();

        assert_eq!(
            expected_device(mock_uri, mock_uuid, Some(mock_ip_and_mac)),
            instance
        );
    }

    #[tokio::test]
    async fn test_apply_filters_include_ip_nonexist() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mock_uri = "device_uri";
        let mock_uuid = "device_uuid";
        let mock_ip_and_mac = IpAndMac {
            ip: "mock.ip",
            mac: "mock:mac",
        };

        let mut mock = MockOnvifQuery::new();
        configure_scenario(&mut mock, mock_uri, Ok(mock_ip_and_mac));

        let onvif_config = OnvifDiscoveryDetails {
            ip_addresses: Some(FilterList {
                action: FilterType::Include,
                items: vec!["nonexist.ip".to_string()],
            }),
            mac_addresses: None,
            scopes: None,
            uuids: None,
            discovery_timeout_seconds: 1,
        };
        assert!(apply_filters(&onvif_config, mock_uri, mock_uuid, &mock)
            .await
            .is_none());
    }

    #[tokio::test]
    async fn test_apply_filters_include_ip_similar() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mock_uri = "device_uri";
        let mock_uuid = "device_uuid";
        let mock_ip_and_mac = IpAndMac {
            ip: "mock.ip",
            mac: "mock:mac",
        };

        let mut mock = MockOnvifQuery::new();
        configure_scenario(&mut mock, mock_uri, Ok(mock_ip_and_mac));

        let onvif_config = OnvifDiscoveryDetails {
            ip_addresses: Some(FilterList {
                action: FilterType::Include,
                items: vec!["mock.i".to_string()],
            }),
            mac_addresses: None,
            scopes: None,
            uuids: None,
            discovery_timeout_seconds: 1,
        };
        assert!(apply_filters(&onvif_config, mock_uri, mock_uuid, &mock)
            .await
            .is_none());
    }

    #[tokio::test]
    async fn test_apply_filters_exclude_ip_nonexist() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mock_uri = "device_uri";
        let mock_uuid = "device_uuid";
        let mock_ip_and_mac = IpAndMac {
            ip: "mock.ip",
            mac: "mock:mac",
        };

        let mut mock = MockOnvifQuery::new();
        configure_scenario(&mut mock, mock_uri, Ok(mock_ip_and_mac.clone()));

        let onvif_config = OnvifDiscoveryDetails {
            ip_addresses: Some(FilterList {
                action: FilterType::Exclude,
                items: vec!["nonexist.ip".to_string()],
            }),
            mac_addresses: None,
            scopes: None,
            uuids: None,
            discovery_timeout_seconds: 1,
        };
        let instance = apply_filters(&onvif_config, mock_uri, mock_uuid, &mock)
            .await
            .unwrap();

        assert_eq!(
            expected_device(mock_uri, mock_uuid, Some(mock_ip_and_mac)),
            instance
        );
    }

    #[tokio::test]
    async fn test_apply_filters_exclude_ip_exist() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mock_uri = "device_uri";
        let mock_uuid = "device_uuid";
        let mock_ip = "mock.ip";
        let mock_ip_and_mac = IpAndMac {
            ip: mock_ip,
            mac: "mock:mac",
        };

        let mut mock = MockOnvifQuery::new();
        configure_scenario(&mut mock, mock_uri, Ok(mock_ip_and_mac.clone()));

        let onvif_config = OnvifDiscoveryDetails {
            ip_addresses: Some(FilterList {
                action: FilterType::Exclude,
                items: vec![mock_ip.to_string()],
            }),
            mac_addresses: None,
            scopes: None,
            uuids: None,
            discovery_timeout_seconds: 1,
        };
        assert!(apply_filters(&onvif_config, mock_uri, mock_uuid, &mock)
            .await
            .is_none());
    }

    #[tokio::test]
    async fn test_apply_filters_exclude_ip_similar() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mock_uri = "device_uri";
        let mock_uuid = "device_uuid";
        let mock_ip_and_mac = IpAndMac {
            ip: "mock.ip",
            mac: "mock:mac",
        };

        let mut mock = MockOnvifQuery::new();
        configure_scenario(&mut mock, mock_uri, Ok(mock_ip_and_mac.clone()));

        let onvif_config = OnvifDiscoveryDetails {
            ip_addresses: Some(FilterList {
                action: FilterType::Exclude,
                items: vec!["mock.i".to_string()],
            }),
            mac_addresses: None,
            scopes: None,
            uuids: None,
            discovery_timeout_seconds: 1,
        };
        let instance = apply_filters(&onvif_config, mock_uri, mock_uuid, &mock)
            .await
            .unwrap();

        assert_eq!(
            expected_device(mock_uri, mock_uuid, Some(mock_ip_and_mac)),
            instance
        );
    }

    #[tokio::test]
    async fn test_apply_filters_mac_filter_get_ip_mac_address_fail() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mock_uri = "device_uri";
        let mock_uuid = "device_uuid";
        let mock_mac = "mock:mac";

        let mut mock = MockOnvifQuery::new();
        configure_scenario(
            &mut mock,
            mock_uri,
            Err(String::from("mock get_device_ip_and_mac_address failure")),
        );

        let onvif_config = OnvifDiscoveryDetails {
            ip_addresses: None,
            mac_addresses: Some(FilterList {
                action: FilterType::Include,
                items: vec![mock_mac.to_string()],
            }),
            scopes: None,
            uuids: None,
            discovery_timeout_seconds: 1,
        };
        assert!(apply_filters(&onvif_config, mock_uri, mock_uuid, &mock)
            .await
            .is_none());
    }

    #[tokio::test]
    async fn test_apply_filters_include_mac_exist() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mock_uri = "device_uri";
        let mock_uuid = "device_uuid";
        let mock_mac = "mock:mac";
        let mock_ip_and_mac = IpAndMac {
            ip: "mock.ip",
            mac: mock_mac,
        };

        let mut mock = MockOnvifQuery::new();
        configure_scenario(&mut mock, mock_uri, Ok(mock_ip_and_mac.clone()));

        let onvif_config = OnvifDiscoveryDetails {
            ip_addresses: None,
            mac_addresses: Some(FilterList {
                action: FilterType::Include,
                items: vec![mock_mac.to_string()],
            }),
            scopes: None,
            uuids: None,
            discovery_timeout_seconds: 1,
        };
        let instance = apply_filters(&onvif_config, mock_uri, mock_uuid, &mock)
            .await
            .unwrap();

        assert_eq!(
            expected_device(mock_uri, mock_uuid, Some(mock_ip_and_mac)),
            instance
        );
    }

    #[tokio::test]
    async fn test_apply_filters_include_mac_nonexist() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mock_uri = "device_uri";
        let mock_uuid = "device_uuid";
        let mock_ip_and_mac = IpAndMac {
            ip: "mock.ip",
            mac: "mock:mac",
        };

        let mut mock = MockOnvifQuery::new();
        configure_scenario(&mut mock, mock_uri, Ok(mock_ip_and_mac.clone()));

        let onvif_config = OnvifDiscoveryDetails {
            ip_addresses: None,
            mac_addresses: Some(FilterList {
                action: FilterType::Include,
                items: vec!["nonexist:mac".to_string()],
            }),
            scopes: None,
            uuids: None,
            discovery_timeout_seconds: 1,
        };
        assert!(apply_filters(&onvif_config, mock_uri, mock_uuid, &mock)
            .await
            .is_none());
    }

    #[tokio::test]
    async fn test_apply_filters_exclude_mac_nonexist() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mock_uri = "device_uri";
        let mock_uuid = "device_uuid";
        let mock_ip_and_mac = IpAndMac {
            ip: "mock.ip",
            mac: "mock:mac",
        };

        let mut mock = MockOnvifQuery::new();
        configure_scenario(&mut mock, mock_uri, Ok(mock_ip_and_mac.clone()));

        let onvif_config = OnvifDiscoveryDetails {
            ip_addresses: None,
            mac_addresses: Some(FilterList {
                action: FilterType::Exclude,
                items: vec!["nonexist:mac".to_string()],
            }),
            scopes: None,
            uuids: None,
            discovery_timeout_seconds: 1,
        };
        let instance = apply_filters(&onvif_config, mock_uri, mock_uuid, &mock)
            .await
            .unwrap();

        assert_eq!(
            expected_device(mock_uri, mock_uuid, Some(mock_ip_and_mac)),
            instance
        );
    }

    #[tokio::test]
    async fn test_apply_filters_exclude_mac_exist() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mock_uri = "device_uri";
        let mock_uuid = "device_uuid";
        let mock_mac = "mock:mac";
        let mock_ip_and_mac = IpAndMac {
            ip: "mock.ip",
            mac: mock_mac,
        };

        let mut mock = MockOnvifQuery::new();
        configure_scenario(&mut mock, mock_uri, Ok(mock_ip_and_mac.clone()));

        let onvif_config = OnvifDiscoveryDetails {
            ip_addresses: None,
            mac_addresses: Some(FilterList {
                action: FilterType::Exclude,
                items: vec![mock_mac.to_string()],
            }),
            scopes: None,
            uuids: None,
            discovery_timeout_seconds: 1,
        };
        assert!(apply_filters(&onvif_config, mock_uri, mock_uuid, &mock)
            .await
            .is_none());
    }

    #[tokio::test]
    async fn test_apply_filters_include_mac_exist_different_letter_cases() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mock_uri = "device_uri";
        let mock_uuid = "device_uuid";
        let mock_mac = "MocK:Mac";
        let mock_ip_and_mac = IpAndMac {
            ip: "mock.ip",
            mac: mock_mac,
        };

        let mut mock = MockOnvifQuery::new();
        configure_scenario(&mut mock, mock_uri, Ok(mock_ip_and_mac.clone()));

        let onvif_config = OnvifDiscoveryDetails {
            ip_addresses: None,
            mac_addresses: Some(FilterList {
                action: FilterType::Include,
                items: vec![mock_mac.to_uppercase()],
            }),
            scopes: None,
            uuids: None,
            discovery_timeout_seconds: 1,
        };
        let instance = apply_filters(&onvif_config, mock_uri, mock_uuid, &mock)
            .await
            .unwrap();

        assert_eq!(
            expected_device(mock_uri, mock_uuid, Some(mock_ip_and_mac)),
            instance
        );
    }

    #[tokio::test]
    async fn test_apply_filters_exclude_mac_exist_different_letter_cases() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mock_uri = "device_uri";
        let mock_uuid = "device_uuid";
        let mock_mac = "MocK:Mac";
        let mock_ip_and_mac = IpAndMac {
            ip: "mock.ip",
            mac: mock_mac,
        };

        let mut mock = MockOnvifQuery::new();
        configure_scenario(&mut mock, mock_uri, Ok(mock_ip_and_mac.clone()));

        let onvif_config = OnvifDiscoveryDetails {
            ip_addresses: None,
            mac_addresses: Some(FilterList {
                action: FilterType::Exclude,
                items: vec![mock_mac.to_uppercase()],
            }),
            scopes: None,
            uuids: None,
            discovery_timeout_seconds: 1,
        };
        assert!(apply_filters(&onvif_config, mock_uri, mock_uuid, &mock)
            .await
            .is_none());
    }

    #[tokio::test]
    async fn test_apply_filters_include_uuid_exist() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mock_uri = "device_uri";
        let mock_uuid = "device_uuid";
        let mock_ip_and_mac = IpAndMac {
            ip: "mock.ip",
            mac: "mock:mac",
        };

        let mut mock = MockOnvifQuery::new();
        configure_scenario(&mut mock, mock_uri, Ok(mock_ip_and_mac.clone()));

        let onvif_config = OnvifDiscoveryDetails {
            ip_addresses: None,
            mac_addresses: None,
            scopes: None,
            uuids: Some(FilterList {
                action: FilterType::Include,
                items: vec![mock_uuid.to_string()],
            }),
            discovery_timeout_seconds: 1,
        };
        let instance = apply_filters(&onvif_config, mock_uri, mock_uuid, &mock)
            .await
            .unwrap();

        assert_eq!(
            expected_device(mock_uri, mock_uuid, Some(mock_ip_and_mac)),
            instance
        );
    }

    #[tokio::test]
    async fn test_apply_filters_include_uuid_nonexist() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mock_uri = "device_uri";
        let mock_uuid = "device_uuid";

        let mock = MockOnvifQuery::new();
        let onvif_config = OnvifDiscoveryDetails {
            ip_addresses: None,
            mac_addresses: None,
            scopes: None,
            uuids: Some(FilterList {
                action: FilterType::Include,
                items: vec!["nonexist-uuid".to_string()],
            }),
            discovery_timeout_seconds: 1,
        };
        assert!(apply_filters(&onvif_config, mock_uri, mock_uuid, &mock)
            .await
            .is_none());
    }

    #[tokio::test]
    async fn test_apply_filters_include_uuid_similar() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mock_uri = "device_uri";
        let mock_uuid = "device_uuid";

        let mock = MockOnvifQuery::new();
        let onvif_config = OnvifDiscoveryDetails {
            ip_addresses: None,
            mac_addresses: None,
            scopes: None,
            uuids: Some(FilterList {
                action: FilterType::Include,
                items: vec!["device_uui".to_string()],
            }),
            discovery_timeout_seconds: 1,
        };
        assert!(apply_filters(&onvif_config, mock_uri, mock_uuid, &mock)
            .await
            .is_none());
    }

    #[tokio::test]
    async fn test_apply_filters_exclude_uuid_exist() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mock_uri = "device_uri";
        let mock_uuid = "device_uuid";

        let mock = MockOnvifQuery::new();
        let onvif_config = OnvifDiscoveryDetails {
            ip_addresses: None,
            mac_addresses: None,
            scopes: None,
            uuids: Some(FilterList {
                action: FilterType::Exclude,
                items: vec![mock_uuid.to_string()],
            }),
            discovery_timeout_seconds: 1,
        };
        assert!(apply_filters(&onvif_config, mock_uri, mock_uuid, &mock)
            .await
            .is_none());
    }

    #[tokio::test]
    async fn test_apply_filters_exclude_uuid_nonexist() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mock_uri = "device_uri";
        let mock_uuid = "device_uuid";
        let mock_ip_and_mac = IpAndMac {
            ip: "mock.ip",
            mac: "mock:mac",
        };

        let mut mock = MockOnvifQuery::new();
        configure_scenario(&mut mock, mock_uri, Ok(mock_ip_and_mac.clone()));

        let onvif_config = OnvifDiscoveryDetails {
            ip_addresses: None,
            mac_addresses: None,
            scopes: None,
            uuids: Some(FilterList {
                action: FilterType::Exclude,
                items: vec!["nonexist-uuid".to_string()],
            }),
            discovery_timeout_seconds: 1,
        };
        let instance = apply_filters(&onvif_config, mock_uri, mock_uuid, &mock)
            .await
            .unwrap();

        assert_eq!(
            expected_device(mock_uri, mock_uuid, Some(mock_ip_and_mac)),
            instance
        );
    }

    #[tokio::test]
    async fn test_apply_filters_exclude_uuid_similar() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mock_uri = "device_uri";
        let mock_uuid = "device_uuid";
        let mock_ip_and_mac = IpAndMac {
            ip: "mock.ip",
            mac: "mock:mac",
        };

        let mut mock = MockOnvifQuery::new();
        configure_scenario(&mut mock, mock_uri, Ok(mock_ip_and_mac.clone()));

        let onvif_config = OnvifDiscoveryDetails {
            ip_addresses: None,
            mac_addresses: None,
            scopes: None,
            uuids: Some(FilterList {
                action: FilterType::Exclude,
                items: vec!["device_uui".to_string()],
            }),
            discovery_timeout_seconds: 1,
        };
        let instance = apply_filters(&onvif_config, mock_uri, mock_uuid, &mock)
            .await
            .unwrap();

        assert_eq!(
            expected_device(mock_uri, mock_uuid, Some(mock_ip_and_mac)),
            instance
        );
    }

    #[tokio::test]
    async fn test_apply_filters_include_uuid_exist_different_letter_cases() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mock_uri = "device_uri";
        let mock_uuid = "Device_Uuid";
        let mock_ip_and_mac = IpAndMac {
            ip: "mock.ip",
            mac: "mock:mac",
        };

        let mut mock = MockOnvifQuery::new();
        configure_scenario(&mut mock, mock_uri, Ok(mock_ip_and_mac.clone()));

        let onvif_config = OnvifDiscoveryDetails {
            ip_addresses: None,
            mac_addresses: None,
            scopes: None,
            uuids: Some(FilterList {
                action: FilterType::Include,
                items: vec![mock_uuid.to_uppercase()],
            }),
            discovery_timeout_seconds: 1,
        };
        let instance = apply_filters(&onvif_config, mock_uri, mock_uuid, &mock)
            .await
            .unwrap();

        assert_eq!(
            expected_device(mock_uri, mock_uuid, Some(mock_ip_and_mac)),
            instance
        );
    }

    #[tokio::test]
    async fn test_apply_filters_exclude_uuid_exist_different_letter_cases() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mock_uri = "device_uri";
        let mock_uuid = "device_uuid";

        let mock = MockOnvifQuery::new();
        let onvif_config = OnvifDiscoveryDetails {
            ip_addresses: None,
            mac_addresses: None,
            scopes: None,
            uuids: Some(FilterList {
                action: FilterType::Exclude,
                items: vec![mock_uuid.to_uppercase()],
            }),
            discovery_timeout_seconds: 1,
        };
        assert!(apply_filters(&onvif_config, mock_uri, mock_uuid, &mock)
            .await
            .is_none());
    }
}
