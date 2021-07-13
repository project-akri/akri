use super::discovery_utils::{parse_link_value, CoAPClient, CoAPClientImpl};
use akri_discovery_utils::discovery::{
    discovery_handler::{deserialize_discovery_details, DISCOVERED_DEVICES_CHANNEL_CAPACITY},
    v0::{discovery_handler_server::DiscoveryHandler, Device, DiscoverRequest, DiscoverResponse},
    DiscoverStream,
};
use async_trait::async_trait;
use coap_lite::CoapRequest;
use log::{debug, error, info};
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::delay_for;
use tonic::{Response, Status};

// TODO: make this configurable
pub const DISCOVERY_INTERVAL_SECS: u64 = 10;

pub const COAP_RESOURCE_TYPES_LABEL_ID: &str = "COAP_RESOURCE_TYPES";
pub const COAP_IP_LABEL_ID: &str = "COAP_IP";

pub const COAP_PREFIX: &str = "coap://";
pub const COAP_PORT: u16 = 5683;

/// This defines a query filter. The RFC7252 allows only one filter element.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct QueryFilter {
    pub name: String,
    pub value: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CoAPDiscoveryDetails {
    pub multicast: bool,
    pub multicast_ip_address: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub static_ip_addresses: Vec<String>,
    pub query_filter: Option<QueryFilter>,
    pub discovery_timeout_seconds: u32,
}

pub struct DiscoveryHandlerImpl {
    register_sender: tokio::sync::mpsc::Sender<()>,
}

impl DiscoveryHandlerImpl {
    pub fn new(register_sender: tokio::sync::mpsc::Sender<()>) -> Self {
        DiscoveryHandlerImpl { register_sender }
    }
}

#[async_trait]
impl DiscoveryHandler for DiscoveryHandlerImpl {
    type DiscoverStream = DiscoverStream;
    async fn discover(
        &self,
        request: tonic::Request<DiscoverRequest>,
    ) -> Result<Response<Self::DiscoverStream>, Status> {
        info!("discover - coap discovery handler started");

        let mut register_sender = self.register_sender.clone();
        let discover_request = request.get_ref();
        let (mut discovered_devices_sender, discovered_devices_receiver) =
            mpsc::channel(DISCOVERED_DEVICES_CHANNEL_CAPACITY);
        let discovery_handler_config: CoAPDiscoveryDetails =
            deserialize_discovery_details(&discover_request.discovery_details)
                .map_err(|e| tonic::Status::new(tonic::Code::InvalidArgument, format!("{}", e)))?;

        debug!(
            "discover - applying coap discovery config {:?}",
            discovery_handler_config
        );

        let multicast = discovery_handler_config.multicast;
        let static_addrs = discovery_handler_config.static_ip_addresses;
        let multicast_addr = discovery_handler_config.multicast_ip_address;
        let query_filter = discovery_handler_config.query_filter;
        let timeout =
            Duration::from_secs(discovery_handler_config.discovery_timeout_seconds as u64);

        tokio::spawn(async move {
            loop {
                let mut devices: Vec<Device> = Vec::new();

                // Discover devices via static IPs
                static_addrs.iter().for_each(|ip_address| {
                    let coap_client = CoAPClientImpl::new((ip_address.as_str(), COAP_PORT));
                    let device = discover_endpoint(
                        &coap_client,
                        &ip_address,
                        query_filter.as_ref(),
                        timeout,
                    );

                    match device {
                        Ok(device) => devices.push(device),
                        Err(e) => {
                            info!(
                                "discover - discovering endpoint {} went wrong: {}",
                                ip_address, e
                            );
                        }
                    }
                });

                // Discover devices via multicast
                if multicast {
                    let coap_client = CoAPClientImpl::new((multicast_addr.as_str(), COAP_PORT));
                    let discovered =
                        discover_multicast(&coap_client, query_filter.as_ref(), timeout);

                    match discovered {
                        Ok(mut discovered) => {
                            devices.append(&mut discovered);
                        }
                        Err(e) => {
                            error!("Error while discovering devices via multicast {}", e);
                        }
                    }
                }

                if let Err(e) = discovered_devices_sender
                    .send(Ok(DiscoverResponse { devices }))
                    .await
                {
                    error!(
                        "discover - for CoAP failed to send discovery response with error {}",
                        e
                    );
                    register_sender.send(()).await.unwrap();
                    break;
                }

                delay_for(Duration::from_secs(DISCOVERY_INTERVAL_SECS)).await;
            }
        });

        info!("discover - coap discovery handler end");
        Ok(Response::new(discovered_devices_receiver))
    }
}

fn discover_endpoint(
    coap_client: &impl CoAPClient,
    ip_address: &String,
    query_filter: Option<&QueryFilter>,
    timeout: Duration,
) -> Result<Device, anyhow::Error> {
    let endpoint = format!(
        "{}{}:{}{}",
        COAP_PREFIX,
        ip_address,
        COAP_PORT,
        build_path(query_filter)
    );

    info!("discover - discovering resources on endpoint {}", endpoint);

    let response = coap_client.get_with_timeout(endpoint.as_str(), timeout);

    match response {
        Ok(response) => {
            let payload = String::from_utf8(response.message.payload)
                .expect("Received payload is not a string");
            info!(
                "discover - device {} responded to unicast request with {}",
                ip_address, payload
            );

            let parsed = parse_payload(ip_address, query_filter, &payload);

            match parsed {
                Some(result) => Ok(result),
                None => Err(anyhow::format_err!(
                    "Could not find any resource in the parsed payload"
                )),
            }
        }
        Err(e) => Err(anyhow::format_err!(
            "Error requesting resource discovery to device: {}",
            e
        )),
    }
}

fn discover_multicast(
    coap_client: &impl CoAPClient,
    query_filter: Option<&QueryFilter>,
    timeout: Duration,
) -> Result<Vec<Device>, anyhow::Error> {
    use std::net::SocketAddr;

    let mut packet: CoapRequest<SocketAddr> = CoapRequest::new();
    packet.set_path(build_path(query_filter).as_str());

    coap_client.send_all_coap(&packet, 0)?;
    coap_client.set_receive_timeout(Some(timeout))?;

    let mut results = Vec::new();

    while let Ok((response, src)) = coap_client.receive_from() {
        let ip_addr = src.ip().to_string();
        let payload =
            String::from_utf8(response.message.payload).expect("Received payload is not a string");
        info!(
            "discover - device {} responded to the multicast request with payload {}",
            ip_addr, payload
        );

        let result = parse_payload(&ip_addr, query_filter, &payload);

        if let Some(r) = result {
            results.push(r)
        }
    }

    Ok(results)
}

fn parse_payload(
    ip_address: &String,
    query_filter: Option<&QueryFilter>,
    payload: &String,
) -> Option<Device> {
    let mut properties: HashMap<String, String> = HashMap::new();
    let mut resources = parse_link_value(payload.as_str());

    // Check the parsed resources because CoAP devices are allowed to ignore query filters
    if let Some(qf) = query_filter {
        resources = resources
            .into_iter()
            .filter(|(uri, rtype)| {
                let is_uri_okay = qf.name != String::from("href") || *uri == qf.value;
                // TODO: support wildcart syntax
                let is_type_okay = qf.name != String::from("rt") || *rtype == qf.value;

                is_uri_okay && is_type_okay
            })
            .collect();
    }

    // Don't register devices without any resource
    if resources.is_empty() {
        return None;
    }

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

    Some(Device {
        id: ip_address.clone(),
        properties,
        mounts: Vec::default(),
        device_specs: Vec::default(),
    })
}

fn build_path(query_filter: Option<&QueryFilter>) -> String {
    if let Some(qf) = query_filter {
        format!("/well-known/core?{}={}", qf.name, qf.value)
    } else {
        String::from("/well-known/core")
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    use super::super::discovery_utils::MockCoAPClient;
    use super::*;
    use akri_discovery_utils::discovery::v0::DiscoverRequest;
    use akri_shared::akri::configuration::DiscoveryHandlerInfo;
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
        mock.expect_get_with_timeout()
            .withf(move |_url, tm| *tm == timeout)
            .returning(|_url, _timeout| Ok(create_core_response()));
    }

    #[tokio::test]
    async fn test_basic_discover_ok() {
        // Set node name for generating instance id
        std::env::set_var("AGENT_NODE_NAME", "node-1");
        let mut mock_coap_client = MockCoAPClient::new();
        let timeout = Duration::from_secs(5);
        configure_unicast_response(&mut mock_coap_client, timeout);

        let coap_yaml = r#"
          name: coap
          discoveryDetails: |+
              multicast: false
              multicastIpAddress: 224.0.1.187
              staticIpAddresses: []
              discoveryTimeoutSeconds: 10
        "#;
        let deserialized: DiscoveryHandlerInfo = serde_yaml::from_str(&coap_yaml).unwrap();
        let (register_sender, _register_receiver) = tokio::sync::mpsc::channel(2);
        let discovery_handler = DiscoveryHandlerImpl::new(register_sender);

        let discover_request = tonic::Request::new(DiscoverRequest {
            discovery_details: deserialized.discovery_details.clone(),
        });
        let mut stream = discovery_handler
            .discover(discover_request)
            .await
            .unwrap()
            .into_inner();
        let devices = stream.recv().await.unwrap().unwrap().devices;

        assert_eq!(0, devices.len());
    }

    #[tokio::test]
    async fn test_discover_resources_via_ip_addresses() {
        // Set node name for generating instance id
        std::env::set_var("AGENT_NODE_NAME", "node-1");
        let mut mock_coap_client = MockCoAPClient::new();
        let timeout = Duration::from_secs(5);
        configure_unicast_response(&mut mock_coap_client, timeout);

        let ip_address = String::from("127.0.0.1");
        let query_filter = None;
        let result = discover_endpoint(
            &mock_coap_client,
            &ip_address,
            query_filter.as_ref(),
            timeout,
        )
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
    async fn test_discover_resources_via_multicast() {
        // Set node name for generating instance id
        std::env::set_var("AGENT_NODE_NAME", "node-1");
        let mut mock_coap_client = MockCoAPClient::new();
        let timeout = Duration::from_secs(1);
        configure_multicast_scenario(&mut mock_coap_client, timeout.clone());

        let query_filter = None;
        let results =
            discover_multicast(&mock_coap_client, query_filter.as_ref(), timeout.clone()).unwrap();

        assert_eq!(results.len(), 2);
    }

    fn configure_query_filter_response(mock: &mut MockCoAPClient, query: &str) {
        let pattern = format!("?{}", query);

        mock.expect_get_with_timeout()
            .withf(move |url, _tm| url.ends_with(pattern.as_str()))
            // It's okay for the response to be the same CoRE response, devices are not required to
            // support filtering
            .returning(|_url, _timeout| Ok(create_core_response()));
    }

    #[tokio::test]
    async fn test_query_filtering_href() {
        // Set node name for generating instance id
        std::env::set_var("AGENT_NODE_NAME", "node-1");
        let mut mock_coap_client = MockCoAPClient::new();
        let timeout = Duration::from_secs(5);
        configure_query_filter_response(&mut mock_coap_client, "href=/sensors/temp");

        let ip_address = String::from("127.0.0.1");
        let query_filter = Some(QueryFilter {
            name: String::from("href"),
            value: String::from("/sensors/temp"),
        });
        let result = discover_endpoint(
            &mock_coap_client,
            &ip_address,
            query_filter.as_ref(),
            timeout,
        )
        .unwrap();

        assert_eq!(
            result.properties.get("oic.r.temperature"),
            Some(&"/sensors/temp".to_string())
        );
        assert_eq!(result.properties.get("oic.r.light.brightness"), None);
    }

    #[tokio::test]
    async fn test_query_filtering_resource_types() {
        // Set node name for generating instance id
        std::env::set_var("AGENT_NODE_NAME", "node-1");
        let mut mock_coap_client = MockCoAPClient::new();
        let timeout = Duration::from_secs(5);
        configure_query_filter_response(&mut mock_coap_client, "rt=oic.r.temperature");

        let ip_address = String::from("127.0.0.1");
        let query_filter = Some(QueryFilter {
            name: String::from("rt"),
            value: String::from("oic.r.temperature"),
        });
        let result = discover_endpoint(
            &mock_coap_client,
            &ip_address,
            query_filter.as_ref(),
            timeout,
        )
        .unwrap();

        assert_eq!(
            result.properties.get("oic.r.temperature"),
            Some(&"/sensors/temp".to_string())
        );
        assert_eq!(result.properties.get("oic.r.light.brightness"), None);
    }
}
