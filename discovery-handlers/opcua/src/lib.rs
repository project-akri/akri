#[macro_use]
extern crate serde_derive;

pub mod discovery_handler;
mod discovery_impl;
mod wrappers;

/// Name of the environment variable that will be mounted into the OPC UA broker pods.
/// Holds the DiscoveryURL for the OPC UA Server the broker is to connect to.
pub const OPCUA_DISCOVERY_URL_LABEL: &str = "OPCUA_DISCOVERY_URL";
/// Protocol name that opcua discovery handlers use when registering with the Agent
pub const PROTOCOL_NAME: &str = "opcua";
/// Defines whether this discovery handler discovers local devices on nodes rather than ones visible to multiple nodes
pub const IS_LOCAL: bool = false;