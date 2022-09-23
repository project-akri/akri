use akri_discovery_utils::{
    discovery::{
        discovery_handler::{deserialize_discovery_details, DISCOVERED_DEVICES_CHANNEL_CAPACITY},
        v0::{discovery_handler_server::DiscoveryHandler, Device, DiscoverRequest, DiscoverResponse},
        DiscoverStream, 
    },
    registration_client::{DeviceQueryInput,query_devices},
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


#[derive(Serialize, Debug)]
pub struct QueryDevicePostBody {
    pub id: String,
    pub protocol: String
}

/// DebugEchoDiscoveryDetails describes the necessary information needed to discover and filter debug echo devices.
/// Specifically, it contains a list (`descriptions`) of fake devices to be discovered.
/// This information is expected to be serialized in the discovery details map sent during Discover requests.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DebugEchoDiscoveryDetails {
    //if there is query_device_http in the discovery detail, discovery handler will call agent QueryDeviceInfo rpc function for each discovered new device
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query_device_http: Option<String>,    
    //for debug echo discovery handler, each string in description will be treated as a discovered device ID
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
        let query_device_http = discovery_handler_config.query_device_http;
        
        let descriptions = discovery_handler_config.descriptions;
        let mut previously_discovered_list: Vec<Device> = Vec::new();
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

                //Standardize the procedures as same as the method used in OPC protocol discovery handler
                //1. get a filtered list of current dicovery
                //2. Query all devices' external info (if there is query uri configuration) 
                //3. compare current Devices list with the Devices list of last discovery iteration
                //4. check if there is no change (two lists have same length, and every component in current list is in previous list). If there is discrepancy, return full list of Devices
                
                let mut latest_discovered_list:Vec<Device>= Vec::new();
                let availability =
                    fs::read_to_string(DEBUG_ECHO_AVAILABILITY_CHECK_PATH).unwrap_or_default();
                trace!(
                    "discover -- debugEcho devices are online? {}",
                    !availability.contains(OFFLINE)
                );

                if  !availability.contains(OFFLINE) {
                    let device_query_requests = descriptions.iter()
                    .map(|description| {
                        let mut properties = HashMap::new();
                        properties.insert(
                            super::DEBUG_ECHO_DESCRIPTION_LABEL.to_string(),
                            String::from(description),
                        );
                        let query_body = QueryDevicePostBody{
                            id:String::from(description),
                            protocol: "DebugEcho".to_string()
                        };
                        DeviceQueryInput{
                            id:String::from(description),
                            properties,
                            query_device_payload:Some(serde_json::to_string(&query_body).unwrap_or(String::from("{}"))),
                            mounts:Vec::default(),
                        }
                    }).collect::<Vec<DeviceQueryInput>>();
                    latest_discovered_list = query_devices(device_query_requests,query_device_http.clone()).await;
                }
                let mut changed_device_list = false;
                let mut matching_device_count = 0;
                latest_discovered_list.iter().for_each(|device| {
                    if !previously_discovered_list.contains(device) {
                        changed_device_list = true;
                    } else {
                        matching_device_count += 1;
                    }
                });

                if changed_device_list || matching_device_count != previously_discovered_list.len() {
                    info!("Debug Echo detect change in discovered devices, send full list to agent");
                    previously_discovered_list = latest_discovered_list.clone();
                    if let Err(e) = discovered_devices_sender
                        .send(Ok(DiscoverResponse {devices: latest_discovered_list }))
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
