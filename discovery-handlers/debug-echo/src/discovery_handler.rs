use akri_discovery_utils::discovery::{
    v0::{discovery_server::Discovery, Device, DiscoverRequest, DiscoverResponse},
    DiscoverStream,
};
use async_trait::async_trait;
use log::{error, info, trace};
use std::time::Duration;
use std::{collections::HashMap, fs};
use tokio::sync::mpsc;
use tokio::time::delay_for;
use tonic::{Response, Status};

/// Protocol name that debugEcho discovery handlers use when registering with the Agent
pub const PROTOCOL_NAME: &str = "debugEcho";
/// Endpoint for the debugEcho discovery services
pub const DISCOVERY_PORT: &str = "10001";
// TODO: make this configurable
pub const DISCOVERY_INTERVAL_SECS: u64 = 10;

/// File acting as an environment variable for testing discovery.
/// To mimic an instance going offline, kubectl exec into the pod running this discovery handler
/// and echo "OFFLINE" > /tmp/debug-echo-availability.txt.
/// To mimic a device coming back online, remove the word "OFFLINE" from the file
/// ie: echo "" > /tmp/debug-echo-availability.txt.
pub const DEBUG_ECHO_AVAILABILITY_CHECK_PATH: &str = "/tmp/debug-echo-availability.txt";
/// String to write into DEBUG_ECHO_AVAILABILITY_CHECK_PATH to make Other devices undiscoverable
pub const OFFLINE: &str = "OFFLINE";

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub enum DiscoveryHandlerType {
    DebugEcho(DebugEchoDiscoveryHandlerConfig),
}

/// DebugEchoDiscoveryHandlerConfig describes the necessary information needed to discover and filter debug echo devices.
/// Specifically, it contains a list (`descriptions`) of fake devices to be discovered.
/// This information is expected to be serialized in the discovery details map sent during Discover requests.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DebugEchoDiscoveryHandlerConfig {
    pub descriptions: Vec<String>,
}

/// The DiscoveryHandler discovers a list of devices, named in its `descriptions`.
/// It mocks discovering the devices by inspecting the contents of the file at `DEBUG_ECHO_AVAILABILITY_CHECK_PATH`.
/// If the file contains "OFFLINE", it won't discover any of the devices, else it discovers them all.
pub struct DiscoveryHandler {
    shutdown_sender: Option<tokio::sync::mpsc::Sender<()>>,
}

impl DiscoveryHandler {
    pub fn new(shutdown_sender: Option<tokio::sync::mpsc::Sender<()>>) -> Self {
        DiscoveryHandler { shutdown_sender }
    }
}

#[async_trait]
impl Discovery for DiscoveryHandler {
    type DiscoverStream = DiscoverStream;
    async fn discover(
        &self,
        request: tonic::Request<DiscoverRequest>,
    ) -> Result<Response<Self::DiscoverStream>, Status> {
        info!("discover - called for debug echo protocol");
        let shutdown_sender = self.shutdown_sender.clone();
        let discover_request = request.get_ref();
        let (mut tx, rx) = mpsc::channel(4);
        let discovery_handler_config =
            deserialize_discovery_details(&discover_request.discovery_details).map_err(|e| {
                tonic::Status::new(
                    tonic::Code::InvalidArgument,
                    format!("Invalid debugEcho discovery handler configuration: {}", e),
                )
            })?;
        let descriptions = discovery_handler_config.descriptions;
        let mut offline = fs::read_to_string(DEBUG_ECHO_AVAILABILITY_CHECK_PATH)
            .unwrap_or_default()
            .contains(OFFLINE);
        let mut first_loop = true;
        tokio::spawn(async move {
            loop {
                let availability =
                    fs::read_to_string(DEBUG_ECHO_AVAILABILITY_CHECK_PATH).unwrap_or_default();
                trace!(
                    "discover -- debugEcho devices are online? {}",
                    !availability.contains(OFFLINE)
                );
                if (availability.contains(OFFLINE) && !offline) || offline && first_loop {
                    if first_loop {
                        first_loop = false;
                    }
                    // If the device is now offline, return an empty list of instance info
                    offline = true;
                    if let Err(e) = tx
                        .send(Ok(DiscoverResponse {
                            devices: Vec::new(),
                        }))
                        .await
                    {
                        error!("discover - for debugEcho failed to send discovery response with error {}", e);
                        if let Some(mut sender) = shutdown_sender {
                            sender.send(()).await.unwrap();
                        }
                        break;
                    }
                } else if (!availability.contains(OFFLINE) && offline) || !offline && first_loop {
                    if first_loop {
                        first_loop = false;
                    }
                    offline = false;
                    let devices = descriptions
                        .iter()
                        .map(|description| Device {
                            id: description.clone(),
                            properties: HashMap::new(),
                            mounts: Vec::default(),
                            device_specs: Vec::default(),
                        })
                        .collect::<Vec<Device>>();
                    if let Err(e) = tx.send(Ok(DiscoverResponse { devices })).await {
                        // TODO: consider re-registering here
                        error!("discover - for debugEcho failed to send discovery response with error {}", e);
                        if let Some(mut sender) = shutdown_sender {
                            sender.send(()).await.unwrap();
                        }
                        break;
                    }
                }
                delay_for(Duration::from_secs(DISCOVERY_INTERVAL_SECS)).await;
            }
        });
        trace!("outside of thread");
        Ok(Response::new(rx))
    }
}

/// deserialize_discovery_details obtains the `DebugEchoDiscoveryHandlerConfig` from a discovery details map.
/// It expects the `DebugEchoDiscoveryHandlerConfig` to be serialized yaml stored in the map as
/// the String value associated with the key `protocolHandler`.
fn deserialize_discovery_details(
    discovery_details: &HashMap<String, String>,
) -> Result<DebugEchoDiscoveryHandlerConfig, anyhow::Error> {
    trace!(
        "inner_get_discovery_handler - for discovery details {:?}",
        discovery_details
    );
    // Determine whether it is an embedded protocol
    if let Some(discovery_handler_str) = discovery_details.get("protocolHandler") {
        trace!("protocol handler {:?}", discovery_handler_str);
        if let Ok(discovery_handler) = serde_yaml::from_str(discovery_handler_str) {
            match discovery_handler {
                DiscoveryHandlerType::DebugEcho(debug_echo_discovery_handler_config) => {
                    Ok(debug_echo_discovery_handler_config)
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
        let yaml = r#"
          protocolHandler: |+
            debugEcho: {}
        "#;
        let deserialized: HashMap<String, String> = serde_yaml::from_str(&yaml).unwrap();
        assert!(deserialize_discovery_details(&deserialized).is_err());

        let yaml = r#"
        protocolHandler: |+
          debugEcho:
            descriptions: []
        "#;
        let deserialized: HashMap<String, String> = serde_yaml::from_str(&yaml).unwrap();
        let dh_config = deserialize_discovery_details(&deserialized).unwrap();
        assert!(dh_config.descriptions.is_empty());
        let serialized = serde_json::to_string(&dh_config).unwrap();
        let expected_deserialized = r#"{"descriptions":[]}"#;
        assert_eq!(expected_deserialized, serialized);
    }

    #[test]
    fn test_deserialize_discovery_details_detailed() {
        let yaml = r#"
        protocolHandler: |+
          debugEcho:
            descriptions:
              - "foo1"
        "#;
        let deserialized: HashMap<String, String> = serde_yaml::from_str(&yaml).unwrap();
        let dh_config = deserialize_discovery_details(&deserialized).unwrap();
        assert_eq!(dh_config.descriptions.len(), 1);
        assert_eq!(&dh_config.descriptions[0], "foo1");
    }
}
