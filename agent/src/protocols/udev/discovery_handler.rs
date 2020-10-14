use super::super::{DiscoveryHandler, DiscoveryResult};
use super::{discovery_impl, udev_enumerator, UDEV_DEVNODE_LABEL_ID};
use akri_shared::akri::configuration::UdevDiscoveryHandlerConfig;
use async_trait::async_trait;
use failure::Error;
use std::collections::HashSet;

/// `UdevDiscoveryHandler` discovers udev instances by parsing the udev rules in `discovery_handler_config.udev_rules`.
/// The instances it discovers are always unshared.
#[derive(Debug)]
pub struct UdevDiscoveryHandler {
    discovery_handler_config: UdevDiscoveryHandlerConfig,
}

impl UdevDiscoveryHandler {
    pub fn new(discovery_handler_config: &UdevDiscoveryHandlerConfig) -> Self {
        UdevDiscoveryHandler {
            discovery_handler_config: discovery_handler_config.clone(),
        }
    }
}

#[async_trait]
impl DiscoveryHandler for UdevDiscoveryHandler {
    async fn discover(&self) -> Result<Vec<DiscoveryResult>, Error> {
        let udev_rules = self.discovery_handler_config.udev_rules.clone();
        trace!("discover - for udev rules {:?}", udev_rules);
        let mut devpaths: HashSet<String> = HashSet::new();
        udev_rules.iter().for_each(|rule| {
            let enumerator = udev_enumerator::create_enumerator();
            match discovery_impl::do_parse_and_find(enumerator, &rule) {
                Ok(paths) => paths.into_iter().for_each(|path| {
                    devpaths.insert(path);
                }),
                Err(e) => error!(
                    "discover - for rule {} do_parse_and_find returned error {}",
                    rule, e
                ),
            }
        });
        trace!(
            "discover - mapping and returning devices at devpaths {:?}",
            devpaths
        );
        Ok(devpaths
            .into_iter()
            .map(|path| {
                let mut properties = std::collections::HashMap::new();
                properties.insert(UDEV_DEVNODE_LABEL_ID.to_string(), path.clone());
                DiscoveryResult::new(&path, properties, self.are_shared().unwrap())
            })
            .collect::<Vec<DiscoveryResult>>())
    }

    fn are_shared(&self) -> Result<bool, Error> {
        Ok(false)
    }
}
