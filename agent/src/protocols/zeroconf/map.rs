use super::constants::*;
use super::id::id;
use crate::protocols::DiscoveryResult;
use std::collections::HashMap;
use zeroconf::{prelude::TTxtRecord, ServiceDiscovery};

pub fn map(service: ServiceDiscovery) -> DiscoveryResult {
    trace!("[zeroconf:discovery] Service: {:?}", service);
    let mut props = HashMap::new();

    // Create environment values to expose to each Akri Instance
    props.insert(BROKER_NAME.to_string(), "zeroconf".to_string());
    props.insert(DEVICE_KIND.to_string(), service.kind().to_string());
    props.insert(DEVICE_NAME.to_string(), service.name().to_string());
    props.insert(DEVICE_HOST.to_string(), service.host_name().to_string());
    props.insert(DEVICE_ADDR.to_string(), service.address().to_string());
    props.insert(DEVICE_PORT.to_string(), service.port().to_string());

    // Map (any) TXT records
    // Prefix TXT records keys with a constant
    match service.txt() {
        Some(txt_records) => {
            trace!("[zeroconf:discovery] TXT records: some");
            // Covers edge-case in which TXT records property exists but is empty
            if !txt_records.is_empty() {
                for (key, value) in txt_records.iter() {
                    props.insert(
                        format!("{}_{}", DEVICE_ENVS, key.to_ascii_uppercase()),
                        value,
                    );
                }
            }
        }
        None => trace!("[zeroconf:discovery] TXT records: none"),
    }

    DiscoveryResult::new(&id(&service), props, true)
}
