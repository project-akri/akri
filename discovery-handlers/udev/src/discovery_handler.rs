use super::{discovery_impl::do_parse_and_find, wrappers::udev_enumerator};
use akri_discovery_utils::discovery::v0::{
    discovery_server::Discovery, Device, DiscoverRequest, DiscoverResponse, Mount,
};
use anyhow::Error;
use async_trait::async_trait;
use log::{error, info, trace};
use std::collections::HashSet;
use std::{collections::HashMap, time::Duration};
use tokio::sync::mpsc;
use tokio::time::delay_for;
use tonic::{Response, Status};

/// Protocol name that onvif discovery handlers use when registering with the Agent
pub const PROTOCOL_NAME: &str = "udev";
pub const DISCOVERY_PORT: &str = "10000";
// TODO: make this configurable
pub const DISCOVERY_INTERVAL_SECS: u64 = 10;

pub type DiscoverStream = mpsc::Receiver<Result<DiscoverResponse, Status>>;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub enum DiscoveryHandlerType {
    Udev(UdevDiscoveryHandlerConfig),
}

/// This defines the udev data stored in the Configuration
/// CRD
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UdevDiscoveryHandlerConfig {
    pub udev_rules: Vec<String>,
}

/// `DiscoveryHandler` discovers udev instances by parsing the udev rules in `discovery_handler_config.udev_rules`.
/// The instances it discovers are always unshared.
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
        info!("discover - called for udev protocol");
        let shutdown_sender = self.shutdown_sender.clone();
        let discover_request = request.get_ref();
        let (mut tx, rx) = mpsc::channel(4);
        let discovery_handler_config = get_configuration(&discover_request.discovery_details)
            .map_err(|e| {
                tonic::Status::new(
                    tonic::Code::InvalidArgument,
                    format!("Invalid udev discovery handler configuration: {}", e),
                )
            })?;
        let mut previously_discovered_devices: Vec<Device> = Vec::new();
        tokio::spawn(async move {
            let udev_rules = discovery_handler_config.udev_rules.clone();
            loop {
                trace!("discover - for udev rules {:?}", udev_rules);
                let mut devpaths: HashSet<String> = HashSet::new();
                udev_rules
                    .iter()
                    .map(|rule| {
                        let enumerator = udev_enumerator::create_enumerator();
                        let paths = do_parse_and_find(enumerator, &rule)?;
                        paths.into_iter().for_each(|path| {
                            devpaths.insert(path);
                        });
                        Ok(())
                    })
                    .collect::<Result<(), Error>>()
                    .unwrap();
                trace!(
                    "discover - mapping and returning devices at devpaths {:?}",
                    devpaths
                );
                let discovered_devices = devpaths
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
                        Device {
                            id: path,
                            properties,
                            mounts: vec![mount],
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
                    info!("discover - sending updated device list");
                    previously_discovered_devices = discovered_devices.clone();
                    if let Err(e) = tx
                        .send(Ok(DiscoverResponse {
                            devices: discovered_devices,
                        }))
                        .await
                    {
                        error!(
                            "discover - for udev failed to send discovery response with error {}",
                            e
                        );
                        if shutdown_sender.is_some() {
                            shutdown_sender.unwrap().send(()).await.unwrap();
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

fn get_configuration(
    discovery_details: &HashMap<String, String>,
) -> Result<UdevDiscoveryHandlerConfig, Error> {
    info!(
        "inner_get_discovery_handler - for discovery details {:?}",
        discovery_details
    );
    // Determine whether it is an embedded protocol
    if let Some(discovery_handler_str) = discovery_details.get("protocolHandler") {
        info!("protocol handler {:?}", discovery_handler_str);
        if let Ok(discovery_handler) = serde_yaml::from_str(discovery_handler_str) {
            match discovery_handler {
                DiscoveryHandlerType::Udev(discovery_handler_config) => {
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
    fn test_get_configuration_empty() {
        // Check that udev errors if no udev rules passed in
        let yaml = r#"
          protocolHandler: |+
            udev: {}
        "#;
        let deserialized: HashMap<String, String> = serde_yaml::from_str(&yaml).unwrap();
        assert!(get_configuration(&deserialized).is_err());

        let yaml = r#"
        protocolHandler: |+
          udev: 
            udevRules: []
        "#;
        let deserialized: HashMap<String, String> = serde_yaml::from_str(&yaml).unwrap();
        let udev_dh_config = get_configuration(&deserialized).unwrap();
        assert!(udev_dh_config.udev_rules.is_empty());
        let serialized = serde_json::to_string(&udev_dh_config).unwrap();
        let expected_deserialized = r#"{"udevRules":[]}"#;
        assert_eq!(expected_deserialized, serialized);
    }

    #[test]
    fn test_get_configuration_detailed() {
        let yaml = r#"
        protocolHandler: |+
          udev:
            udevRules:
            -  'KERNEL=="video[0-9]*"'
        "#;
        let deserialized: HashMap<String, String> = serde_yaml::from_str(&yaml).unwrap();
        let udev_dh_config = get_configuration(&deserialized).unwrap();
        assert_eq!(udev_dh_config.udev_rules.len(), 1);
        assert_eq!(&udev_dh_config.udev_rules[0], "KERNEL==\"video[0-9]*\"");
    }
}
