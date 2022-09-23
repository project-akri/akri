use super::{discovery_impl::do_parse_and_find, wrappers::udev_enumerator};
use akri_discovery_utils::{
    discovery::{
        discovery_handler::{deserialize_discovery_details, DISCOVERED_DEVICES_CHANNEL_CAPACITY},
        v0::{
            discovery_handler_server::DiscoveryHandler, Device, DiscoverRequest, DiscoverResponse,
            Mount,
        },
        DiscoverStream,
    },
    call_agent_service::{DeviceQueryInput,query_devices},
};
use std::env;
use async_trait::async_trait;
use log::{error, info, trace};
use std::collections::HashSet;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tonic::{Response, Status};

// TODO: make this configurable
pub const DISCOVERY_INTERVAL_SECS: u64 = 10;


#[derive(Serialize, Debug)]
pub struct QueryDevicePostBody {
    pub node_name: String,
    pub name: String,
    pub protocol: String
}

/// This defines the udev data stored in the Configuration
/// CRD DiscoveryDetails
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UdevDiscoveryDetails {
    //if there is query_device_http in the discovery detail, discovery handler will call agent QueryDeviceInfo rpc function for each discovered new device
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query_device_http: Option<String>,
    pub udev_rules: Vec<String>,
}

/// `DiscoveryHandlerImpl` discovers udev instances by parsing the udev rules in `discovery_handler_config.udev_rules`.
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
        info!("discover - called for udev protocol");
        let register_sender = self.register_sender.clone();
        let discover_request = request.get_ref();
        let (discovered_devices_sender, discovered_devices_receiver) =
            mpsc::channel(DISCOVERED_DEVICES_CHANNEL_CAPACITY);
        let discovery_handler_config: UdevDiscoveryDetails =
            deserialize_discovery_details(&discover_request.discovery_details)
                .map_err(|e| tonic::Status::new(tonic::Code::InvalidArgument, format!("{}", e)))?;
        let query_device_http = discovery_handler_config.query_device_http.clone();
        let mut previously_discovered_devices: Vec<Device> = Vec::new();
        tokio::spawn(async move {
            let udev_rules = discovery_handler_config.udev_rules.clone();
            loop {
                trace!("discover - for udev rules {:?}", udev_rules);
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

                let mut devpaths: HashSet<String> = HashSet::new();
                udev_rules.iter().for_each(|rule| {
                    let enumerator = udev_enumerator::create_enumerator();
                    let paths = do_parse_and_find(enumerator, rule).unwrap();
                    paths.into_iter().for_each(|path| {
                        devpaths.insert(path);
                    });
                });
                trace!(
                    "discover - mapping and returning devices at devpaths {:?}",
                    devpaths
                );
                let device_query_requests:Vec<DeviceQueryInput> = devpaths
                    .into_iter()
                    .map(|path| {
                        let mut properties = std::collections::HashMap::new();
                        properties.insert(super::UDEV_DEVNODE_LABEL_ID.to_string(), path.clone());
                        let mount = Mount {
                            container_path: path.clone(),
                            host_path: path.clone(),
                            read_only: true,
                        };
                        // TODO: use device spec
                        let node_name=env::var("AGENT_NODE_NAME").unwrap_or(String::from(""));
                        let query_body = QueryDevicePostBody{
                            node_name,
                            name:path.clone(),
                            protocol: "udev".to_string()
                        };
                        DeviceQueryInput{
                            id:path,
                            properties,
                            query_device_payload:Some(serde_json::to_string(&query_body).unwrap_or(String::from("{}"))),
                            mounts:vec![mount],
                        }
                    })
                    .collect::<Vec<DeviceQueryInput>>();
                let discovered_devices= query_devices(device_query_requests,query_device_http.clone()).await;
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
                    info!("discover - sending updated device list");
                    previously_discovered_devices = discovered_devices.clone();
                    if let Err(e) = discovered_devices_sender
                        .send(Ok(DiscoverResponse {
                            devices: discovered_devices,
                        }))
                        .await
                    {
                        error!(
                            "discover - for udev failed to send discovery response with error {}",
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
        // Check that udev errors if no udev rules passed in
        let udev_dh_config: Result<UdevDiscoveryDetails, anyhow::Error> =
            deserialize_discovery_details("");
        assert!(udev_dh_config.is_err());

        let yaml = r#"
          udevRules: []
        "#;
        let udev_dh_config: UdevDiscoveryDetails = deserialize_discovery_details(yaml).unwrap();
        assert!(udev_dh_config.udev_rules.is_empty());
        let serialized = serde_json::to_string(&udev_dh_config).unwrap();
        let expected_deserialized = r#"{"udevRules":[]}"#;
        assert_eq!(expected_deserialized, serialized);
    }

    #[test]
    fn test_deserialize_discovery_details_detailed() {
        let yaml = r#"
          udevRules:
          - 'KERNEL=="video[0-9]*"'
        "#;
        let udev_dh_config: UdevDiscoveryDetails = deserialize_discovery_details(yaml).unwrap();
        assert_eq!(udev_dh_config.udev_rules.len(), 1);
        assert_eq!(&udev_dh_config.udev_rules[0], "KERNEL==\"video[0-9]*\"");
    }
}
