use super::constants::*;
use super::id::id;
use crate::protocols::{zeroconf::ZeroconfDiscoveryHandler, DiscoveryHandler, DiscoveryResult};
use akri_shared::akri::configuration::ZeroconfDiscoveryHandlerConfig;
use std::collections::HashMap;
use tokio_test::assert_ok;
use zeroconf::{browser::ServiceDiscoveryBuilder, ServiceDiscovery, TxtRecord};

#[test]
fn test_discover_multiple_services() {
    let config = ZeroconfDiscoveryHandlerConfig {
        kind: KIND.to_string(),
        name: None,
        domain: None,
        port: None,
        txt_records: None,
    };
    let handler = ZeroconfDiscoveryHandler::new(&config);

    assert_ok!(tokio_test::block_on(handler.discover()));
}
#[test]
fn test_transform() {
    let config = ZeroconfDiscoveryHandlerConfig {
        kind: KIND.to_string(),
        name: None,
        domain: None,
        port: None,
        txt_records: None,
    };
    let handler = ZeroconfDiscoveryHandler::new(&config);

    let service: ServiceDiscovery = ServiceDiscoveryBuilder::default()
        .kind(KIND.to_string())
        .name(NAME.to_string())
        .domain(DOMAIN.to_string())
        .host_name(HOST.to_string())
        .address(ADDR.to_string())
        .port(PORT)
        .txt(None)
        .build()
        .expect("service");

    let id = id(&service);
    let services = vec![service];

    let result = handler.transform(services);

    // Contains a single result
    let dr = &result[0];

    let mut props: HashMap<String, String> = HashMap::new();
    props.insert(BROKER_NAME.to_string(), "zeroconf".to_string());
    props.insert(DEVICE_KIND.to_string(), KIND.to_string());
    props.insert(DEVICE_NAME.to_string(), NAME.to_string());
    props.insert(DEVICE_HOST.to_string(), HOST.to_string());
    props.insert(DEVICE_ADDR.to_string(), ADDR.to_string());
    props.insert(DEVICE_PORT.to_string(), PORT.to_string());

    assert_eq!(dr, &DiscoveryResult::new(&id, props, true));
}
