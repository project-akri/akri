use super::super::{DiscoveryHandler, DiscoveryResult};
use super::filter::filter;
use akri_shared::akri::configuration::ZeroconfDiscoveryHandlerConfig;
use async_trait::async_trait;
use failure::Error;
use std::{
    any::Any,
    collections::HashMap,
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
