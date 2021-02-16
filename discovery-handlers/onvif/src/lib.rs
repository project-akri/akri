pub mod discovery_handler;
mod discovery_impl;

#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate yaserde_derive;

use akri_discovery_utils::discovery::v0::RegisterRequest;
pub fn get_register_request(endpoint: &str) -> RegisterRequest {
    RegisterRequest {
        protocol: discovery_handler::PROTOCOL_NAME.to_string(),
        endpoint: endpoint.to_string(),
        is_local: false,
    }
}
