use akri_discovery_utils::discovery::{
    discovery_handler::{deserialize_discovery_details, DISCOVERED_DEVICES_CHANNEL_CAPACITY},
    v0::{discovery_handler_server::DiscoveryHandler, Device, DiscoverRequest, DiscoverResponse},
    DiscoverStream,
};
use async_trait::async_trait;
use log::{error, info, trace};
use std::time::Duration;
use std::{collections::HashMap, fs};
use tokio::sync::mpsc;
use tokio::time::sleep;
use tonic::{Response, Status};

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

/// DebugEchoDiscoveryDetails describes the necessary information needed to discover and filter debug echo devices.
/// Specifically, it contains a list (`descriptions`) of fake devices to be discovered.
/// This information is expected to be serialized in the discovery details map sent during Discover requests.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DebugEchoDiscoveryDetails {
    pub descriptions: Vec<String>,
}

/// The DiscoveryHandlerImpl discovers a list of devices, named in its `descriptions`.
/// It mocks discovering the devices by inspecting the contents of the file at `DEBUG_ECHO_AVAILABILITY_CHECK_PATH`.
/// If the file contains "OFFLINE", it won't discover any of the devices, else it discovers them all.
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
        info!("discover - called for debug echo protocol");
        let register_sender = self.register_sender.clone();
        let discover_request = request.get_ref();
        let (discovered_devices_sender, discovered_devices_receiver) =
            mpsc::channel(DISCOVERED_DEVICES_CHANNEL_CAPACITY);
        let discovery_handler_config: DebugEchoDiscoveryDetails =
            deserialize_discovery_details(&discover_request.discovery_details)
                .map_err(|e| tonic::Status::new(tonic::Code::InvalidArgument, format!("{}", e)))?;
        let descriptions = discovery_handler_config.descriptions;
        let mut offline = fs::read_to_string(DEBUG_ECHO_AVAILABILITY_CHECK_PATH)
            .unwrap_or_default()
            .contains(OFFLINE);
        let mut first_loop = true;
        tokio::spawn(async move {
            loop {
                // Before each iteration, check if receiver has dropped
                if discovered_devices_sender.is_closed() {
                    error!("discover - channel closed ... attempting to re-register with Agent");
                    if let Some(sender) = register_sender {
                        sender.send(()).await.unwrap();
                    }
                    break;
                }

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
                    if let Err(e) = discovered_devices_sender
                        .send(Ok(DiscoverResponse {
                            devices: Vec::new(),
                        }))
                        .await
                    {
                        error!("discover - for debugEcho failed to send discovery response with error {}", e);
                        if let Some(sender) = register_sender {
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
                        .map(|description| {
                            let mut properties = HashMap::new();
                            properties.insert(
                                super::DEBUG_ECHO_DESCRIPTION_LABEL.to_string(),
                                description.clone(),
                            );
                            Device {
                                id: description.clone(),
                                properties,
                                mounts: Vec::default(),
                                device_specs: Vec::default(),
                            }
                        })
                        .collect::<Vec<Device>>();
                    if let Err(e) = discovered_devices_sender
                        .send(Ok(DiscoverResponse { devices }))
                        .await
                    {
                        // TODO: consider re-registering here
                        error!("discover - for debugEcho failed to send discovery response with error {}", e);
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
    use akri_discovery_utils::discovery::v0::DiscoverRequest;
    use akri_shared::akri::configuration::DiscoveryHandlerInfo;

    #[test]
    fn test_deserialize_discovery_details_empty() {
        let dh_config: Result<DebugEchoDiscoveryDetails, anyhow::Error> =
            deserialize_discovery_details("");
        assert!(dh_config.is_err());

        let dh_config: DebugEchoDiscoveryDetails =
            deserialize_discovery_details("descriptions: []").unwrap();
        assert!(dh_config.descriptions.is_empty());
        let serialized = serde_json::to_string(&dh_config).unwrap();
        let expected_deserialized = r#"{"descriptions":[]}"#;
        assert_eq!(expected_deserialized, serialized);
    }

    #[test]
    fn test_deserialize_discovery_details_detailed() {
        let yaml = r#"
            descriptions:
              - "foo1"
        "#;
        let dh_config: DebugEchoDiscoveryDetails = deserialize_discovery_details(yaml).unwrap();
        assert_eq!(dh_config.descriptions.len(), 1);
        assert_eq!(&dh_config.descriptions[0], "foo1");
    }

    #[tokio::test]
    async fn test_discover_online_devices() {
        // Make devices "online"
        fs::write(DEBUG_ECHO_AVAILABILITY_CHECK_PATH, "").unwrap();
        let debug_echo_yaml = r#"
          name: debugEcho
          discoveryDetails: |+
              descriptions:
              - "foo1"
        "#;
        let deserialized: DiscoveryHandlerInfo = serde_yaml::from_str(debug_echo_yaml).unwrap();
        let discovery_handler = DiscoveryHandlerImpl::new(None);
        let properties: HashMap<String, String> = [(
            super::super::DEBUG_ECHO_DESCRIPTION_LABEL.to_string(),
            "foo1".to_string(),
        )]
        .iter()
        .cloned()
        .collect();
        let device = akri_discovery_utils::discovery::v0::Device {
            id: "foo1".to_string(),
            properties,
            mounts: Vec::default(),
            device_specs: Vec::default(),
        };
        let discover_request = tonic::Request::new(DiscoverRequest {
            discovery_details: deserialized.discovery_details.clone(),
            discovery_properties: HashMap::new(),
        });
        let mut stream = discovery_handler
            .discover(discover_request)
            .await
            .unwrap()
            .into_inner()
            .into_inner();
        let devices = stream.recv().await.unwrap().unwrap().devices;
        assert_eq!(1, devices.len());
        assert_eq!(devices[0], device);
    }
}
