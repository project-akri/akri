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
    filtering::{FilterList, FilterType},
};
use async_trait::async_trait;
use log::{error, info, trace};
use std::{collections::HashMap, time::Duration};
use tokio::{sync::mpsc, time::delay_for};
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

/// `DiscoveryHandlerImpl` discovers the onvif instances as described by the filters `discover_handler_config.ip_addresses`,
/// `discover_handler_config.mac_addresses`, and `discover_handler_config.scopes`.
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
        let (mut discovered_devices_sender, discovered_devices_receiver) =
            mpsc::channel(DISCOVERED_DEVICES_CHANNEL_CAPACITY);
        let discovery_handler_config: OnvifDiscoveryDetails =
            deserialize_discovery_details(&discover_request.discovery_details)
                .map_err(|e| tonic::Status::new(tonic::Code::InvalidArgument, format!("{}", e)))?;
        let mut cameras: Vec<Device> = Vec::new();
        tokio::spawn(async move {
            loop {
                let onvif_query = OnvifQueryImpl {};

                trace!("discover - filters:{:?}", &discovery_handler_config,);
                let discovered_onvif_cameras = util::simple_onvif_discover(Duration::from_secs(
                    discovery_handler_config.discovery_timeout_seconds as u64,
                ))
                .await
                .unwrap();
                trace!("discover - discovered:{:?}", &discovered_onvif_cameras,);
                // apply_filters never returns an error -- safe to unwrap
                let filtered_onvif_cameras = apply_filters(
                    &discovery_handler_config,
                    discovered_onvif_cameras,
                    &onvif_query,
                )
                .await
                .unwrap();
                trace!("discover - filtered:{:?}", &filtered_onvif_cameras);
                let mut changed_camera_list = false;
                let mut matching_camera_count = 0;
                filtered_onvif_cameras.iter().for_each(|camera| {
                    if !cameras.contains(camera) {
                        changed_camera_list = true;
                    } else {
                        matching_camera_count += 1;
                    }
                });
                if changed_camera_list || matching_camera_count != cameras.len() {
                    trace!("discover - sending updated device list");
                    cameras = filtered_onvif_cameras.clone();
                    if let Err(e) = discovered_devices_sender
                        .send(Ok(DiscoverResponse {
                            devices: filtered_onvif_cameras,
                        }))
                        .await
                    {
                        error!(
                            "discover - for ONVIF failed to send discovery response with error {}",
                            e
                        );
                        if let Some(mut sender) = register_sender {
                            sender.send(()).await.unwrap();
                        }
                        break;
                    }
                }
                delay_for(Duration::from_secs(DISCOVERY_INTERVAL_SECS)).await;
            }
        });
        Ok(Response::new(discovered_devices_receiver))
    }
}

fn execute_filter(filter_list: Option<&FilterList>, filter_against: &[String]) -> bool {
    if filter_list.is_none() {
        return false;
    }
    let filter_action = filter_list.as_ref().unwrap().action.clone();
    let filter_count = filter_list
        .unwrap()
        .items
        .iter()
        .filter(|pattern| {
            filter_against
                .iter()
                .filter(|filter_against_item| filter_against_item.contains(*pattern))
                .count()
                > 0
        })
        .count();

    if FilterType::Include == filter_action {
        filter_count == 0
    } else {
        filter_count != 0
    }
}

async fn apply_filters(
    discovery_handler_config: &OnvifDiscoveryDetails,
    device_service_uris: Vec<String>,
    onvif_query: &impl OnvifQuery,
) -> Result<Vec<Device>, anyhow::Error> {
    let mut result = Vec::new();
    for device_service_url in device_service_uris.iter() {
        trace!("apply_filters - device service url {}", &device_service_url);
        let (ip_address, mac_address) = match onvif_query
            .get_device_ip_and_mac_address(&device_service_url)
            .await
        {
            Ok(ip_and_mac) => ip_and_mac,
            Err(e) => {
                error!("apply_filters - error getting ip and mac address: {}", e);
                continue;
            }
        };

        // Evaluate camera ip address against ip filter if provided
        let ip_address_as_vec = vec![ip_address.clone()];
        if execute_filter(
            discovery_handler_config.ip_addresses.as_ref(),
            &ip_address_as_vec,
        ) {
            continue;
        }

        // Evaluate camera mac address against mac filter if provided
        let mac_address_as_vec = vec![mac_address.clone()];
        if execute_filter(
            discovery_handler_config.mac_addresses.as_ref(),
            &mac_address_as_vec,
        ) {
            continue;
        }

        let ip_and_mac_joined = format!("{}-{}", &ip_address, &mac_address);

        // Evaluate camera scopes against scopes filter if provided
        let device_scopes = match onvif_query.get_device_scopes(&device_service_url).await {
            Ok(scopes) => scopes,
            Err(e) => {
                error!("apply_filters - error getting scopes: {}", e);
                continue;
            }
        };
        if execute_filter(discovery_handler_config.scopes.as_ref(), &device_scopes) {
            continue;
        }

        let mut properties = HashMap::new();
        properties.insert(
            ONVIF_DEVICE_SERVICE_URL_LABEL_ID.to_string(),
            device_service_url.to_string(),
        );
        properties.insert(ONVIF_DEVICE_IP_ADDRESS_LABEL_ID.into(), ip_address);
        properties.insert(ONVIF_DEVICE_MAC_ADDRESS_LABEL_ID.into(), mac_address);

        trace!(
            "apply_filters - returns DiscoveryResult ip/mac: {:?}, props: {:?}",
            &ip_and_mac_joined,
            &properties
        );
        result.push(Device {
            id: ip_and_mac_joined,
            properties,
            mounts: Vec::default(),
            device_specs: Vec::default(),
        })
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::super::discovery_utils::MockOnvifQuery;
    use super::*;

    struct IpAndMac {
        mock_uri: &'static str,
        mock_ip: &'static str,
        mock_mac: &'static str,
    }

    struct Scope {
        mock_uri: &'static str,
        mock_scope: &'static str,
    }

    fn configure_scenario(
        mock: &mut MockOnvifQuery,
        ip_and_mac: Option<IpAndMac>,
        scope: Option<Scope>,
    ) {
        if let Some(ip_and_mac_) = ip_and_mac {
            configure_get_device_ip_and_mac_address(
                mock,
                &ip_and_mac_.mock_uri,
                &ip_and_mac_.mock_ip,
                &ip_and_mac_.mock_mac,
            )
        }
        if let Some(scope_) = scope {
            configure_get_device_scopes(mock, &scope_.mock_uri, &scope_.mock_scope)
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

    fn configure_get_device_scopes(
        mock: &mut MockOnvifQuery,
        uri: &'static str,
        scope: &'static str,
    ) {
        mock.expect_get_device_scopes()
            .times(1)
            .withf(move |u| u == uri)
            .returning(move |_| Ok(vec![scope.to_string()]));
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

        let mut mock = MockOnvifQuery::new();
        configure_scenario(
            &mut mock,
            Some(IpAndMac {
                mock_uri: "device_uri",
                mock_ip: "mock.ip",
                mock_mac: "mock:mac",
            }),
            Some(Scope {
                mock_uri: "device_uri",
                mock_scope: "mock.scope",
            }),
        );

        let onvif_config = OnvifDiscoveryDetails {
            ip_addresses: None,
            mac_addresses: None,
            scopes: None,
            discovery_timeout_seconds: 1,
        };
        let instances = apply_filters(&onvif_config, vec![mock_uri.to_string()], &mock)
            .await
            .unwrap();

        assert_eq!(1, instances.len());
    }

    #[tokio::test]
    async fn test_apply_filters_include_ip_exist() {
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
            Some(Scope {
                mock_uri,
                mock_scope: "mock.scope",
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
        let instances = apply_filters(&onvif_config, vec![mock_uri.to_string()], &mock)
            .await
            .unwrap();

        assert_eq!(1, instances.len());
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
            None,
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
        let instances = apply_filters(&onvif_config, vec![mock_uri.to_string()], &mock)
            .await
            .unwrap();

        assert_eq!(0, instances.len());
    }

    #[tokio::test]
    async fn test_apply_filters_exclude_ip_nonexist() {
        let mock_uri = "device_uri";

        let mut mock = MockOnvifQuery::new();
        configure_scenario(
            &mut mock,
            Some(IpAndMac {
                mock_uri,
                mock_ip: "mock.ip",
                mock_mac: "mock:mac",
            }),
            Some(Scope {
                mock_uri,
                mock_scope: "mock.scope",
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
        let instances = apply_filters(&onvif_config, vec![mock_uri.to_string()], &mock)
            .await
            .unwrap();

        assert_eq!(1, instances.len());
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
            None,
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
        let instances = apply_filters(&onvif_config, vec![mock_uri.to_string()], &mock)
            .await
            .unwrap();

        assert_eq!(0, instances.len());
    }

    #[tokio::test]
    async fn test_apply_filters_include_mac_exist() {
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
            Some(Scope {
                mock_uri,
                mock_scope: "mock.scope",
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
        let instances = apply_filters(&onvif_config, vec![mock_uri.to_string()], &mock)
            .await
            .unwrap();

        assert_eq!(1, instances.len());
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
            None,
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
        let instances = apply_filters(&onvif_config, vec![mock_uri.to_string()], &mock)
            .await
            .unwrap();

        assert_eq!(0, instances.len());
    }

    #[tokio::test]
    async fn test_apply_filters_exclude_mac_nonexist() {
        let mock_uri = "device_uri";

        let mut mock = MockOnvifQuery::new();
        configure_scenario(
            &mut mock,
            Some(IpAndMac {
                mock_uri,
                mock_ip: "mock.ip",
                mock_mac: "mock:mac",
            }),
            Some(Scope {
                mock_uri,
                mock_scope: "mock.scope",
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
        let instances = apply_filters(&onvif_config, vec![mock_uri.to_string()], &mock)
            .await
            .unwrap();

        assert_eq!(1, instances.len());
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
            None,
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
        let instances = apply_filters(&onvif_config, vec![mock_uri.to_string()], &mock)
            .await
            .unwrap();

        assert_eq!(0, instances.len());
    }
}
