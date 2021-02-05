
use akri_discovery_utils::discovery::v0::{Device, DiscoverResponse, DiscoverRequest, discovery_server::{Discovery, DiscoveryServer}, Mount};
use anyhow::Error;
use async_trait::async_trait;
use std::{collections::HashMap, fs};
use tokio::sync::mpsc;
use tokio::time::delay_for;
use log::{error, info, trace};
use std::collections::HashSet;
use tonic::{transport::Server, Response, Status};

pub const DISCOVERY_ENDPOINT: &str = "[::1]:10001";

pub type DiscoverStream = mpsc::Receiver<Result<DiscoverResponse, Status>>;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub enum DiscoveryHandlerType {
    DebugEcho(UdevDiscoveryHandlerConfig),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UdevDiscoveryHandlerConfig {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub descriptions: Vec<String>,
}

/// `UdevDiscoveryHandler` discovers udev instances by parsing the udev rules in `discovery_handler_config.udev_rules`.
/// The instances it discovers are always unshared.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UdevDiscoveryHandler {
}

impl UdevDiscoveryHandler {
    pub fn new(discovery_handler_config: &UdevDiscoveryHandlerConfig) -> Self {
        UdevDiscoveryHandler {
            discovery_handler_config: discovery_handler_config.clone(),
        }
    }
}

#[async_trait]
impl Discovery for UdevDiscoveryHandler {
    type DiscoverStream = DiscoverStream;
    async fn discover(& self, request: tonic::Request<DiscoverRequest>) -> Result<Response<Self::DiscoverStream>, Status> {
        info!("discover - called for debug echo protocol");
        let discover_request = request.get_ref();
        let (mut tx, rx) = mpsc::channel(4);
        let discovery_handler_config = get_configuration(&discover_request.discovery_details).map_err(|e| {
            tonic::Status::new(
                tonic::Code::InvalidArgument,
                format!("Invalid debugEcho discovery handler configuration: {}", e),
            )
        })?;
        let descriptions = discovery_handler_config.descriptions;
        let mut availability =
                    fs::read_to_string(DEBUG_ECHO_AVAILABILITY_CHECK_PATH).unwrap_or_default();
        let mut offline =  availability.contains(OFFLINE);
        let mut first_loop = true;
        tokio::spawn(async move {
            loop {
                let udev_rules = self.discovery_handler_config.udev_rules.clone();
                trace!("discover - for udev rules {:?}", udev_rules);
                let mut devpaths: HashSet<String> = HashSet::new();
                udev_rules
                    .iter()
                    .map(|rule| {
                        let enumerator = udev_enumerator::create_enumerator();
                        let paths = discovery_impl::do_parse_and_find(enumerator, &rule)?;
                        paths.into_iter().for_each(|path| {
                            devpaths.insert(path);
                        });
                        Ok(())
                    })
                    .collect::<Result<(), Error>>()?;
                trace!(
                    "discover - mapping and returning devices at devpaths {:?}",
                    devpaths
                );
                let devices = devpaths
                    .into_iter()
                    .map(|path| {
                        let mut properties = std::collections::HashMap::new();
                        properties.insert(UDEV_DEVNODE_LABEL_ID.to_string(), path.clone());
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
                    .collect::<Vec<Devices>>();

                if let Err(e) = tx.send(Ok(DiscoverResponse{ devices })).await {
                    error!("discover - for debugEcho failed to send discovery response with error {}", e);
                    break;
                }
                delay_for(Duration::from_secs(5)).await;
            }
        });
        Ok(Response::new(rx))
    }
}

fn get_configuration(
    discovery_details: &HashMap<String, String>,
)  -> Result<UdevDiscoveryHandlerConfig, Error>{
    info!("inner_get_discovery_handler - for discovery details {:?}", discovery_details);
    // Determine whether it is an embedded protocol
    if let Some(discovery_handler_str) = discovery_details.get("protocolHandler") {
        info!("protocol handler {:?}",discovery_handler_str);
        if let Ok(discovery_handler) = serde_yaml::from_str(discovery_handler_str) {
            match discovery_handler {
                DiscoveryHandlerType::DebugEcho(debug_echo_discovery_handler_config) => Ok(debug_echo_discovery_handler_config),
                _ => Err(anyhow::format_err!("No protocol configured")),
            }
        } else {
            Err(anyhow::format_err!("Discovery details had protocol handler but does not have embedded support. Discovery details: {:?}", discovery_details))
        }
    } else {
        Err(anyhow::format_err!("Generic discovery handlers not supported. Discovery details: {:?}", discovery_details))
    }
}

pub async fn run_debug_echo_server(
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    info!("run_debug_echo_server - entered");
    let discovery_handler = UdevDiscoveryHandler::new();
    let addr = DISCOVERY_ENDPOINT.parse()?;
    Server::builder().add_service(DiscoveryServer::new(discovery_handler)).serve(addr).await?;
    Ok(())
}