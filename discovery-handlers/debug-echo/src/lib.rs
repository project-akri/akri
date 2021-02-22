pub mod discovery_handler;

#[macro_use]
extern crate serde_derive;

use akri_discovery_utils::discovery::v0::RegisterRequest;
pub fn get_register_request(endpoint: &str) -> RegisterRequest {
    RegisterRequest {
        protocol: discovery_handler::PROTOCOL_NAME.to_string(),
        endpoint: endpoint.to_string(),
        is_local: true,
    }
}
