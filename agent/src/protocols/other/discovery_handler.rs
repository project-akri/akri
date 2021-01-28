use super::super::super::discover::discovery::{DiscoverResponse, DiscoverRequest};
use super::super::{DiscoveryHandler2, DiscoveryResultStream};
use akri_shared::{
    akri::configuration::{ProtocolHandler, ProtocolHandler2},
    os::env_var::{ActualEnvVarQuery, EnvVarQuery},
};
use anyhow::Error;
use async_trait::async_trait;
use std::{collections::HashMap, fs};
use tokio::sync::mpsc;
use tonic::{Response, Status};

/// File acting as an environment variable for testing discovery.
/// To mimic an instance going offline, kubectl exec into one of the akri-agent-daemonset pods
/// and echo "OFFLINE" > /tmp/debug-echo-availability.txt
/// To mimic a device coming back online, remove the word "OFFLINE" from the file
/// ie: echo "" > /tmp/debug-echo-availability.txt
pub const DEBUG_ECHO_AVAILABILITY_CHECK_PATH: &str = "/tmp/debug-echo-availability.txt";
/// String to write into DEBUG_ECHO_AVAILABILITY_CHECK_PATH to make Other devices undiscoverable
pub const OFFLINE: &str = "OFFLINE";


/// This defines the DebugEcho data stored in the Configuration
/// CRD
///
/// DebugEcho is used for testing Akri.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DebugEchoDiscoveryHandlerConfig {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub descriptions: Vec<String>,
    pub shared: bool,
}

/// `OtherDiscoveryHandler` contains a `OtherDiscoveryHandlerConfig` which has a
/// list of mock instances (`discovery_handler_config.descriptions`) and their sharability.
/// It mocks discovering the instances by inspecting the contents of the file at `DEBUG_ECHO_AVAILABILITY_CHECK_PATH`.
/// If the file contains "OFFLINE", it won't discover any of the instances, else it discovers them all.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
#[derive(Debug)]
pub struct OtherDiscoveryHandler {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub descriptions: Vec<String>,
    pub shared: bool,
}

impl OtherDiscoveryHandler {
    pub fn new() -> Self {
        OtherDiscoveryHandler {}
    }
}

#[async_trait]
impl DiscoveryHandler2 for OtherDiscoveryHandler {
    async fn discover(&mut self, discover_request: DiscoverRequest) -> Result<Response<DiscoveryResultStream>, Status> {
        let (mut tx, rx) = mpsc::channel(4);
        tokio::spawn(async move {
            let availability =
                fs::read_to_string(DEBUG_ECHO_AVAILABILITY_CHECK_PATH).unwrap_or_default();
            trace!(
                "discover -- Other capabilities visible? {}",
                !availability.contains(OFFLINE)
            );
            // If the device is offline, return an empty list of instance info
            if availability.contains(OFFLINE) {
                Ok(Vec::new())
            } else {
                Ok(self
                    .discovery_handler_config
                    .descriptions
                    .iter()
                    .map(|description| {
                        DiscoveryResult::new(description, HashMap::new(), self.are_shared().unwrap())
                    })
                    .collect::<Vec<DiscoveryResult>>())
            }
        });
        Ok(Response::new(rx))
    }
}

fn verify_configuration(
    discovery_handler_config: &ProtocolHandler2,
    query: &impl EnvVarQuery,
)  -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    // Determine whether it is an embedded protocol
    if let Some(protocol_handler_str) = discovery_handler_config.discovery_details.get("protocolHandler") {
        println!("protocol handler {:?}",protocol_handler_str);
        if let Ok(protocol_handler) = serde_yaml::from_str(protocol_handler_str) {
            match protocol_handler {
                DebugEchoDiscoveryHandler => match query.get_env_var("ENABLE_DEBUG_ECHO") {
                    Ok(_) => Ok(Box::new(debug_echo::DebugEchoDiscoveryHandler::new(&dbg))),
                    _ => Err(anyhow::format_err!("No protocol configured")),
                }
            }
        } else {
            Err(anyhow::format_err!("Discovery details had protocol handler but does not have embedded support. Discovery details: {:?}", discovery_handler_config.discovery_details))
        }
    } else {
        Err(anyhow::format_err!("Generic discovery handlers not supported. Discovery details: {:?}", discovery_handler_config.discovery_details))
    }
}