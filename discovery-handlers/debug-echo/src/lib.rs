pub mod discovery_handler;

#[macro_use]
extern crate serde_derive;

/// Protocol name that debugEcho discovery handlers use when registering with the Agent
pub const PROTOCOL_NAME: &str = "debugEcho";
/// Defines whether this discovery handler discovers local devices on nodes rather than ones visible to multiple nodes
pub const IS_LOCAL: bool = true;
