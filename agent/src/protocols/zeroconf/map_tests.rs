use super::constants::*;
use super::id::id;
use super::map::map;
use crate::protocols::DiscoveryResult;
use std::collections::HashMap;
use zeroconf::{browser::ServiceDiscoveryBuilder, TxtRecord};

#[test]
fn test_map_none_txt_records() {
    let service = ServiceDiscoveryBuilder::default()
        .kind(KIND.to_string())
        .name(NAME.to_string())
        .domain(DOMAIN.to_string())
        .host_name(HOST.to_string())
        .address(ADDR.to_string())
        .port(PORT)
        .txt(None)
        .build()
        .expect("service");

    let mut props: HashMap<String, String> = HashMap::new();
    props.insert(BROKER_NAME.to_string(), "zeroconf".to_string());
    props.insert(DEVICE_KIND.to_string(), service.kind().to_string());
    props.insert(DEVICE_NAME.to_string(), service.name().to_string());
    props.insert(DEVICE_HOST.to_string(), service.host_name().to_string());
    props.insert(DEVICE_ADDR.to_string(), service.address().to_string());
    props.insert(DEVICE_PORT.to_string(), service.port().to_string());

    let discovery_result = DiscoveryResult::new(&id(&service), props, true);

    assert_eq!(map(service), discovery_result);
}
#[test]
fn test_map_some_but_empty_txt_records() {
    let txt_records: HashMap<String, String> = HashMap::new();

    let service = ServiceDiscoveryBuilder::default()
        .kind(KIND.to_string())
        .name(NAME.to_string())
        .domain(DOMAIN.to_string())
        .host_name(HOST.to_string())
        .address(ADDR.to_string())
        .port(PORT)
        .txt(Some(TxtRecord::from(txt_records)))
        .build()
        .expect("service");

    let mut props: HashMap<String, String> = HashMap::new();
    props.insert(BROKER_NAME.to_string(), "zeroconf".to_string());
    props.insert(DEVICE_KIND.to_string(), service.kind().to_string());
    props.insert(DEVICE_NAME.to_string(), service.name().to_string());
    props.insert(DEVICE_HOST.to_string(), service.host_name().to_string());
    props.insert(DEVICE_ADDR.to_string(), service.address().to_string());
    props.insert(DEVICE_PORT.to_string(), service.port().to_string());

    let discovery_result = DiscoveryResult::new(&id(&service), props, true);

    let a = map(service);

    assert_eq!(a, discovery_result);
}
#[test]
fn test_map_some_txt_records() {
    let mut txt_records: HashMap<String, String> = HashMap::new();
    txt_records.insert("project".to_string(), "akri".to_string());
    txt_records.insert("protocol".to_string(), "zeroconf".to_string());

    let service = ServiceDiscoveryBuilder::default()
        .kind(KIND.to_string())
        .name(NAME.to_string())
        .domain(DOMAIN.to_string())
        .host_name(HOST.to_string())
        .address(ADDR.to_string())
        .port(PORT)
        .txt(Some(TxtRecord::from(txt_records)))
        .build()
        .expect("service");

    let mut props: HashMap<String, String> = HashMap::new();
    props.insert(BROKER_NAME.to_string(), "zeroconf".to_string());
    props.insert(DEVICE_KIND.to_string(), service.kind().to_string());
    props.insert(DEVICE_NAME.to_string(), service.name().to_string());
    props.insert(DEVICE_HOST.to_string(), service.host_name().to_string());
    props.insert(DEVICE_ADDR.to_string(), service.address().to_string());
    props.insert(DEVICE_PORT.to_string(), service.port().to_string());

    let mut txt_records: HashMap<String, String> = HashMap::new();
    txt_records.insert("project".to_string(), "akri".to_string());
    txt_records.insert("protocol".to_string(), "zeroconf".to_string());

    for (key, value) in txt_records.iter() {
        props.insert(
            format!("{}_{}", DEVICE_ENVS, key.to_ascii_uppercase()),
            value.to_owned(),
        );
    }

    let discovery_result = DiscoveryResult::new(&id(&service), props, true);

    assert_eq!(map(service), discovery_result);
}
