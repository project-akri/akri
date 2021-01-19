use std::collections::HashMap;
use std::time::Duration;

use crate::protocols::{DiscoveryHandler, DiscoveryResult};

use akri_shared::akri::configuration::CoAPCoREDiscoveryHandlerConfig;
use akri_shared::coap_core::{CoAPClient, CoAPClientImpl};
use async_trait::async_trait;
use log::info;

use super::discovery_impl;

pub const COAP_RESOURCE_TYPES_LABEL_ID: &str = "COAP_RESOURCE_TYPES";
pub const COAP_IP_LABEL_ID: &str = "COAP_IP";

pub struct CoAPCoREDiscoveryHandler {
    discovery_handler_config: CoAPCoREDiscoveryHandlerConfig,
}

impl CoAPCoREDiscoveryHandler {
    pub fn new(discovery_handler_config: &CoAPCoREDiscoveryHandlerConfig) -> Self {
        CoAPCoREDiscoveryHandler {
            discovery_handler_config: discovery_handler_config.clone(),
        }
    }

    fn discover_endpoint(&self, coap_client: &impl CoAPClient, ip_address: &String, duration: Duration) -> Option<DiscoveryResult>
    {
        let endpoint = format!("coap://{}:5683/well-known/core", ip_address);
        info!("Discovering resources on endpoint {}", endpoint);

        let response = coap_client.get_with_timeout(endpoint.as_str(), duration);

        match response {
            Ok(response) => {
                let payload = String::from_utf8(response.message.payload)
                    .expect("Receive payload is not a string");
                info!("Device responded with {}", payload);

                let mut properties: HashMap<String, String> = HashMap::new();
                let resources = discovery_impl::parse_link_value(payload.as_str());
                let resource_types: Vec<String> = resources.iter().map(|res| res.1.clone()).collect();

                properties.insert(COAP_IP_LABEL_ID.to_string(), ip_address.clone());
                properties.insert(COAP_RESOURCE_TYPES_LABEL_ID.to_string(), resource_types.join(","));

                for (uri, rtype) in resources {
                    properties.insert(rtype, uri);
                }

                let result = DiscoveryResult::new(ip_address.as_str(), properties, false);

                Some(result)
            }
            Err(e) => {
                info!("Error requesting resource discovery to device: {}", e);
                None
            }
        }
    }
}

#[async_trait]
impl DiscoveryHandler for CoAPCoREDiscoveryHandler {
    async fn discover(&self) -> Result<Vec<DiscoveryResult>, anyhow::Error> {
        let coap_client = CoAPClientImpl {};
        let ip_addresses = &self.discovery_handler_config.ip_addresses;
        let mut results: Vec<DiscoveryResult> = vec![];

        for ip_address in ip_addresses {
            // TODO: make timeout configurable
            let result = self.discover_endpoint(&coap_client, ip_address, Duration::from_secs(5));

            if let Some(result) = result {
                results.push(result);
            }
        }

        Ok(results)
    }

    fn are_shared(&self) -> Result<bool, anyhow::Error> {
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use coap_lite::{CoapResponse, Packet, MessageType};
    use akri_shared::coap_core::test_coap_core::MockCoAPClient;

    fn configure_coap_response(mock: &mut MockCoAPClient) {
        mock.expect_get_with_timeout()
            .returning(|url, _| {
                let mut request = Packet::new();
                request.header.set_type(MessageType::Confirmable);

                let mut response = CoapResponse::new(&request).unwrap();
                
                match url {
                    "coap://127.0.0.1:5683/well-known/core" => {
                        response.message.payload = 
                            br#"</sensors/temp>;rt="oic.r.temperature";if="sensor",
                                </sensors/light>;rt="oic.r.light.brightness";if="sensor""#.to_vec();
                    },
                    u => {
                        panic!("Unexpected url passed to get_with_timeout: {}", u);
                    }
                }

                Ok(response)
            });
    }

    #[tokio::test]
    async fn test_discover_resources() {
        // TODO: find better way than setting env variable, which could be shared to other tests
        std::env::set_var("AGENT_NODE_NAME", "node-1");
        let mut mock_coap_client = MockCoAPClient::new();
        configure_coap_response(&mut mock_coap_client);

        let ip_address = String::from("127.0.0.1");
        let config = CoAPCoREDiscoveryHandlerConfig {
            ip_addresses: vec![ip_address.clone()],
        };
        let handler = CoAPCoREDiscoveryHandler::new(&config);
        let result = handler.discover_endpoint(&mock_coap_client, &ip_address, Duration::from_secs(5)).unwrap();

        assert_eq!(result.properties.get(COAP_IP_LABEL_ID), Some(&"127.0.0.1".to_string()));
        assert_eq!(result.properties.get(COAP_RESOURCE_TYPES_LABEL_ID), Some(&"oic.r.temperature,oic.r.light.brightness".to_string()));
        assert_eq!(result.properties.get("oic.r.temperature"), Some(&"/sensors/temp".to_string()));
        assert_eq!(result.properties.get("oic.r.light.brightness"), Some(&"/sensors/light".to_string()));
    }
}
