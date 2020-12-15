use super::filter::filter;
use super::map::map;
use crate::protocols::{DiscoveryHandler, DiscoveryResult};
use akri_shared::akri::configuration::ZeroconfDiscoveryHandlerConfig;
use async_trait::async_trait;
use failure::Error;
use std::{
    any::Any,
    sync::{
        mpsc::{channel, Receiver, Sender},
        Arc,
    },
    time::{Duration, Instant},
};
use zeroconf::{browser::TMdnsBrowser, event_loop::TEventLoop, MdnsBrowser, ServiceDiscovery};

const SCAN_DURATION: u64 = 5;

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
    pub fn transform<Z>(&self, services_discovered: Z) -> Vec<DiscoveryResult>
    where
        Z: IntoIterator<Item = ServiceDiscovery>,
    {
        let result = services_discovered
            .into_iter()
            .filter(|service| filter(&self.discovery_handler_config, service))
            .map(|service| map(service))
            .collect();
        result
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
        trace!("[zeroconf:discovery] Transforming services discovered into discovery results");
        let result = self.transform(rx);
        trace!("[zeroconf:discovery] Result: {:?}", result);

        Ok(result)
    }
    fn are_shared(&self) -> Result<bool, Error> {
        trace!("[zeroconf::are_shared] Entered");
        Ok(true)
    }
}
