use super::{discovery_impl::do_standard_discovery, OPCUA_DISCOVERY_URL_LABEL};
use akri_discovery_utils::{
    discovery::{
        v0::{discovery_server::Discovery, Device, DiscoverRequest, DiscoverResponse},
        DiscoverStream,
    },
    filtering::FilterList,
};
use anyhow::Error;
use async_trait::async_trait;
use log::{error, info, trace};
use std::{collections::HashMap, time::Duration};
use tokio::sync::mpsc;
use tokio::time::delay_for;
use tonic::{Response, Status};

// TODO: make this configurable
pub const DISCOVERY_INTERVAL_SECS: u64 = 10;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub enum DiscoveryHandlerType {
    Opcua(OpcuaDiscoveryHandlerConfig),
}

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
pub struct OpcuaDiscoveryHandlerConfig {
    pub opcua_discovery_method: OpcuaDiscoveryMethod,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub application_names: Option<FilterList>,
}

/// `DiscoveryHandler` discovers udev instances by parsing the udev rules in `discovery_handler_config.udev_rules`.
/// The instances it discovers are always unshared.
pub struct DiscoveryHandler {
    register_sender: Option<tokio::sync::mpsc::Sender<()>>,
}

impl DiscoveryHandler {
    pub fn new(register_sender: Option<tokio::sync::mpsc::Sender<()>>) -> Self {
        DiscoveryHandler { register_sender }
    }
}

#[async_trait]
impl Discovery for DiscoveryHandler {
    type DiscoverStream = DiscoverStream;
    async fn discover(
        &self,
        request: tonic::Request<DiscoverRequest>,
    ) -> Result<Response<Self::DiscoverStream>, Status> {
        info!("discover - called for OPC UA protocol");
        let register_sender = self.register_sender.clone();
        let discover_request = request.get_ref();
        let (mut tx, rx) = mpsc::channel(4);
        let discovery_handler_config =
            deserialize_discovery_details(&discover_request.discovery_details).map_err(|e| {
                tonic::Status::new(
                    tonic::Code::InvalidArgument,
                    format!("Invalid OPC UA discovery handler configuration: {}", e),
                )
            })?;
        let mut previously_discovered_devices: Vec<Device> = Vec::new();
        tokio::spawn(async move {
            let discovery_method = discovery_handler_config.opcua_discovery_method.clone();
            let application_names = discovery_handler_config.application_names.clone();
            loop {
                let discovery_urls: Vec<String> = match discovery_method.clone() {
                    OpcuaDiscoveryMethod::Standard(standard_opcua_discovery) => {
                        do_standard_discovery(
                            standard_opcua_discovery.discovery_urls.clone(),
                            application_names.clone(),
                        )
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
                    if let Err(e) = tx
                        .send(Ok(DiscoverResponse {
                            devices: discovered_devices,
                        }))
                        .await
                    {
                        error!(
                            "discover - for OPC UA failed to send discovery response with error {}",
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
        Ok(Response::new(rx))
    }
}

/// This obtains the `OpcuaDiscoveryHandlerConfig` from a discovery details map.
/// It expects the `OpcuaDiscoveryHandlerConfig` to be serialized yaml stored in the map as
/// the String value associated with the key `protocolHandler`.
fn deserialize_discovery_details(
    discovery_details: &HashMap<String, String>,
) -> Result<OpcuaDiscoveryHandlerConfig, Error> {
    info!(
        "inner_get_discovery_handler - for discovery details {:?}",
        discovery_details
    );
    // Determine whether it is an embedded protocol
    if let Some(discovery_handler_str) = discovery_details.get("protocolHandler") {
        if let Ok(discovery_handler) = serde_yaml::from_str(discovery_handler_str) {
            match discovery_handler {
                DiscoveryHandlerType::Opcua(discovery_handler_config) => {
                    Ok(discovery_handler_config)
                }
            }
        } else {
            Err(anyhow::format_err!("Discovery details had protocol handler but does not have embedded support. Discovery details: {:?}", discovery_details))
        }
    } else {
        Err(anyhow::format_err!(
            "Generic discovery handlers not supported. Discovery details: {:?}",
            discovery_details
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_discovery_details_empty() {
        // Check that if no DiscoveryUrls are provided, the default LDS url is used.
        let yaml = r#"
          protocolHandler: |+
            opcua:
              opcuaDiscoveryMethod: 
                standard: {}
        "#;
        let deserialized: HashMap<String, String> = serde_yaml::from_str(&yaml).unwrap();
        let serialized =
            serde_json::to_string(&deserialize_discovery_details(&deserialized).unwrap()).unwrap();
        let expected_deserialized = r#"{"opcuaDiscoveryMethod":{"standard":{"discoveryUrls":["opc.tcp://localhost:4840/"]}}}"#;
        assert_eq!(expected_deserialized, serialized);
    }

    #[test]
    fn test_deserialize_discovery_details_detailed() {
        // Test standard discovery
        let yaml = r#"
          protocolHandler: |+
            opcua:
              opcuaDiscoveryMethod: 
                standard:
                  discoveryUrls:
                  - opc.tcp://127.0.0.1:4855/
              applicationNames:
                action: Include
                items: 
                - "Some application name" 
        "#;
        let deserialized: HashMap<String, String> = serde_yaml::from_str(&yaml).unwrap();
        let serialized =
            serde_json::to_string(&deserialize_discovery_details(&deserialized).unwrap()).unwrap();
        let expected_deserialized = r#"{"opcuaDiscoveryMethod":{"standard":{"discoveryUrls":["opc.tcp://127.0.0.1:4855/"]}},"applicationNames":{"items":["Some application name"],"action":"Include"}}"#;
        assert_eq!(expected_deserialized, serialized);
    }
}
