use super::filter::filter;
use akri_shared::akri::configuration::ZeroconfDiscoveryHandlerConfig;
use std::collections::{hash_map::RandomState, HashMap};
use zeroconf::{browser::ServiceDiscoveryBuilder, TxtRecord};

// `kind` is not considered by filter; so no tests matching `kind` are included here
const KIND: &str = "_rust._tcp";

const NAME: &str = "freddie";
const DOMAIN: &str = "local";

// HOST = NAME.DOMAIN
const HOST: &str = "freddie.local";

const ADDR: &str = "127.0.0.1";
const PORT: u16 = 8888;

#[test]
fn test_parse_all_none() {
    let config = ZeroconfDiscoveryHandlerConfig {
        kind: KIND.to_string(),
        name: None,
        domain: None,
        port: None,
        txt_records: None,
    };
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

    assert!(filter(&config, &service));
}
#[test]
fn test_parse_name_match() {
    let config = ZeroconfDiscoveryHandlerConfig {
        kind: KIND.to_string(),
        name: Some(NAME.to_string()),
        domain: None,
        port: None,
        txt_records: None,
    };
    let service = ServiceDiscoveryBuilder::default()
        .kind(KIND.to_string())
        .name(NAME.to_string())
        .host_name(HOST.to_string())
        .domain(DOMAIN.to_string())
        .address(ADDR.to_string())
        .port(PORT)
        .txt(None)
        .build()
        .expect("service");

    assert!(filter(&config, &service));
}
#[test]
fn test_parse_name_nomatch() {
    let config = ZeroconfDiscoveryHandlerConfig {
        kind: KIND.to_string(),
        name: Some("difference".to_string()),
        domain: None,
        port: None,
        txt_records: None,
    };
    let service = ServiceDiscoveryBuilder::default()
        .kind(KIND.to_string())
        .name(NAME.to_string())
        .host_name(HOST.to_string())
        .domain(DOMAIN.to_string())
        .address(ADDR.to_string())
        .port(PORT)
        .txt(None)
        .build()
        .expect("service");

    assert_eq!(false, filter(&config, &service));
}
#[test]
fn test_parse_host_match() {
    let config = ZeroconfDiscoveryHandlerConfig {
        name: Some(NAME.to_string()),
        kind: KIND.to_string(),
        domain: Some(DOMAIN.to_string()),
        port: None,
        txt_records: None,
    };
    let service = ServiceDiscoveryBuilder::default()
        .kind(KIND.to_string())
        .name(NAME.to_string())
        .host_name(HOST.to_string())
        .domain(DOMAIN.to_string())
        .address(ADDR.to_string())
        .port(PORT)
        .txt(None)
        .build()
        .expect("service");

    assert!(filter(&config, &service));
}
#[test]
fn test_parse_host_nomatch() {
    let config = ZeroconfDiscoveryHandlerConfig {
        name: Some("different".to_string()),
        kind: KIND.to_string(),
        domain: Some(DOMAIN.to_string()),
        port: None,
        txt_records: None,
    };
    let service = ServiceDiscoveryBuilder::default()
        .kind(KIND.to_string())
        .name(NAME.to_string())
        .host_name(HOST.to_string())
        .domain(DOMAIN.to_string())
        .address(ADDR.to_string())
        .port(PORT)
        .txt(None)
        .build()
        .expect("service");

    assert_ne!(true, filter(&config, &service));
}
#[test]
fn test_parse_port_match() {
    let config = ZeroconfDiscoveryHandlerConfig {
        kind: KIND.to_string(),
        name: None,
        domain: None,
        port: Some(PORT),
        txt_records: None,
    };
    let service = ServiceDiscoveryBuilder::default()
        .kind(KIND.to_string())
        .name(NAME.to_string())
        .host_name(HOST.to_string())
        .domain(DOMAIN.to_string())
        .address(ADDR.to_string())
        .port(PORT)
        .txt(None)
        .build()
        .expect("service");

    assert!(filter(&config, &service));
}
#[test]
fn test_parse_port_nomatch() {
    let config = ZeroconfDiscoveryHandlerConfig {
        kind: KIND.to_string(),
        name: None,
        domain: None,
        port: Some(9999),
        txt_records: None,
    };
    let service = ServiceDiscoveryBuilder::default()
        .kind(KIND.to_string())
        .name(NAME.to_string())
        .host_name(HOST.to_string())
        .domain(DOMAIN.to_string())
        .address(ADDR.to_string())
        .port(PORT)
        .txt(None)
        .build()
        .expect("service");

    assert_ne!(true, filter(&config, &service));
}
#[test]
fn test_parse_txt_records_match() {
    let s = RandomState::new();
    let mut txt_records: HashMap<String, String> = HashMap::with_hasher(s);
    txt_records.insert("project".to_string(), "akri".to_string());
    txt_records.insert("protocol".to_string(), "zeroconf".to_string());

    let config = ZeroconfDiscoveryHandlerConfig {
        kind: KIND.to_string(),
        name: None,
        domain: None,
        port: None,
        txt_records: Some(txt_records),
    };

    let s = RandomState::new();
    let mut txt_records: HashMap<String, String> = HashMap::with_hasher(s);
    txt_records.insert("project".to_string(), "akri".to_string());
    txt_records.insert("protocol".to_string(), "zeroconf".to_string());

    let service = ServiceDiscoveryBuilder::default()
        .kind(KIND.to_string())
        .name(NAME.to_string())
        .host_name(HOST.to_string())
        .domain(DOMAIN.to_string())
        .address(ADDR.to_string())
        .port(PORT)
        .txt(Some(TxtRecord::from(txt_records)))
        .build()
        .expect("service");

    assert!(filter(&config, &service));
}
#[test]
fn test_parse_txt_records_nomatch_1() {
    let mut txt_records: HashMap<String, String> = HashMap::new();
    txt_records.insert("project".to_string(), "akri".to_string());
    txt_records.insert("protocol".to_string(), "zeroconf".to_string());

    let config = ZeroconfDiscoveryHandlerConfig {
        kind: KIND.to_string(),
        name: None,
        domain: None,
        port: None,
        txt_records: Some(txt_records),
    };
    let service = ServiceDiscoveryBuilder::default()
        .kind(KIND.to_string())
        .name(NAME.to_string())
        .host_name(HOST.to_string())
        .domain(DOMAIN.to_string())
        .address(ADDR.to_string())
        .port(PORT)
        .txt(None)
        .build()
        .expect("service");

    assert_ne!(true, filter(&config, &service));
}
#[test]
fn test_parse_txt_records_nomatch_2() {
    let s = RandomState::new();
    let mut txt_records: HashMap<String, String> = HashMap::with_hasher(s);
    txt_records.insert("project".to_string(), "akri".to_string());
    txt_records.insert("protocol".to_string(), "zeroconf".to_string());

    let config = ZeroconfDiscoveryHandlerConfig {
        kind: KIND.to_string(),
        name: None,
        domain: None,
        port: None,
        txt_records: Some(txt_records),
    };

    let s = RandomState::new();
    let mut txt_records: HashMap<String, String> = HashMap::with_hasher(s);
    txt_records.insert("project".to_string(), "akri".to_string());
    txt_records.insert("protocol".to_string(), "different".to_string());

    let service = ServiceDiscoveryBuilder::default()
        .kind(KIND.to_string())
        .name(NAME.to_string())
        .host_name(HOST.to_string())
        .domain(DOMAIN.to_string())
        .address(ADDR.to_string())
        .port(PORT)
        .txt(None)
        .build()
        .expect("service");

    assert_ne!(true, filter(&config, &service));
}
