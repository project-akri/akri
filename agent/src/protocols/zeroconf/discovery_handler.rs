use super::super::{DiscoveryHandler, DiscoveryResult};
use akri_shared::akri::configuration::ZeroconfDiscoveryHandlerConfig;
use async_trait::async_trait;
use failure::{format_err, Error};
use std::{
    any::Any,
    collections::HashMap,
    ops::Add,
    sync::{
        mpsc::{channel, Receiver, Sender},
        Arc,
    },
    time::{Duration, Instant},
};
use zeroconf::{
    browser::TMdnsBrowser, event_loop::TEventLoop, prelude::TTxtRecord, MdnsBrowser,
    ServiceDiscovery,
};

const SCAN_DURATION: u64 = 5;

const BROKER_NAME: &str = "AKRI_ZEROCONF";
const DEVICE_KIND: &str = "AKRI_ZEROCONF_DEVICE_KIND";
const DEVICE_NAME: &str = "AKRI_ZEROCONF_DEVICE_NAME";
const DEVICE_HOST: &str = "AKRI_ZEROCONF_DEVICE_HOST";
const DEVICE_ADDR: &str = "AKRI_ZEROCONF_DEVICE_ADDR";
const DEVICE_PORT: &str = "AKRI_ZEROCONF_DEVICE_PORT";

// Prefix for environment variables created from discovered device's TXT records
const DEVICE_ENVS: &str = "AKRI_ZEROCONF_DEVICE";

// TODO(dazwilkin) Refactor :-)
fn filter(config: &ZeroconfDiscoveryHandlerConfig, service: &ServiceDiscovery) -> bool {
    trace!("[zeroconf:filter] Service: {:?}", service);
    let include = (if let Some(name) = &config.name {
        let result = name == service.name();
        trace!("[zeroconf:filter] Name ({}) [{}]", name, result);
        result
    } else {
        true
    }) && (if let Some(domain) = &config.domain {
        let result = domain == service.domain();
        trace!("[zeroconf:filter] Domain ({}) [{}]", domain, result);
        result
    } else {
        true
    }) && (if let Some(port) = config.port {
        let result = &port == service.port();
        trace!("[zeroconf:filter] Port ({}) [{}]", port, result);
        result
    } else {
        true
    }) && (if let Some(txt_records) = &config.txt_records {
        // The config has TXT records
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
                            None => {
                                // The service doesn't have the key so it doesn't match
                                false
                            }
                        };
                        result
                    })
                    // The result is only true if *all* the keys and values match
                    .all(|x| x == true);
                trace!("[zeroconf:filter] TXT Records [{}]", result);
                result
            }
            None => {
                // The service has no TXT records (but the config does), can't be match
                false
            }
        };
        trace!("[zeroconf:filter] TXT Records [{}]", result);
        result
    } else {
        // If the handler has no TXT records, the service's TXT records (if any) pass
        true
    });
    trace!(
        "[zeroconf:filter] {} Service: {:?}",
        if include { "INCLUDE" } else { "EXCLUDE" },
        service
    );
    include
}

#[derive(Debug)]
pub struct ZeroconfDiscoveryHandler {
    discovery_handler_config: ZeroconfDiscoveryHandlerConfig,
}
impl ZeroconfDiscoveryHandler {
    pub fn new(discovery_handler_config: &ZeroconfDiscoveryHandlerConfig) -> Self {
        trace!("[zeroconf:new] Entered");
        ZeroconfDiscoveryHandler {
            discovery_handler_config: discovery_handler_config.clone(),
        }
    }
}
#[async_trait]
impl DiscoveryHandler for ZeroconfDiscoveryHandler {
    async fn discover(&self) -> Result<Vec<DiscoveryResult>, Error> {
        trace!("[zeroconf:discover] Entered");

        let mut browser = MdnsBrowser::new(&self.discovery_handler_config.kind);

        // Channel for results
        let (tx, rx): (Sender<ServiceDiscovery>, Receiver<ServiceDiscovery>) = channel();

        // Browser will return Services as discovered
        // Closes over `tx`
        browser.set_service_discovered_callback(Box::new(
            move |result: zeroconf::Result<ServiceDiscovery>, _context: Option<Arc<dyn Any>>| {
                match result {
                    Ok(service) => {
                        trace!("[zeroconf:discovery:λ] Service Discovered: {:?}", service);
                        tx.send(service).unwrap();
                    }
                    Err(e) => {
                        trace!("[zeroconf:discovery:λ] Error: {:?}", e);
                    }
                };
            },
        ));

        trace!("[zeroconf:discovery] Started browsing");
        let event_loop = browser.browse_services().unwrap();
        let now = Instant::now();
        let end = Duration::from_secs(SCAN_DURATION);
        while now.elapsed() < end {
            event_loop.poll(Duration::from_secs(0)).unwrap();
        }
        trace!("[zeroconf:discovery] Stopped browsing");
        // Explicitly drop browser to close the channel to ensure receive iteration completes
        drop(browser);

        // Receive
        trace!("[zeroconf:discovery] Iterating over services");
        // TODO(dazwilkin) Provide additional filtering, e.g. domain here
        let result = rx
            .iter()
            .filter(|service| filter(&self.discovery_handler_config, service))
            .map(|service| {
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
                        for (key, value) in txt_records.iter() {
                            props.insert(
                                format!("{}_{}", DEVICE_ENVS, key.to_ascii_uppercase()),
                                value,
                            );
                        }
                    }
                    None => trace!("[zeroconf:discovery] TXT records: none"),
                }

                DiscoveryResult::new(service.host_name(), props, true)
            })
            .collect::<Vec<DiscoveryResult>>();

        trace!("[zeroconf:discovery] Result: {:?}", result);
        Ok(result)
    }
    fn are_shared(&self) -> Result<bool, Error> {
        trace!("[zeroconf::are_shared] Entered");
        Ok(true)
    }
}
