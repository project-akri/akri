use super::super::{DiscoveryHandler, DiscoveryResult};
use super::{discovery_impl::do_standard_discovery, OPCUA_DISCOVERY_URL_LABEL};
use akri_shared::akri::configuration::{OpcuaDiscoveryHandlerConfig, OpcuaDiscoveryMethod};
use anyhow::Error;
use async_trait::async_trait;

/// `OpcuaDiscoveryHandler` discovers the OPC UA server instances as described by the `discovery_handler_config.opcua_discovery_method`
/// and the filter `discover_handler_config.application_names`. The instances it discovers are always shared.
#[derive(Debug)]
pub struct OpcuaDiscoveryHandler {
    discovery_handler_config: OpcuaDiscoveryHandlerConfig,
}

impl OpcuaDiscoveryHandler {
    pub fn new(discovery_handler_config: &OpcuaDiscoveryHandlerConfig) -> Self {
        OpcuaDiscoveryHandler {
            discovery_handler_config: discovery_handler_config.clone(),
        }
    }
}

#[async_trait]
impl DiscoveryHandler for OpcuaDiscoveryHandler {
    async fn discover(&self) -> Result<Vec<DiscoveryResult>, Error> {
        let discovery_urls: Vec<String> =
            match &self.discovery_handler_config.opcua_discovery_method {
                OpcuaDiscoveryMethod::standard(standard_opcua_discovery) => do_standard_discovery(
                    standard_opcua_discovery.discovery_urls.clone(),
                    self.discovery_handler_config.application_names.clone(),
                ),
                // No other discovery methods implemented yet
            };

        // Build DiscoveryResult for each server discovered
        Ok(discovery_urls
            .into_iter()
            .map(|discovery_url| {
                let mut properties = std::collections::HashMap::new();
                trace!(
                    "discover - found OPC UA server at DiscoveryURL {}",
                    discovery_url
                );
                properties.insert(OPCUA_DISCOVERY_URL_LABEL.to_string(), discovery_url.clone());
                DiscoveryResult::new(&discovery_url, properties, self.are_shared().unwrap())
            })
            .collect::<Vec<DiscoveryResult>>())
    }
    fn are_shared(&self) -> Result<bool, Error> {
        Ok(true)
    }
}
