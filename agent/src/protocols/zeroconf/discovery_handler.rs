use super::super::{DiscoveryHandler, DiscoveryResult};
use akri_shared::akri::configuration::ZeroConfDiscoveryHandlerConfig;
use async_trait::async_trait;
use failure::Error;
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
use zeroconf::browser::TMdnsBrowser;
use zeroconf::event_loop::TEventLoop;
use zeroconf::{MdnsBrowser, ServiceDiscovery};

const BROKER_NAME: &str = "AKRI_ZEROCONF";
const DEVICE_KIND: &str = "AKRI_ZEROCONF_DEVICE_KIND";
const DEVICE_NAME: &str = "AKRI_ZEROCONF_DEVICE_NAME";
const DEVICE_HOST: &str = "AKRI_ZEROCONF_DEVICE_HOST";
const DEVICE_ADDR: &str = "AKRI_ZEROCONF_DEVICE_ADDR";
const DEVICE_PORT: &str = "AKRI_ZEROCONF_DEVICE_PORT";

#[derive(Debug)]
pub struct ZeroConfDiscoveryHandler {
    discovery_handler_config: ZeroConfDiscoveryHandlerConfig,
}
impl ZeroConfDiscoveryHandler {
    pub fn new(discovery_handler_config: &ZeroConfDiscoveryHandlerConfig) -> Self {
        println!("[zeroconf:new] Entered");
        ZeroConfDiscoveryHandler {
            discovery_handler_config: discovery_handler_config.clone(),
        }
    }
}
#[async_trait]
impl DiscoveryHandler for ZeroConfDiscoveryHandler {
    async fn discover(&self) -> Result<Vec<DiscoveryResult>, Error> {
        println!("[zeroconf:discover] Entered");

        // TODO(dazwilkin) Apply filter, domain etc. to browser
        // TODO(dazwilkin) Validate kind: Kubernetes only supports TCP, UDP and SCTP

        let mut browser = MdnsBrowser::new(&self.discovery_handler_config.kind);

        // Channel for results
        let (tx, rx): (Sender<ServiceDiscovery>, Receiver<ServiceDiscovery>) = channel();

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

        println!("[zeroconf:discovery] Started browsing");
        let event_loop = browser.browse_services().unwrap();
        let now = Instant::now();
        let end = Duration::from_secs(5);
        while now.elapsed() < end {
            event_loop.poll(Duration::from_secs(0)).unwrap();
        }
        println!("[zeroconf:discovery] Stopped browsing");
        // Explicitly drop browser to close the channel to ensure receive iteration completes
        drop(browser);

        // Receive
        println!("[zeroconf:discovery] Iterating over services");
        let result = rx
            .iter()
            .map(|service| {
                println!("[zeroconf:discovery] Service: {:?}", service);
                let mut props = HashMap::new();
                props.insert(BROKER_NAME.to_string(), "zeroconf".to_string());
                props.insert(DEVICE_KIND.to_string(), service.kind().to_string());
                props.insert(DEVICE_NAME.to_string(), service.name().to_string());
                props.insert(DEVICE_HOST.to_string(), service.host_name().to_string());
                props.insert(DEVICE_ADDR.to_string(), service.address().to_string());
                props.insert(DEVICE_PORT.to_string(), service.port().to_string());
                DiscoveryResult::new(service.host_name(), props, true)
            })
            .collect::<Vec<DiscoveryResult>>();

        println!("[zeroconf:discovery] Result: {:?}", result);
        Ok(result)
    }
    fn are_shared(&self) -> Result<bool, Error> {
        println!("[zeroconf::are_shared] Entered");
        Ok(true)
    }
}
