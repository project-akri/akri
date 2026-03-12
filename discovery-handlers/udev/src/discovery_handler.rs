use super::{
    discovery_impl::{DeviceProperties, do_parse_and_find, insert_device_with_relatives},
    wrappers::udev_enumerator,
};
use akri_discovery_utils::discovery::{
    DiscoverStream,
    discovery_handler::{DISCOVERED_DEVICES_CHANNEL_CAPACITY, deserialize_discovery_details},
    v0::{
        Device, DeviceSpec, DiscoverRequest, DiscoverResponse,
        discovery_handler_server::DiscoveryHandler,
    },
};
use async_trait::async_trait;
use log::{error, info, trace};
use serde::{Deserialize, Deserializer, de};
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

    #[serde(default = "default_permissions")]
    #[serde(deserialize_with = "validate_permissions")]
    pub permissions: String,
}

// Validate the permissible set of cgroups `permissions`
fn validate_permissions<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let value: String = Deserialize::deserialize(deserializer)?;

    // Validating that the string only contains allowed combinations of 'r', 'w', 'm'
    let valid_permissions = ["r", "w", "m", "rw", "rm", "rwm", "wm"];
    if valid_permissions.contains(&value.as_str()) {
        Ok(value)
    } else {
        Err(de::Error::invalid_value(
            de::Unexpected::Str(&value),
            &"a valid permission combination ('r', 'w', 'm', 'rw', 'rm', 'rwm', 'wm')",
        ))
    }
}

/// Default permissions for devices
fn default_permissions() -> String {
    "rwm".to_string()
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
                .map_err(|e| tonic::Status::new(tonic::Code::InvalidArgument, format!("{e}")))?;
        let device_plugin_resource_name: Option<String> = discover_request
            .discovery_properties
            .get(super::DEVICE_PLUGIN_RESOURCE_PROPERTY_KEY)
            .and_then(|b| b.vec.as_ref())
            .and_then(|v| std::str::from_utf8(v).ok())
            .map(|s| s.to_string());
        let vfio_passthrough: bool = discover_request
            .discovery_properties
            .get(super::VFIO_PASSTHROUGH_PROPERTY_KEY)
            .and_then(|b| b.vec.as_ref())
            .and_then(|v| std::str::from_utf8(v).ok())
            .map(|s| s.trim().eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        let mut previously_discovered_devices: Vec<Device> = Vec::new();
        tokio::spawn(async move {
            let udev_rules = discovery_handler_config.udev_rules.clone();
            loop {
                trace!("discover - for udev rules {udev_rules:?}");
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
                trace!("discover - mapping and returning devices at devpaths {devpaths:?}");
                let discovered_devices = devpaths
                    .into_iter()
                    .map(|(id, paths)| {
                        let mut properties = HashMap::new();
                        let mut device_specs = Vec::new();
                        for (i, (_, node)) in paths.into_iter().enumerate() {
                            let property_suffix = if discovery_handler_config.group_recursive {
                                format!("_{i}")
                            } else {
                                Default::default()
                            };

                            if let Some(devnode) = node {
                                properties.insert(
                                    super::UDEV_DEVNODE_LABEL_ID.to_string() + &property_suffix,
                                    devnode.clone(),
                                );

                                if let Some((bus, device)) = super::usb_utils::extract_usb_address(&devnode) {
                                    if let Some(ref resource_name) = device_plugin_resource_name {
                                        let env_var = super::usb_utils::to_usb_resource_env_var(resource_name);
                                        let value = format!("{}:{}", bus, device);
                                        trace!("discover - USB resource: {}={} path={}", env_var, value, devnode);
                                        properties.insert(env_var + &property_suffix, value);
                                    }
                                }

                                device_specs.push(DeviceSpec {
                                    container_path: devnode.clone(),
                                    host_path: devnode,
                                    permissions: discovery_handler_config.permissions.clone(),
                                })
                            }
                        }

                        //id is the sysfs path of the most top level device so we only need this one
                        properties.insert(super::UDEV_DEVPATH_LABEL_ID.to_string(), id.clone());

                        let is_usb_device = id.split('/').any(|s| s.starts_with("usb"));
                        if !is_usb_device {
                            if let Some(ref resource_name) = device_plugin_resource_name {
                                if let Some(pci_addr) = super::usb_utils::extract_pci_address(&id) {
                                    let env_var = super::usb_utils::to_pci_resource_env_var(resource_name);
                                    trace!("discover - PCI resource: {}={} path={}", env_var, pci_addr, id);
                                    properties.insert(env_var, pci_addr);

                                    if vfio_passthrough {
                                        if let Some(iommu_group) = super::usb_utils::read_iommu_group(&id) {
                                            let vfio_group = format!("/dev/vfio/{}", iommu_group);
                                            trace!("discover - PCI VFIO group: {} path={}", vfio_group, id);
                                            device_specs.push(DeviceSpec {
                                                container_path: vfio_group.clone(),
                                                host_path: vfio_group,
                                                permissions: "rw".to_string(),
                                            });
                                            device_specs.push(DeviceSpec {
                                                container_path: "/dev/vfio/vfio".to_string(),
                                                host_path: "/dev/vfio/vfio".to_string(),
                                                permissions: "rw".to_string(),
                                            });
                                        } else {
                                            trace!("discover - PCI device {} has no IOMMU group (not vfio-pci bound?), skipping vfio DeviceSpec", id);
                                        }
                                    }
                                }
                            }
                        }

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
                    previously_discovered_devices.clone_from(&discovered_devices);
                    if let Err(e) = discovered_devices_sender
                        .send(Ok(DiscoverResponse {
                            devices: discovered_devices,
                        }))
                        .await
                    {
                        error!(
                            "discover - for udev failed to send discovery response with error {e}"
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
        let expected_deserialized =
            r#"{"udevRules":[],"groupRecursive":false,"permissions":"rwm"}"#;
        assert_eq!(expected_deserialized, serialized);
    }

    #[test]
    fn test_deserialize_discovery_details_detailed() {
        let yaml = r#"
          udevRules:
          - 'KERNEL=="video[0-9]*"'
          permissions: rwm
        "#;
        let udev_dh_config: UdevDiscoveryDetails = deserialize_discovery_details(yaml).unwrap();
        assert_eq!(udev_dh_config.udev_rules.len(), 1);
        assert_eq!(&udev_dh_config.udev_rules[0], "KERNEL==\"video[0-9]*\"");
        assert_eq!(&udev_dh_config.permissions, "rwm");
    }

    #[test]
    fn test_deserialize_discovery_details_permissions_invalid() {
        let yaml = r#"
          udevRules:
          - 'KERNEL=="video[0-9]*"'
          permissions: xyz
        "#;
        match deserialize_discovery_details::<UdevDiscoveryDetails>(yaml) {
            Ok(_) => panic!("Expected error parsing invalid permissions"),
            Err(e) => assert!(e.to_string().contains("a valid permission combination")),
        }
    }
}
