use super::{
    discovery_impl::{do_parse_and_find, insert_device_with_relatives, DeviceProperties},
    wrappers::udev_enumerator,
};
use akri_discovery_utils::discovery::{
    discovery_handler::{deserialize_discovery_details, DISCOVERED_DEVICES_CHANNEL_CAPACITY},
    v0::{
        discovery_handler_server::DiscoveryHandler, Device, DeviceSpec, DiscoverRequest,
        DiscoverResponse,
    },
    DiscoverStream,
};
use async_trait::async_trait;
use log::{error, info, trace};
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tonic::{Response, Status};

// TODO: make this configurable
pub const DISCOVERY_INTERVAL_SECS: u64 = 10;

/// This defines the udev data stored in the Configuration
/// CRD DiscoveryDetails
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UdevDiscoveryDetails {
    pub udev_rules: Vec<String>,

    #[serde(default)]
    pub group_recursive: bool,
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
                let mut devpaths: HashMap<String, HashSet<DeviceProperties>> = HashMap::new();
                udev_rules.iter().for_each(|rule| {
                    let enumerator = udev_enumerator::create_enumerator();
                    let paths = do_parse_and_find(enumerator, rule).unwrap();
                    for path in paths.into_iter() {
                        if !discovery_handler_config.group_recursive {
                            devpaths.insert(path.0.clone(), HashSet::from([path]));
                        } else {
                            insert_device_with_relatives(&mut devpaths, path);
                        }
                    }
                });
                trace!(
                    "discover - mapping and returning devices at devpaths {:?}",
                    devpaths
                );
                let discovered_devices = devpaths
                    .into_iter()
                    .map(|(id, paths)| {
                        let mut properties = HashMap::new();
                        let mut device_specs = Vec::new();
                        for (i, (_, node)) in paths.into_iter().enumerate() {
                            let property_suffix = discovery_handler_config
                                .group_recursive
                                .then(|| format!("_{}", i))
                                .unwrap_or_default();
                            if let Some(devnode) = node {
                                properties.insert(
                                    super::UDEV_DEVNODE_LABEL_ID.to_string() + &property_suffix,
                                    devnode.clone(),
                                );
                                device_specs.push(DeviceSpec {
                                    container_path: devnode.clone(),
                                    host_path: devnode,
                                    permissions: "rwm".to_string(),
                                })
                            }
                        }

                        //id is the sysfs path of the most top level device so we only need this one
                        properties.insert(super::UDEV_DEVPATH_LABEL_ID.to_string(), id.clone());

                        // TODO: use device spec
                        Device {
                            id,
                            properties,
                            mounts: Vec::default(),
                            device_specs,
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
        let expected_deserialized = r#"{"udevRules":[],"groupRecursive":false}"#;
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
