pub mod discovery_handler;

#[macro_use]
extern crate serde_derive;

/// Name debugEcho discovery handlers use when registering with the Agent
pub const DISCOVERY_HANDLER_NAME: &str = "debugEcho";
/// Label of the environment variable in debugEcho discovery handlers that sets whether debug echo registers
/// as discovering local instances on nodes rather than ones visible to multiple nodes
pub const DEBUG_ECHO_INSTANCES_SHARED_LABEL: &str = "DEBUG_ECHO_INSTANCES_SHARED";
/// Name of environment variable that is set in debug echo brokers. Contains the description of
/// the device.
pub const DEBUG_ECHO_DESCRIPTION_LABEL: &str = "DEBUG_ECHO_DESCRIPTION";
