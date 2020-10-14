use super::super::{DiscoveryHandler, DiscoveryResult};
use akri_shared::akri::configuration::OpcuaDiscoveryHandlerConfig;
use async_trait::async_trait;
use failure::Error;

/// `OnvifDiscoveryHandler` discovers the OPC instances. The instances it discovers are always shared.
#[derive(Debug)]
pub struct OpcuaDiscoveryHandler {}

impl OpcuaDiscoveryHandler {
    pub fn new(_discovery_handler_config: &OpcuaDiscoveryHandlerConfig) -> Self {
        OpcuaDiscoveryHandler {}
    }
}

#[async_trait]
impl DiscoveryHandler for OpcuaDiscoveryHandler {
    async fn discover(&self) -> Result<Vec<DiscoveryResult>, Error> {
        Err(failure::format_err!("OPC protocol handler not implemented"))
    }
    fn are_shared(&self) -> Result<bool, Error> {
        Ok(true)
    }
}
