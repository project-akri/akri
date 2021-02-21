#[macro_use]
extern crate serde_derive;

pub mod discovery_handler;
mod discovery_impl;
mod wrappers;

/// Name of the environment variable that will be mounted into the OPC UA broker pods.
/// Holds the DiscoveryURL for the OPC UA Server the broker is to connect to.
pub const OPCUA_DISCOVERY_URL_LABEL: &str = "OPCUA_DISCOVERY_URL";
use akri_discovery_utils::discovery::v0::RegisterRequest;
pub fn get_register_request(endpoint: &str) -> RegisterRequest {
    RegisterRequest {
        protocol: discovery_handler::PROTOCOL_NAME.to_string(),
        endpoint: endpoint.to_string(),
        is_local: false,
    }
}