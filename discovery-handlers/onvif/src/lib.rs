pub mod discovery_handler;
mod discovery_impl;
mod discovery_utils;

#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate yaserde_derive;

/// Protocol name that onvif discovery handlers use when registering with the Agent
pub const PROTOCOL_NAME: &str = "onvif";
/// Defines whether this discovery handler discovers local devices on nodes rather than ones visible to multiple nodes
pub const IS_LOCAL: bool = false;
