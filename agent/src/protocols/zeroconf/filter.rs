use akri_shared::akri::configuration::ZeroconfDiscoveryHandlerConfig;
use zeroconf::{prelude::TTxtRecord, ServiceDiscovery};

// TODO(dazwilkin) Refactor :-)
pub fn filter(config: &ZeroconfDiscoveryHandlerConfig, service: &ServiceDiscovery) -> bool {
    trace!("[zeroconf:filter] Service: {:?}", service);
    let include = match &config.name {
        Some(name) => {
            let result = name == service.name();
            trace!("[zeroconf:filter] Name ({}) [{}]", name, result);
            result
        }
        None => true,
    } && match &config.domain {
        Some(domain) => {
            let result = domain == service.domain();
            trace!("[zeroconf:filter] Domain ({}) [{}]", domain, result);
            result
        }
        None => true,
    } && match config.port {
        Some(port) => {
            let result = &port == service.port();
            trace!("[zeroconf:filter] Port ({}) [{}]", port, result);
            result
        }
        None => true,
    } && match &config.txt_records {
        // The config has TXT records
        Some(txt_records) => {
            trace!(
                "[zeroconf:filter] TXT Records [Config={}]",
                &txt_records.len()
            );
            // Get this service's TXT records, if any
            let result = match service.txt() {
                // The service has TXT records, match every one
                Some(service_txt_records) => {
                    trace!(
                        "[zeroconf:filter] TXT Records [Service={}]",
                        &service_txt_records.len()
                    );
                    // The Service must have the same key and same value in each record to pass the filter
                    let result = txt_records
                        .iter()
                        .map(|(key, value)| {
                            // Apply keys consistently as UPPERCASE
                            let key = key.to_ascii_uppercase();
                            let result = match service_txt_records.get(&key) {
                                Some(service_value) => {
                                    let result = &service_value == value;
                                    trace!(
                                        "[zeroconf:filter] TXT Record Key: {} Value: {} [{}]",
                                        key,
                                        value,
                                        result
                                    );
                                    result
                                }
                                // The service doesn't have the key so it doesn't match
                                None => false,
                            };
                            result
                        })
                        .all(|x| x == true);
                    trace!("[zeroconf:filter] TXT Records [{}]", result);
                    result
                }
                // The service has no TXT records (but the config does), can't be match
                None => false,
            };
            trace!("[zeroconf:filter] TXT Records [{}]", result);
            result
        }
        // The config has no TXT records, any service TXT records will pass
        None => true,
    };
    trace!(
        "[zeroconf:filter] {} Service: {:?}",
        if include { "INCLUDE" } else { "EXCLUDE" },
        service
    );
    include
}
