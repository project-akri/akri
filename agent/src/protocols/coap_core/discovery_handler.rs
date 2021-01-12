use std::collections::HashMap;
use std::time::Duration;

use crate::protocols::{DiscoveryHandler, DiscoveryResult};

use akri_shared::akri::configuration::CoAPCoREDiscoveryHandlerConfig;
use async_trait::async_trait;
use coap::CoAPClient;
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
}

#[async_trait]
impl DiscoveryHandler for CoAPCoREDiscoveryHandler {
    async fn discover(&self) -> Result<Vec<DiscoveryResult>, failure::Error> {
        let ip_addresses = &self.discovery_handler_config.ip_addresses;
        let mut results: Vec<DiscoveryResult> = vec![];

        for ip_address in ip_addresses {
            let endpoint = format!("coap://{}:5683/well-known/core", ip_address);
            info!("Discovering resources on endpoint {}", endpoint);

            // TODO: make timeout configurable
            let response = CoAPClient::get_with_timeout(endpoint.as_str(), Duration::from_secs(5));

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

                    results.push(result);
                }
                Err(e) => {
                    info!("Error requesting resource discovery to device: {}", e);
                }
            }
        }

        Ok(results)
    }

    fn are_shared(&self) -> Result<bool, failure::Error> {
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // use coap::Server;
    // use coap_lite::{CoapRequest, CoapResponse};
    // use std::future::Future;
    // use std::net::SocketAddr;
    // use std::thread::JoinHandle;

    // pub fn spawn_server<
    //     F: FnMut(CoapRequest<SocketAddr>) -> HandlerRet + Send + 'static,
    //     HandlerRet,
    // >(
    //     request_handler: F,
    // ) -> JoinHandle<()>
    // where
    //     HandlerRet: Future<Output = Option<CoapResponse>>,
    // {
    //     std::thread::Builder::new()
    //         .name(String::from("coap server"))
    //         .spawn(move || {
    //             tokio::runtime::Runtime::new()
    //                 .unwrap()
    //                 .block_on(async move {
    //                     let mut server = Server::new("127.0.0.1:5683").unwrap();

    //                     server.run(request_handler).await.unwrap();
    //                 })
    //         })
    //         .unwrap()
    // }

    // async fn request_handler(req: CoapRequest<SocketAddr>) -> Option<CoapResponse> {
    //     let path = req.get_path();

    //     match path.as_str() {
    //         "/well-known/core" => match req.response {
    //             Some(mut response) => {
    //                 response.message.payload =
    //                     br#"</sensors/temp>;rt="oic.r.temperature";if="sensor",
    //                         </sensors/light>;rt="oic.r.light.brightness";if="sensor""#.to_vec();
    //                 Some(response)
    //             }
    //             _ => None,
    //         },
    //         _ => None,
    //     }
    // }

    #[tokio::test]
    async fn test_discover_resources() {
        // TODO: find better way than setting env variable, which could be shared to other tests
        std::env::set_var("AGENT_NODE_NAME", "node-1");

        // let handle = spawn_server(request_handler);

        let config = CoAPCoREDiscoveryHandlerConfig {
            ip_addresses: vec![String::from("127.0.0.1")],
        };
        let handler = CoAPCoREDiscoveryHandler::new(&config);
        let results = handler.discover().await.unwrap();

        let discovered = results.get(0).expect("No resources discovered");

        assert_eq!(discovered.properties.get(COAP_IP_LABEL_ID), Some(&"127.0.0.1".to_string()));
        assert_eq!(discovered.properties.get(COAP_RESOURCE_TYPES_LABEL_ID), Some(&"oic.r.temperature,oic.r.light.brightness".to_string()));
        assert_eq!(discovered.properties.get("oic.r.temperature"), Some(&"/sensors/temp".to_string()));
        assert_eq!(discovered.properties.get("oic.r.light.brightness"), Some(&"/sensors/light".to_string()));
    }
}
