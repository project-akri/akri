use super::super::{DiscoveryHandler, DiscoveryResult};

use akri_shared::akri::configuration::HTTPDiscoveryHandlerConfig;
use async_trait::async_trait;
use failure::Error;
use reqwest::get;
use std::collections::HashMap;

const BROKER_NAME: &str = "AKRI_HTTP";
const DEVICE_ENDPOINT: &str = "AKRI_HTTP_DEVICE_ENDPOINT";

pub struct HTTPDiscoveryHandler {
    discovery_handler_config: HTTPDiscoveryHandlerConfig,
}
impl HTTPDiscoveryHandler {
    pub fn new(discovery_handler_config: &HTTPDiscoveryHandlerConfig) -> Self {
        trace!("[http:new] Entered");
        HTTPDiscoveryHandler {
            discovery_handler_config: discovery_handler_config.clone(),
        }
    }
}
#[async_trait]

impl DiscoveryHandler for HTTPDiscoveryHandler {
    async fn discover(&self) -> Result<Vec<DiscoveryResult>, failure::Error> {
        trace!("[http:discover] Entered");

        let url = self.discovery_handler_config.discovery_endpoint.clone();
        trace!("[http:discover] url: {}", &url);

        match get(&url).await {
            Ok(resp) => {
                trace!(
                    "[http:discover] Connected to discovery endpoint: {:?} => {:?}",
                    &url,
                    &resp
                );

                // Reponse is a newline separated list of devices (host:port) or empty
                let device_list = &resp.text().await?;

                let result = device_list
                    .lines()
                    .map(|endpoint| {
                        trace!("[http:discover:map] Creating DiscoverResult: {}", endpoint);
                        trace!(
                            "[http:discover] props.inserting: {}, {}",
                            BROKER_NAME,
                            DEVICE_ENDPOINT,
                        );
                        let mut props = HashMap::new();
                        props.insert(BROKER_NAME.to_string(), "http".to_string());
                        props.insert(DEVICE_ENDPOINT.to_string(), endpoint.to_string());
                        DiscoveryResult::new(endpoint, props, true)
                    })
                    .collect::<Vec<DiscoveryResult>>();
                trace!("[protocol:http] Result: {:?}", &result);
                Ok(result)
            }
            Err(err) => {
                trace!(
                    "[http:discover] Failed to connect to discovery endpoint: {}",
                    &url
                );
                trace!("[http:discover] Error: {}", err);

                Err(failure::format_err!(
                    "Failed to connect to discovery endpoint results: {:?}",
                    err
                ))
            }
        }
    }
    fn are_shared(&self) -> Result<bool, Error> {
        trace!("[http:are_shared] Entered");
        Ok(true)
    }
}
