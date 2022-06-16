use super::{discovery_impl::do_standard_discovery, OPCUA_DISCOVERY_URL_LABEL};
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
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tonic::{Response, Status};

// TODO: make this configurable
pub const DISCOVERY_INTERVAL_SECS: u64 = 10;

/// Methods for discovering OPC UA Servers
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub enum OpcuaDiscoveryMethod {
    Standard(StandardOpcuaDiscovery),
    // TODO: add scan
}

/// Discovers OPC UA Servers and/or LocalDiscoveryServers at specified DiscoveryURLs.
/// If the DiscoveryURL is for a LocalDiscoveryServer, it will discover all Servers
/// that have registered with that LocalDiscoveryServer.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct StandardOpcuaDiscovery {
    #[serde(default = "lds_discovery_url", skip_serializing_if = "Vec::is_empty")]
    pub discovery_urls: Vec<String>,
}

/// If no DiscoveryURLs are specified, uses the OPC UA default DiscoveryURL
/// for the LocalDiscoveryServer running on the host
fn lds_discovery_url() -> Vec<String> {
    vec!["opc.tcp://localhost:4840/".to_string()]
}

/// This defines the OPC UA data stored in the Configuration
/// CRD
///
/// The OPC UA discovery handler is designed to support multiple methods
/// for discovering OPC UA servers and stores a filter list for
/// application names.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct OpcuaDiscoveryDetails {
    pub opcua_discovery_method: OpcuaDiscoveryMethod,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub application_names: Option<FilterList>,
}

/// `DiscoveryHandlerImpl` discovers udev instances by parsing the udev rules in `discovery_handler_config.udev_rules`.
/// The instances it discovers are always unshared.
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
        info!("discover - called for OPC UA protocol");
        let register_sender = self.register_sender.clone();
        let discover_request = request.get_ref();
        let (discovered_devices_sender, discovered_devices_receiver) =
            mpsc::channel(DISCOVERED_DEVICES_CHANNEL_CAPACITY);
        let discovery_handler_config: OpcuaDiscoveryDetails =
            deserialize_discovery_details(&discover_request.discovery_details)
                .map_err(|e| tonic::Status::new(tonic::Code::InvalidArgument, format!("{}", e)))?;
        let mut previously_discovered_devices: Vec<Device> = Vec::new();
        tokio::spawn(async move {
            let discovery_method = discovery_handler_config.opcua_discovery_method.clone();
            let application_names = discovery_handler_config.application_names.clone();
            loop {
                // Before each iteration, check if receiver has dropped
                if discovered_devices_sender.is_closed() {
                    error!("discover - channel closed ... attempting to re-register with Agent");
                    if let Some(sender) = register_sender {
                        sender.send(()).await.unwrap();
                    }
                    break;
                }

                let discovery_urls: Vec<String> = match discovery_method.clone() {
                    OpcuaDiscoveryMethod::Standard(standard_opcua_discovery) => {
                        let discovery_urls = standard_opcua_discovery.discovery_urls.clone();
                        let application_names = application_names.clone();
                        tokio::task::spawn_blocking(move || {
                            do_standard_discovery(discovery_urls, application_names)
                        })
                        .await
                        .unwrap()
                    } // No other discovery methods implemented yet
                };

                // Build DiscoveryResult for each server discovered
                let discovered_devices = discovery_urls
                    .into_iter()
                    .map(|discovery_url| {
                        let mut properties = std::collections::HashMap::new();
                        trace!(
                            "discover - found OPC UA server at DiscoveryURL {}",
                            discovery_url
                        );
                        properties
                            .insert(OPCUA_DISCOVERY_URL_LABEL.to_string(), discovery_url.clone());
                        Device {
                            id: discovery_url,
                            properties,
                            mounts: Vec::default(),
                            device_specs: Vec::default(),
                        }
                    })
                    .collect::<Vec<Device>>();
                let mut changed_device_list = false;
                let mut matching_device_count = 0;
                discovered_devices.iter().for_each(|device| {
                    if !previously_discovered_devices.contains(device) {
                        changed_device_list = true;
                    } else {
                        matching_device_count += 1;
                    }
                });
                if changed_device_list
                    || matching_device_count != previously_discovered_devices.len()
                {
                    trace!("discover - for OPC UA, sending updated device list");
                    previously_discovered_devices = discovered_devices.clone();
                    if let Err(e) = discovered_devices_sender
                        .send(Ok(DiscoverResponse {
                            devices: discovered_devices,
                        }))
                        .await
                    {
                        error!(
                            "discover - for OPC UA failed to send discovery response with error {}",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_discovery_details_empty() {
        // Check that if no DiscoveryUrls are provided, the default LDS url is used.
        let yaml = r#"
            opcuaDiscoveryMethod: 
              standard: {}
        "#;
        let dh_config: OpcuaDiscoveryDetails = deserialize_discovery_details(yaml).unwrap();
        let serialized = serde_json::to_string(&dh_config).unwrap();
        let expected_deserialized = r#"{"opcuaDiscoveryMethod":{"standard":{"discoveryUrls":["opc.tcp://localhost:4840/"]}}}"#;
        assert_eq!(expected_deserialized, serialized);
    }

    #[test]
    fn test_deserialize_discovery_details_detailed() {
        // Test standard discovery
        let yaml = r#"
            opcuaDiscoveryMethod: 
              standard:
                discoveryUrls:
                - opc.tcp://127.0.0.1:4855/
            applicationNames:
              action: Include
              items: 
              - "Some application name" 
        "#;
        let dh_config: OpcuaDiscoveryDetails = deserialize_discovery_details(yaml).unwrap();
        let serialized = serde_json::to_string(&dh_config).unwrap();
        let expected_serialized = r#"{"opcuaDiscoveryMethod":{"standard":{"discoveryUrls":["opc.tcp://127.0.0.1:4855/"]}},"applicationNames":{"items":["Some application name"],"action":"Include"}}"#;
        assert_eq!(expected_serialized, serialized);
    }
}
