use super::super::{DiscoveryHandler, DiscoveryResult};
use akri_shared::akri::configuration::ZeroConfDiscoveryHandlerConfig;
use async_trait::async_trait;
use failure::Error;
use std::sync::mpsc::{Receiver, Sender};
use std::{
    collections::HashMap,
    fs,
    time::{Duration, Instant},
};
use zeroconf::{MdnsBrowser, ServiceDiscovery};

const BROKER_NAME: &str = "AKRI_ZEROCONF";
const DEVICE_NAME: &str = "AKRI_ZEROCONF_DEVICE_NAME";
const DEVICE_HOST: &str = "AKRI_ZEROCONF_DEVICE_HOST";
const DEVICE_ADDR: &str = "AKRI_ZEROCONF_DEVICE_ADDR";
const DEVICE_PORT: &str = "AKRI_ZEROCONF_DEVICE_PORT";

#[derive(Debug)]
pub struct ZeroConfDiscoveryHandler {
    discovery_handler_config: ZeroConfDiscoveryHandlerConfig,
}

impl ZeroConfDiscoveryHandler {
    async fn discover(&self) -> Result<Vec<DiscoveryResult>, Error> {
        println!("[zeroconf::discover] Entered");

        let mut browser = MdnsBrowser::new(discovery_handler_config.kind);

        // Channel for results
        let (mut tx, rx): (Sender<ServiceDiscovery>, Receiver<ServiceDiscovery>) = mpsc::channel();

        // Browser will return Services as discovered
        // Closes over `tx`
        browser.set_service_discovered_callback(Box::new(
            move |result: zeroconf::Result<ServiceDiscovery>, _context: Option<Arc<dyn Any>>| {
                match result {
                    Ok(service) => {
                        println!("[zeroconf:discovery:λ] Service Discovered: {:?}", service);
                        tx.send(service).unwrap();
                    }
                    Err(e) => {
                        println!("[zeroconf:discovery:λ] Error: {:?}", e);
                    }
                };
            },
        ));

        let event_loop = browser.browse_services().unwrap();

        println!("[zeroconf:discovery] Started browsing");
        let now = Instant::now();
        let end = now.add(Duration::from_secs(5));
        while now.elapsed() < end {
            event_loop.poll(Duration::from_secs(0)).unwrap();
        }
        println!("[zeroconf:discovery] Stopped browsing");

        // Receive
        println!("[zeroconf:discovery] Iterating over services");
        let result = rx.iter().map(|service| {
            println!("[zeroconf:discovery] Service: {:?}", service);
            let mut props = HashMap::new();
            props.insert(BROKER_NAME.to_string(), "zeroconf".to_string());
            props.insert(DEVICE_NAME.to_string(), serice.name);
            props.insert(DEVICE_HOST.to_string(), service.host_name);
            props.insert(DEVICE_ADDR.to_string(), service.address);
            props.insert(DEVICE_PORT.to_string(), serivce.port.to_string());
            DR::new(endpoint, props, true)
        }).collect::<Vec<DiscoveryResult>>;
        Ok(result)
    }
    fn are_shared(&self) -> Result<bool, Error> {
        println!("[zeroconf::are_shared] Entered");
        Ok(true)
    }
}
