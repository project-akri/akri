use super::super::{DiscoveryHandler, DiscoveryResult};
use akri_shared::akri::configuration::NessieDiscoveryHandlerConfig;
use async_trait::async_trait;
use failure::Error;
use std::collections::HashMap;

pub struct NessieDiscoveryHandler {
    discovery_handler_config: NessieDiscoveryHandlerConfig,
}

impl NessieDiscoveryHandler {
    pub fn new(discovery_handler_config: &NessieDiscoveryHandlerConfig) -> Self {
        NessieDiscoveryHandler {
            discovery_handler_config: discovery_handler_config.clone(),
        }
    }
}

#[async_trait]
impl DiscoveryHandler for NessieDiscoveryHandler {
    async fn discover(&self) -> Result<Vec<DiscoveryResult>, failure::Error> {
        let src = self.discovery_handler_config.nessie_url.clone();
        let mut results = Vec::new();

        match reqwest::get(&src).await {
            Ok(resp) => {
                trace!("Found nessie url: {:?} => {:?}", &src, &resp);
                // If the Nessie URL can be accessed, we will return a DiscoveryResult
                // instance
                let mut props = HashMap::new();
                props.insert("nessie_url".to_string(), src.clone());

                results.push(DiscoveryResult::new(&src, props, true));
            }
            Err(err) => {
                println!("Failed to establish connection to {}", &src);
                println!("Error: {}", err);
                return Ok(results);
            }
        };
        Ok(results)
    }
    fn are_shared(&self) -> Result<bool, Error> {
        Ok(true)
    }
}
