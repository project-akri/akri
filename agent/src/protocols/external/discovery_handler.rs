use agent::src::discover::discovery::{discovery_client::DiscoveryClient, DiscoverRequest};
use super::super::{DiscoveryHandler, DiscoveryResult};
use akri_shared::akri::configuration::ProtocolHandler2;
use anyhow::Error;
use async_trait::async_trait;
use std::{collections::HashMap, fs};
use tonic::transport::Channel;



// Checks if there is a registered DH for this protocol and returns it's endpoint.
fn get_discovery_handler_endpoint(protocol: &str) -> Option<String> {
    None
}


pub struct ExternalDiscoveryHandler {
    protocol_handler: ProtocolHandler2,
    discovery_endpoint: Option<String>,
}

impl ExternalDiscoveryHandler {
    pub fn new(protocol_handler: &ProtocolHandler2) -> Self {
        let discovery_endpoint = get_discovery_handler_endpoint(&protocol_handler.name);
        let discovery_client: Option<DiscoveryClient<Channel>> = None;
        ExternalDiscoveryHandler {
            protocol_handler: protocol_handler.clone(),
            discovery_client,
            discovery_endpoint
        }
    }

    pub fn set_client(self, client: Option<DiscoveryClient<Channel>>) {
        self.discovery_client = client;
    }
}

#[async_trait]
impl DiscoveryHandler for ExternalDiscoveryHandler {
    async fn discover(&self) -> Result<Vec<DiscoveryResult>, Error> {
        let discover_request = tonic::Request::new(DiscoverRequest{
            discovery_details: self.protocol_handler.discovery_details.clone()
        });
        if self.discovery_endpoint.is_none() {
            get_discovery_handler_endpoint(&self.protocol_handler.name);
        }

        if self.discovery_endpoint.is_some() && self.discovery_client.is_none() {
            self.discovery_client = match DiscoveryClient::connect(self.discovery_endpoint.unwrap()).await {
                Ok(client) => Some(client),
                Err(e) => None
            }
        }

        match self.discovery_client {
            Some(client) => {
                let response = client.discover(discover_request).await?;
                Ok(vec![DiscoveryResult{ digest: "id".to_string(), properties: HashMap::new() }])
            },
            None => {
                Ok(Vec::new())
            }
        }

    }
    fn are_shared(&self) -> Result<bool, Error> {
        // TODO
        Ok(true)
    }
}
