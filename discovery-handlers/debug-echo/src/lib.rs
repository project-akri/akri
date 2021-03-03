pub mod discovery_handler;

#[macro_use]
extern crate serde_derive;

/// Protocol name that debugEcho discovery handlers use when registering with the Agent
pub const PROTOCOL_NAME: &str = "debugEcho";
/// Label of the environment variable in debugEcho discovery handlers that sets whether debug echo registers
/// as discovering local instances on nodes rather than ones visible to multiple nodes
pub const INSTANCES_ARE_LOCAL_LABEL: &str = "INSTANCES_ARE_LOCAL";
