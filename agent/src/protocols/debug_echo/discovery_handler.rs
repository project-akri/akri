use super::super::{DiscoveryHandler, DiscoveryResult};
use akri_shared::akri::configuration::DebugEchoDiscoveryHandlerConfig;
use async_trait::async_trait;
use failure::Error;
use std::{collections::HashMap, fs};

/// File acting as an environment variable for testing discovery.
/// To mimic an instance going offline, kubectl exec into one of the akri-agent-daemonset pods
/// and echo "OFFLINE" > /tmp/debug-echo-availability.txt
/// To mimic a device coming back online, remove the word "OFFLINE" from the file
/// ie: echo "" > /tmp/debug-echo-availability.txt
pub const DEBUG_ECHO_AVAILABILITY_CHECK_PATH: &str = "/tmp/debug-echo-availability.txt";
/// String to write into DEBUG_ECHO_AVAILABILITY_CHECK_PATH to make DebugEcho devices undiscoverable
pub const OFFLINE: &str = "OFFLINE";

/// `DebugEchoDiscoveryHandler` contains a `DebugEchoDiscoveryHandlerConfig` which has a
/// list of mock instances (`discovery_handler_config.descriptions`) and their sharability.
/// It mocks discovering the instances by inspecting the contents of the file at `DEBUG_ECHO_AVAILABILITY_CHECK_PATH`.
/// If the file contains "OFFLINE", it won't discover any of the instances, else it discovers them all.
#[derive(Debug)]
pub struct DebugEchoDiscoveryHandler {
    discovery_handler_config: DebugEchoDiscoveryHandlerConfig,
}

impl DebugEchoDiscoveryHandler {
    pub fn new(discovery_handler_config: &DebugEchoDiscoveryHandlerConfig) -> Self {
        DebugEchoDiscoveryHandler {
            discovery_handler_config: discovery_handler_config.clone(),
        }
    }
}

#[async_trait]
impl DiscoveryHandler for DebugEchoDiscoveryHandler {
    async fn discover(&self) -> Result<Vec<DiscoveryResult>, Error> {
        let availability =
            fs::read_to_string(DEBUG_ECHO_AVAILABILITY_CHECK_PATH).unwrap_or_default();
        trace!(
            "discover -- DebugEcho capabilities visible? {}",
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
    }
    fn are_shared(&self) -> Result<bool, Error> {
        Ok(self.discovery_handler_config.shared)
    }
}
