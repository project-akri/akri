use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Duration;

use crate::protocols::{DiscoveryHandler, DiscoveryResult};

use akri_shared::akri::configuration::CoAPCoREDiscoveryHandlerConfig;
use akri_shared::coap_core::{CoAPClient, CoAPClientImpl};
use async_trait::async_trait;
use coap_lite::CoapRequest;
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

    fn discover_endpoint(
        &self,
        coap_client: &impl CoAPClient,
        ip_address: &String,
        timeout: Duration,
    ) -> Option<DiscoveryResult> {
        let endpoint = format!("coap://{}:5683/well-known/core", ip_address);
        info!("Discovering resources on endpoint {}", endpoint);

        let response = coap_client.get_with_timeout(endpoint.as_str(), timeout);

        match response {
            Ok(response) => {
                let payload = String::from_utf8(response.message.payload)
                    .expect("Received payload is not a string");
                info!("Device responded with {}", payload);

                self.parse_payload(ip_address, &payload)
            }
            Err(e) => {
                info!("Error requesting resource discovery to device: {}", e);
                None
            }
        }
    }

    fn discover_multicast(
        &self,
        coap_client: &impl CoAPClient,
        timeout: Duration,
    ) -> std::io::Result<Vec<DiscoveryResult>> {
        let mut packet: CoapRequest<SocketAddr> = CoapRequest::new();
        packet.set_path("/well-known/core");

        coap_client.send_all_coap(&packet, 0)?;
        coap_client.set_receive_timeout(Some(timeout))?;

        let mut results = Vec::new();

        while let Ok((response, src)) = coap_client.receive_from() {
            let ip_addr = src.ip().to_string();
            let payload = String::from_utf8(response.message.payload)
                .expect("Received payload is not a string");

            info!(
                "Device {} responded multicast with payload {}",
                ip_addr, payload
            );

            let result = self.parse_payload(&ip_addr, &payload);

            if let Some(r) = result {
                results.push(r)
            }
        }

        Ok(results)
    }

    fn parse_payload(&self, ip_address: &String, payload: &String) -> Option<DiscoveryResult> {
        let mut properties: HashMap<String, String> = HashMap::new();
        let resources = discovery_impl::parse_link_value(payload.as_str());
        let resource_types: Vec<String> = resources
            .iter()
            .map(|(_uri, rtype)| rtype.clone())
            .collect();

        properties.insert(COAP_IP_LABEL_ID.to_string(), ip_address.clone());
        properties.insert(
            COAP_RESOURCE_TYPES_LABEL_ID.to_string(),
            resource_types.join(","),
        );

        for (uri, rtype) in resources {
            properties.insert(rtype, uri);
        }

        let result = DiscoveryResult::new(ip_address.as_str(), properties, false);

        Some(result)
    }
}

#[async_trait]
impl DiscoveryHandler for CoAPCoREDiscoveryHandler {
    async fn discover(&self) -> Result<Vec<DiscoveryResult>, anyhow::Error> {
        let multicast = &self.discovery_handler_config.multicast;
        let static_addrs = &self.discovery_handler_config.static_ip_addresses;
        let multicast_addr = &self.discovery_handler_config.multicast_ip_address;
        let timeout = Duration::from_secs(5); // TODO: make timeout configurable
        let mut results: Vec<DiscoveryResult> = vec![];

        // Discover devices via static IPs
        for ip_address in static_addrs {
            let coap_client = CoAPClientImpl::new((ip_address.as_str(), 5683));
            let result = self.discover_endpoint(&coap_client, ip_address, timeout);

            if let Some(result) = result {
                results.push(result);
            }
        }

        // Discover devices via multicast
        if *multicast {
            let coap_client = CoAPClientImpl::new((multicast_addr.as_str(), 5683));
            let discovered = self.discover_multicast(&coap_client, timeout);

            match discovered {
                Ok(mut rs) => {
                    results.append(&mut rs);
                }
                Err(e) => {
                    return Err(anyhow::format_err!(
                        "Error while discovering devices via multicast {}",
                        e
                    ));
                }
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
    use std::net::{IpAddr, Ipv4Addr};

    use super::*;
    use akri_shared::coap_core::MockCoAPClient;
    use coap_lite::{CoapResponse, MessageType, Packet};
    use mockall::predicate::eq;

    fn create_core_response() -> CoapResponse {
        let mut request = Packet::new();
        request.header.set_type(MessageType::Confirmable);

        let mut response = CoapResponse::new(&request).unwrap();

        response.message.payload = br#"</sensors/temp>;rt="oic.r.temperature";if="sensor",
                </sensors/light>;rt="oic.r.light.brightness";if="sensor""#
            .to_vec();

        response
    }

    fn configure_unicast_response(mock: &mut MockCoAPClient, timeout: Duration) {
        Box::new(mock.expect_get_with_timeout())
            .withf(move |_url, tm| *tm == timeout)
            .returning(|_url, _timeout| Ok(create_core_response()));
    }

    #[tokio::test]
    async fn test_discover_resources_via_ip_addresses() {
        // TODO: find better way than setting env variable, which could be shared to other tests
        std::env::set_var("AGENT_NODE_NAME", "node-1");
        let mut mock_coap_client = MockCoAPClient::new();
        let timeout = Duration::from_secs(5);
        configure_unicast_response(&mut mock_coap_client, timeout);

        let ip_address = String::from("127.0.0.1");
        let config = CoAPCoREDiscoveryHandlerConfig {
            multicast: false,
            multicast_ip_address: String::from("224.0.1.187"),
            static_ip_addresses: vec![ip_address.clone()],
        };
        let handler = CoAPCoREDiscoveryHandler::new(&config);
        let result = handler
            .discover_endpoint(&mock_coap_client, &ip_address, timeout)
            .unwrap();

        assert_eq!(
            result.properties.get(COAP_IP_LABEL_ID),
            Some(&"127.0.0.1".to_string())
        );
        assert_eq!(
            result.properties.get(COAP_RESOURCE_TYPES_LABEL_ID),
            Some(&"oic.r.temperature,oic.r.light.brightness".to_string())
        );
        assert_eq!(
            result.properties.get("oic.r.temperature"),
            Some(&"/sensors/temp".to_string())
        );
        assert_eq!(
            result.properties.get("oic.r.light.brightness"),
            Some(&"/sensors/light".to_string())
        );
    }

    fn configure_multicast_scenario(mock: &mut MockCoAPClient, timeout: Duration) {
        mock.expect_send_all_coap()
            .times(1)
            .returning(|_, _| Ok(()));

        mock.expect_set_receive_timeout()
            .with(eq(Some(timeout)))
            .returning(|_| Ok(()));

        let mut count = 0;

        // Receive response from 2 devices then time out
        mock.expect_receive_from().times(3).returning(move || {
            count += 1;

            let response = create_core_response();
            let src = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 5683);

            if count <= 2 {
                Ok((response, src))
            } else {
                Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "Timed out",
                ))
            }
        });
    }

    #[tokio::test]
    async fn test_discover_resources_via_discovery() {
        env_logger::try_init().unwrap();

        // TODO: find better way than setting env variable, which could be shared to other tests
        std::env::set_var("AGENT_NODE_NAME", "node-1");
        let mut mock_coap_client = MockCoAPClient::new();
        let timeout = Duration::from_secs(1);
        configure_multicast_scenario(&mut mock_coap_client, timeout.clone());

        let config = CoAPCoREDiscoveryHandlerConfig {
            multicast: true,
            multicast_ip_address: String::from("224.0.1.187"),
            static_ip_addresses: vec![],
        };
        let handler = CoAPCoREDiscoveryHandler::new(&config);
        let results = handler
            .discover_multicast(&mock_coap_client, timeout.clone())
            .unwrap();

        assert_eq!(results.len(), 2);
    }
}
