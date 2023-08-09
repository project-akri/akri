mod credential_store;
pub mod discovery_handler;
mod discovery_impl;
mod discovery_utils;
mod username_token;

#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate yaserde_derive;

/// Name that onvif discovery handlers use when registering with the Agent
pub const DISCOVERY_HANDLER_NAME: &str = "onvif";
/// Defines whether this discovery handler discovers local devices on nodes rather than ones visible to multiple nodes
pub const SHARED: bool = true;
