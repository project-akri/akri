extern crate pest;
#[macro_use]
extern crate pest_derive;
extern crate udev;
#[macro_use]
extern crate serde_derive;

pub mod discovery_handler;
mod discovery_impl;
mod wrappers;

pub const UDEV_DEVNODE_LABEL_ID: &str = "UDEV_DEVNODE";

use akri_discovery_utils::discovery::v0::RegisterRequest;
pub fn get_register_request(endpoint: &str) -> RegisterRequest {
    RegisterRequest {
        protocol: discovery_handler::PROTOCOL_NAME.to_string(),
        endpoint: endpoint.to_string(),
        is_local: true,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
