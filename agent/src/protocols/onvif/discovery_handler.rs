use super::super::{DiscoveryHandler, DiscoveryResult};
use super::discovery_impl::util;
use akri_shared::akri::configuration::{FilterList, FilterType, OnvifDiscoveryHandlerConfig};
use akri_shared::onvif::device_info::{
    OnvifQuery, OnvifQueryImpl, ONVIF_DEVICE_IP_ADDRESS_LABEL_ID,
    ONVIF_DEVICE_MAC_ADDRESS_LABEL_ID, ONVIF_DEVICE_SERVICE_URL_LABEL_ID,
};
use anyhow::Error;
use async_trait::async_trait;
use std::{collections::HashMap, time::Duration};

/// `OnvifDiscoveryHandler` discovers the onvif instances as described by the filters `discover_handler_config.ip_addresses`,
/// `discover_handler_config.mac_addresses`, and `discover_handler_config.scopes`.
/// The instances it discovers are always shared.
#[derive(Debug)]
pub struct OnvifDiscoveryHandler {
    discovery_handler_config: OnvifDiscoveryHandlerConfig,
}

impl OnvifDiscoveryHandler {
    pub fn new(discovery_handler_config: &OnvifDiscoveryHandlerConfig) -> Self {
        OnvifDiscoveryHandler {
            discovery_handler_config: discovery_handler_config.clone(),
        }
    }

    fn execute_filter(filter_list: Option<&FilterList>, filter_against: &[String]) -> bool {
        if filter_list.is_none() {
            return false;
        }
        let filter_action = filter_list.as_ref().unwrap().action.clone();
        let filter_count = filter_list
            .unwrap()
            .items
            .iter()
            .filter(|pattern| {
                filter_against
                    .iter()
                    .filter(|filter_against_item| filter_against_item.contains(*pattern))
                    .count()
                    > 0
            })
            .count();

        if FilterType::Include == filter_action {
            filter_count == 0
        } else {
            filter_count != 0
        }
    }

    async fn apply_filters(
        &self,
        device_service_uris: Vec<String>,
        onvif_query: &impl OnvifQuery,
    ) -> Result<Vec<DiscoveryResult>, anyhow::Error> {
        let mut result = Vec::new();
        for device_service_url in device_service_uris.iter() {
            trace!("apply_filters - device service url {}", &device_service_url);
            let (ip_address, mac_address) = match onvif_query
                .get_device_ip_and_mac_address(&device_service_url)
                .await
            {
                Ok(ip_and_mac) => ip_and_mac,
                Err(e) => {
                    error!("apply_filters - error getting ip and mac address: {}", e);
                    continue;
                }
            };

            // Evaluate camera ip address against ip filter if provided
            let ip_address_as_vec = vec![ip_address.clone()];
            if OnvifDiscoveryHandler::execute_filter(
                self.discovery_handler_config.ip_addresses.as_ref(),
                &ip_address_as_vec,
            ) {
                continue;
            }

            // Evaluate camera mac address against mac filter if provided
            let mac_address_as_vec = vec![mac_address.clone()];
            if OnvifDiscoveryHandler::execute_filter(
                self.discovery_handler_config.mac_addresses.as_ref(),
                &mac_address_as_vec,
            ) {
                continue;
            }

            let ip_and_mac_joined = format!("{}-{}", &ip_address, &mac_address);

            // Evaluate camera scopes against scopes filter if provided
            let device_scopes = match onvif_query.get_device_scopes(&device_service_url).await {
                Ok(scopes) => scopes,
                Err(e) => {
                    error!("apply_filters - error getting scopes: {}", e);
                    continue;
                }
            };
            if OnvifDiscoveryHandler::execute_filter(
                self.discovery_handler_config.scopes.as_ref(),
                &device_scopes,
            ) {
                continue;
            }

            let mut properties = HashMap::new();
            properties.insert(
                ONVIF_DEVICE_SERVICE_URL_LABEL_ID.to_string(),
                device_service_url.to_string(),
            );
            properties.insert(ONVIF_DEVICE_IP_ADDRESS_LABEL_ID.into(), ip_address);
            properties.insert(ONVIF_DEVICE_MAC_ADDRESS_LABEL_ID.into(), mac_address);

            trace!(
                "apply_filters - returns DiscoveryResult ip/mac: {:?}, props: {:?}",
                &ip_and_mac_joined,
                &properties
            );
            result.push(DiscoveryResult::new(
                &ip_and_mac_joined,
                properties,
                self.are_shared().unwrap(),
            ))
        }
        Ok(result)
    }
}

#[async_trait]
impl DiscoveryHandler for OnvifDiscoveryHandler {
    async fn discover(&self) -> Result<Vec<DiscoveryResult>, anyhow::Error> {
        let onvif_query = OnvifQueryImpl {};

        info!("discover - filters:{:?}", &self.discovery_handler_config,);
        let discovered_onvif_cameras = util::simple_onvif_discover(Duration::from_secs(
            self.discovery_handler_config.discovery_timeout_seconds as u64,
        ))
        .await?;
        info!("discover - discovered:{:?}", &discovered_onvif_cameras,);
        let filtered_onvif_cameras = self
            .apply_filters(discovered_onvif_cameras, &onvif_query)
            .await;
        info!("discover - filtered:{:?}", &filtered_onvif_cameras);
        filtered_onvif_cameras
    }
    fn are_shared(&self) -> Result<bool, Error> {
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use akri_shared::onvif::device_info::MockOnvifQuery;

    struct IpAndMac {
        mock_uri: &'static str,
        mock_ip: &'static str,
        mock_mac: &'static str,
    }

    struct Scope {
        mock_uri: &'static str,
        mock_scope: &'static str,
    }

    fn configure_scenario(
        mock: &mut MockOnvifQuery,
        ip_and_mac: Option<IpAndMac>,
        scope: Option<Scope>,
    ) {
        if let Some(ip_and_mac_) = ip_and_mac {
            configure_get_device_ip_and_mac_address(
                mock,
                &ip_and_mac_.mock_uri,
                &ip_and_mac_.mock_ip,
                &ip_and_mac_.mock_mac,
            )
        }
        if let Some(scope_) = scope {
            configure_get_device_scopes(mock, &scope_.mock_uri, &scope_.mock_scope)
        }
    }

    fn configure_get_device_ip_and_mac_address(
        mock: &mut MockOnvifQuery,
        uri: &'static str,
        ip: &'static str,
        mac: &'static str,
    ) {
        mock.expect_get_device_ip_and_mac_address()
            .times(1)
            .withf(move |u| u == uri)
            .returning(move |_| Ok((ip.to_string(), mac.to_string())));
    }

    fn configure_get_device_scopes(
        mock: &mut MockOnvifQuery,
        uri: &'static str,
        scope: &'static str,
    ) {
        mock.expect_get_device_scopes()
            .times(1)
            .withf(move |u| u == uri)
            .returning(move |_| Ok(vec![scope.to_string()]));
    }

    #[tokio::test]
    async fn test_apply_filters_no_filters() {
        let mock_uri = "device_uri";

        let mut mock = MockOnvifQuery::new();
        configure_scenario(
            &mut mock,
            Some(IpAndMac {
                mock_uri: "device_uri",
                mock_ip: "mock.ip",
                mock_mac: "mock:mac",
            }),
            Some(Scope {
                mock_uri: "device_uri",
                mock_scope: "mock.scope",
            }),
        );

        let onvif = OnvifDiscoveryHandler::new(&OnvifDiscoveryHandlerConfig {
            ip_addresses: None,
            mac_addresses: None,
            scopes: None,
            discovery_timeout_seconds: 1,
        });
        let instances = onvif
            .apply_filters(vec![mock_uri.to_string()], &mock)
            .await
            .unwrap();

        assert_eq!(1, instances.len());
    }

    #[tokio::test]
    async fn test_apply_filters_include_ip_exist() {
        let mock_uri = "device_uri";
        let mock_ip = "mock.ip";

        let mut mock = MockOnvifQuery::new();
        configure_scenario(
            &mut mock,
            Some(IpAndMac {
                mock_uri,
                mock_ip,
                mock_mac: "mock:mac",
            }),
            Some(Scope {
                mock_uri,
                mock_scope: "mock.scope",
            }),
        );

        let onvif = OnvifDiscoveryHandler::new(&OnvifDiscoveryHandlerConfig {
            ip_addresses: Some(FilterList {
                action: FilterType::Include,
                items: vec![mock_ip.to_string()],
            }),
            mac_addresses: None,
            scopes: None,
            discovery_timeout_seconds: 1,
        });
        let instances = onvif
            .apply_filters(vec![mock_uri.to_string()], &mock)
            .await
            .unwrap();

        assert_eq!(1, instances.len());
    }

    #[tokio::test]
    async fn test_apply_filters_include_ip_nonexist() {
        let mock_uri = "device_uri";

        let mut mock = MockOnvifQuery::new();
        configure_scenario(
            &mut mock,
            Some(IpAndMac {
                mock_uri,
                mock_ip: "mock.ip",
                mock_mac: "mock:mac",
            }),
            None,
        );

        let onvif = OnvifDiscoveryHandler::new(&OnvifDiscoveryHandlerConfig {
            ip_addresses: Some(FilterList {
                action: FilterType::Include,
                items: vec!["nonexist.ip".to_string()],
            }),
            mac_addresses: None,
            scopes: None,
            discovery_timeout_seconds: 1,
        });
        let instances = onvif
            .apply_filters(vec![mock_uri.to_string()], &mock)
            .await
            .unwrap();

        assert_eq!(0, instances.len());
    }

    #[tokio::test]
    async fn test_apply_filters_exclude_ip_nonexist() {
        let mock_uri = "device_uri";

        let mut mock = MockOnvifQuery::new();
        configure_scenario(
            &mut mock,
            Some(IpAndMac {
                mock_uri,
                mock_ip: "mock.ip",
                mock_mac: "mock:mac",
            }),
            Some(Scope {
                mock_uri,
                mock_scope: "mock.scope",
            }),
        );

        let onvif = OnvifDiscoveryHandler::new(&OnvifDiscoveryHandlerConfig {
            ip_addresses: Some(FilterList {
                action: FilterType::Exclude,
                items: vec!["nonexist.ip".to_string()],
            }),
            mac_addresses: None,
            scopes: None,
            discovery_timeout_seconds: 1,
        });
        let instances = onvif
            .apply_filters(vec![mock_uri.to_string()], &mock)
            .await
            .unwrap();

        assert_eq!(1, instances.len());
    }

    #[tokio::test]
    async fn test_apply_filters_exclude_ip_exist() {
        let mock_uri = "device_uri";
        let mock_ip = "mock.ip";

        let mut mock = MockOnvifQuery::new();
        configure_scenario(
            &mut mock,
            Some(IpAndMac {
                mock_uri,
                mock_ip,
                mock_mac: "mock:mac",
            }),
            None,
        );

        let onvif = OnvifDiscoveryHandler::new(&OnvifDiscoveryHandlerConfig {
            ip_addresses: Some(FilterList {
                action: FilterType::Exclude,
                items: vec![mock_ip.to_string()],
            }),
            mac_addresses: None,
            scopes: None,
            discovery_timeout_seconds: 1,
        });
        let instances = onvif
            .apply_filters(vec![mock_uri.to_string()], &mock)
            .await
            .unwrap();

        assert_eq!(0, instances.len());
    }

    #[tokio::test]
    async fn test_apply_filters_include_mac_exist() {
        let mock_uri = "device_uri";
        let mock_mac = "mock:mac";

        let mut mock = MockOnvifQuery::new();
        configure_scenario(
            &mut mock,
            Some(IpAndMac {
                mock_uri,
                mock_ip: "mock.ip",
                mock_mac,
            }),
            Some(Scope {
                mock_uri,
                mock_scope: "mock.scope",
            }),
        );

        let onvif = OnvifDiscoveryHandler::new(&OnvifDiscoveryHandlerConfig {
            ip_addresses: None,
            mac_addresses: Some(FilterList {
                action: FilterType::Include,
                items: vec![mock_mac.to_string()],
            }),
            scopes: None,
            discovery_timeout_seconds: 1,
        });
        let instances = onvif
            .apply_filters(vec![mock_uri.to_string()], &mock)
            .await
            .unwrap();

        assert_eq!(1, instances.len());
    }

    #[tokio::test]
    async fn test_apply_filters_include_mac_nonexist() {
        let mock_uri = "device_uri";

        let mut mock = MockOnvifQuery::new();
        configure_scenario(
            &mut mock,
            Some(IpAndMac {
                mock_uri,
                mock_ip: "mock.ip",
                mock_mac: "mock:mac",
            }),
            None,
        );

        let onvif = OnvifDiscoveryHandler::new(&OnvifDiscoveryHandlerConfig {
            ip_addresses: None,
            mac_addresses: Some(FilterList {
                action: FilterType::Include,
                items: vec!["nonexist:mac".to_string()],
            }),
            scopes: None,
            discovery_timeout_seconds: 1,
        });
        let instances = onvif
            .apply_filters(vec![mock_uri.to_string()], &mock)
            .await
            .unwrap();

        assert_eq!(0, instances.len());
    }

    #[tokio::test]
    async fn test_apply_filters_exclude_mac_nonexist() {
        let mock_uri = "device_uri";

        let mut mock = MockOnvifQuery::new();
        configure_scenario(
            &mut mock,
            Some(IpAndMac {
                mock_uri,
                mock_ip: "mock.ip",
                mock_mac: "mock:mac",
            }),
            Some(Scope {
                mock_uri,
                mock_scope: "mock.scope",
            }),
        );

        let onvif = OnvifDiscoveryHandler::new(&OnvifDiscoveryHandlerConfig {
            ip_addresses: None,
            mac_addresses: Some(FilterList {
                action: FilterType::Exclude,
                items: vec!["nonexist:mac".to_string()],
            }),
            scopes: None,
            discovery_timeout_seconds: 1,
        });
        let instances = onvif
            .apply_filters(vec![mock_uri.to_string()], &mock)
            .await
            .unwrap();

        assert_eq!(1, instances.len());
    }

    #[tokio::test]
    async fn test_apply_filters_exclude_mac_exist() {
        let mock_uri = "device_uri";
        let mock_mac = "mock:mac";

        let mut mock = MockOnvifQuery::new();
        configure_scenario(
            &mut mock,
            Some(IpAndMac {
                mock_uri,
                mock_ip: "mock.ip",
                mock_mac,
            }),
            None,
        );

        let onvif = OnvifDiscoveryHandler::new(&OnvifDiscoveryHandlerConfig {
            ip_addresses: None,
            mac_addresses: Some(FilterList {
                action: FilterType::Exclude,
                items: vec![mock_mac.to_string()],
            }),
            scopes: None,
            discovery_timeout_seconds: 1,
        });
        let instances = onvif
            .apply_filters(vec![mock_uri.to_string()], &mock)
            .await
            .unwrap();

        assert_eq!(0, instances.len());
    }
}
