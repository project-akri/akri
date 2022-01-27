use super::discovery_impl::util;
use super::discovery_utils::{
    OnvifQuery, OnvifQueryImpl, ONVIF_DEVICE_IP_ADDRESS_LABEL_ID,
    ONVIF_DEVICE_MAC_ADDRESS_LABEL_ID, ONVIF_DEVICE_SERVICE_URL_LABEL_ID,
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
        tokio::spawn(async move {
            let mut previous_cameras = Vec::new();
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
                let onvif_query = OnvifQueryImpl {};

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
                previous_cameras.iter().for_each(|c| {
                    if !latest_cameras.contains(c) {
                        changed_camera_list = true;
                        filtered_camera_devices.remove(c);
                    }
                });

                let futures: Vec<_> = latest_cameras
                    .iter()
                    .filter(|c| !previous_cameras.contains(c))
                    .map(|c| apply_filters(&discovery_handler_config, c, &onvif_query))
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
    onvif_query: &impl OnvifQuery,
) -> Option<(String, Device)> {
    info!("apply_filters - device service url {}", device_service_uri);
    let (ip_address, mac_address) = match onvif_query
        .get_device_ip_and_mac_address(device_service_uri)
        .await
    {
        Ok(ip_and_mac) => ip_and_mac,
        Err(e) => {
            error!("apply_filters - error getting ip and mac address: {}", e);
            return None;
        }
    };
    // Evaluate camera ip address against ip filter if provided
    let ip_address_as_vec = vec![ip_address.clone()];
    if util::execute_filter(
        discovery_handler_config.ip_addresses.as_ref(),
        &ip_address_as_vec,
    ) {
        return None;
    }

    // Evaluate camera mac address against mac filter if provided
    let mac_address_as_vec = vec![mac_address.clone()];
    if util::execute_filter(
        discovery_handler_config.mac_addresses.as_ref(),
        &mac_address_as_vec,
    ) {
        return None;
    }

    let ip_and_mac_joined = format!("{}-{}", &ip_address, &mac_address);
    let mut properties = HashMap::new();
    properties.insert(
        ONVIF_DEVICE_SERVICE_URL_LABEL_ID.to_string(),
        device_service_uri.to_string(),
    );
    properties.insert(ONVIF_DEVICE_IP_ADDRESS_LABEL_ID.into(), ip_address);
    properties.insert(ONVIF_DEVICE_MAC_ADDRESS_LABEL_ID.into(), mac_address);

    Some((
        device_service_uri.to_string(),
        Device {
            id: ip_and_mac_joined,
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

    struct IpAndMac {
        mock_uri: &'static str,
        mock_ip: &'static str,
        mock_mac: &'static str,
    }

    fn configure_scenario(mock: &mut MockOnvifQuery, ip_and_mac: Option<IpAndMac>) {
        if let Some(ip_and_mac_) = ip_and_mac {
            configure_get_device_ip_and_mac_address(
                mock,
                ip_and_mac_.mock_uri,
                ip_and_mac_.mock_ip,
                ip_and_mac_.mock_mac,
            )
        }
    }

    fn configure_get_device_ip_and_mac_address(
        mock: &mut MockOnvifQuery,
        uri: &'static str,
        ip: &'static str,
        mac: &'static str,
    ) {
        mock.expect_get_device_ip_and_mac_address()
            .times(1)
            .withf(move |u| u == uri)
            .returning(move |_| Ok((ip.to_string(), mac.to_string())));
    }

    fn expected_device(uri: &str, ip: &str, mac: &str) -> (String, Device) {
        let mut properties = HashMap::new();
        properties.insert(
            ONVIF_DEVICE_SERVICE_URL_LABEL_ID.to_string(),
            uri.to_string(),
        );

        properties.insert(ONVIF_DEVICE_IP_ADDRESS_LABEL_ID.into(), ip.to_string());
        properties.insert(ONVIF_DEVICE_MAC_ADDRESS_LABEL_ID.into(), mac.to_string());

        (
            uri.to_string(),
            Device {
                id: format!("{}-{}", ip, mac),
                properties,
                mounts: Vec::default(),
                device_specs: Vec::default(),
            },
        )
    }

    #[test]
    fn test_deserialize_discovery_details() {
        let dh_config: OnvifDiscoveryDetails = deserialize_discovery_details("{}").unwrap();
        let serialized = serde_json::to_string(&dh_config).unwrap();
        let expected_deserialized = r#"{"discoveryTimeoutSeconds":1}"#;
        assert_eq!(expected_deserialized, serialized);
    }

    #[tokio::test]
    async fn test_apply_filters_no_filters() {
        let mock_uri = "device_uri";
        let mock_ip = "mock.ip";
        let mock_mac = "mock:mac";

        let mut mock = MockOnvifQuery::new();
        configure_scenario(
            &mut mock,
            Some(IpAndMac {
                mock_uri,
                mock_ip,
                mock_mac,
            }),
        );

        let onvif_config = OnvifDiscoveryDetails {
            ip_addresses: None,
            mac_addresses: None,
            scopes: None,
            discovery_timeout_seconds: 1,
        };
        let instance = apply_filters(&onvif_config, mock_uri, &mock).await.unwrap();

        assert_eq!(expected_device(mock_uri, mock_ip, mock_mac), instance);
    }

    #[tokio::test]
    async fn test_apply_filters_include_ip_exist() {
        let mock_uri = "device_uri";
        let mock_ip = "mock.ip";
        let mock_mac = "mock:mac";

        let mut mock = MockOnvifQuery::new();
        configure_scenario(
            &mut mock,
            Some(IpAndMac {
                mock_uri,
                mock_ip,
                mock_mac,
            }),
        );

        let onvif_config = OnvifDiscoveryDetails {
            ip_addresses: Some(FilterList {
                action: FilterType::Include,
                items: vec![mock_ip.to_string()],
            }),
            mac_addresses: None,
            scopes: None,
            discovery_timeout_seconds: 1,
        };
        let instance = apply_filters(&onvif_config, mock_uri, &mock).await.unwrap();

        assert_eq!(expected_device(mock_uri, mock_ip, mock_mac), instance);
    }

    #[tokio::test]
    async fn test_apply_filters_include_ip_nonexist() {
        let mock_uri = "device_uri";

        let mut mock = MockOnvifQuery::new();
        configure_scenario(
            &mut mock,
            Some(IpAndMac {
                mock_uri,
                mock_ip: "mock.ip",
                mock_mac: "mock:mac",
            }),
        );

        let onvif_config = OnvifDiscoveryDetails {
            ip_addresses: Some(FilterList {
                action: FilterType::Include,
                items: vec!["nonexist.ip".to_string()],
            }),
            mac_addresses: None,
            scopes: None,
            discovery_timeout_seconds: 1,
        };
        assert!(apply_filters(&onvif_config, mock_uri, &mock)
            .await
            .is_none());
    }

    #[tokio::test]
    async fn test_apply_filters_exclude_ip_nonexist() {
        let mock_uri = "device_uri";
        let mock_ip = "mock.ip";
        let mock_mac = "mock:mac";

        let mut mock = MockOnvifQuery::new();
        configure_scenario(
            &mut mock,
            Some(IpAndMac {
                mock_uri,
                mock_ip,
                mock_mac,
            }),
        );

        let onvif_config = OnvifDiscoveryDetails {
            ip_addresses: Some(FilterList {
                action: FilterType::Exclude,
                items: vec!["nonexist.ip".to_string()],
            }),
            mac_addresses: None,
            scopes: None,
            discovery_timeout_seconds: 1,
        };
        let instance = apply_filters(&onvif_config, mock_uri, &mock).await.unwrap();

        assert_eq!(expected_device(mock_uri, mock_ip, mock_mac), instance);
    }

    #[tokio::test]
    async fn test_apply_filters_exclude_ip_exist() {
        let mock_uri = "device_uri";
        let mock_ip = "mock.ip";

        let mut mock = MockOnvifQuery::new();
        configure_scenario(
            &mut mock,
            Some(IpAndMac {
                mock_uri,
                mock_ip,
                mock_mac: "mock:mac",
            }),
        );

        let onvif_config = OnvifDiscoveryDetails {
            ip_addresses: Some(FilterList {
                action: FilterType::Exclude,
                items: vec![mock_ip.to_string()],
            }),
            mac_addresses: None,
            scopes: None,
            discovery_timeout_seconds: 1,
        };
        assert!(apply_filters(&onvif_config, mock_uri, &mock)
            .await
            .is_none());
    }

    #[tokio::test]
    async fn test_apply_filters_include_mac_exist() {
        let mock_uri = "device_uri";
        let mock_ip = "mock.ip";
        let mock_mac = "mock:mac";

        let mut mock = MockOnvifQuery::new();
        configure_scenario(
            &mut mock,
            Some(IpAndMac {
                mock_uri,
                mock_ip,
                mock_mac,
            }),
        );

        let onvif_config = OnvifDiscoveryDetails {
            ip_addresses: None,
            mac_addresses: Some(FilterList {
                action: FilterType::Include,
                items: vec![mock_mac.to_string()],
            }),
            scopes: None,
            discovery_timeout_seconds: 1,
        };
        let instance = apply_filters(&onvif_config, mock_uri, &mock).await.unwrap();

        assert_eq!(expected_device(mock_uri, mock_ip, mock_mac), instance);
    }

    #[tokio::test]
    async fn test_apply_filters_include_mac_nonexist() {
        let mock_uri = "device_uri";

        let mut mock = MockOnvifQuery::new();
        configure_scenario(
            &mut mock,
            Some(IpAndMac {
                mock_uri,
                mock_ip: "mock.ip",
                mock_mac: "mock:mac",
            }),
        );

        let onvif_config = OnvifDiscoveryDetails {
            ip_addresses: None,
            mac_addresses: Some(FilterList {
                action: FilterType::Include,
                items: vec!["nonexist:mac".to_string()],
            }),
            scopes: None,
            discovery_timeout_seconds: 1,
        };
        assert!(apply_filters(&onvif_config, mock_uri, &mock)
            .await
            .is_none());
    }

    #[tokio::test]
    async fn test_apply_filters_exclude_mac_nonexist() {
        let mock_uri = "device_uri";
        let mock_ip = "mock.ip";
        let mock_mac = "mock:mac";

        let mut mock = MockOnvifQuery::new();
        configure_scenario(
            &mut mock,
            Some(IpAndMac {
                mock_uri,
                mock_ip: "mock.ip",
                mock_mac: "mock:mac",
            }),
        );

        let onvif_config = OnvifDiscoveryDetails {
            ip_addresses: None,
            mac_addresses: Some(FilterList {
                action: FilterType::Exclude,
                items: vec!["nonexist:mac".to_string()],
            }),
            scopes: None,
            discovery_timeout_seconds: 1,
        };
        let instance = apply_filters(&onvif_config, mock_uri, &mock).await.unwrap();

        assert_eq!(expected_device(mock_uri, mock_ip, mock_mac), instance);
    }

    #[tokio::test]
    async fn test_apply_filters_exclude_mac_exist() {
        let mock_uri = "device_uri";
        let mock_mac = "mock:mac";

        let mut mock = MockOnvifQuery::new();
        configure_scenario(
            &mut mock,
            Some(IpAndMac {
                mock_uri,
                mock_ip: "mock.ip",
                mock_mac,
            }),
        );

        let onvif_config = OnvifDiscoveryDetails {
            ip_addresses: None,
            mac_addresses: Some(FilterList {
                action: FilterType::Exclude,
                items: vec![mock_mac.to_string()],
            }),
            scopes: None,
            discovery_timeout_seconds: 1,
        };
        assert!(apply_filters(&onvif_config, mock_uri, &mock)
            .await
            .is_none());
    }
}
