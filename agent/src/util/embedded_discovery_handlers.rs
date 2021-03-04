use akri_debug_echo::discovery_handler::DebugEchoDiscoveryHandlerConfig;
use akri_discovery_utils::discovery::{
    v0::discovery_handler_server::DiscoveryHandler, DiscoverStream,
};
#[cfg(feature = "onvif-feat")]
use akri_onvif::discovery_handler::OnvifDiscoveryHandlerConfig;
#[cfg(feature = "opcua-feat")]
use akri_opcua::discovery_handler::OpcuaDiscoveryHandlerConfig;
use akri_shared::{
    akri::configuration::DiscoveryHandlerInfo,
    os::env_var::{ActualEnvVarQuery, EnvVarQuery},
};
#[cfg(feature = "udev-feat")]
use akri_udev::discovery_handler::UdevDiscoveryHandlerConfig;
use anyhow::Error;
use log::trace;

/// Returns the appropriate embedded DiscoveryHandler as determined by the deserialized contents
/// of the value of the discovery_details map at key "discoveryHandlerConfig".
pub fn get_discovery_handler(
    discovery_handler_info: &DiscoveryHandlerInfo,
) -> Result<Box<dyn DiscoveryHandler<DiscoverStream = DiscoverStream>>, Error> {
    let query_var_set = ActualEnvVarQuery {};
    inner_get_discovery_handler(discovery_handler_info, &query_var_set)
}

fn inner_get_discovery_handler(
    discovery_handler_info: &DiscoveryHandlerInfo,
    query: &impl EnvVarQuery,
) -> Result<Box<dyn DiscoveryHandler<DiscoverStream = DiscoverStream>>, Error> {
    trace!(
        "inner_get_discovery_handler - for DiscoveryHandlerInfo {:?}",
        discovery_handler_info
    );
    // Determine whether it is an embedded discovery handler
    if let Some(discovery_handler_str) = discovery_handler_info
        .discovery_details
        .get("discoveryHandlerConfig")
    {
        match discovery_handler_info.name.as_str() {
            #[cfg(feature = "onvif-feat")]
            akri_onvif::DISCOVERY_HANDLER_NAME => {
                let _discovery_handler_config: OnvifDiscoveryHandlerConfig = serde_yaml::from_str(discovery_handler_str).map_err(|e| anyhow::format_err!("ONVIF Configuration discovery details improperly configured with error {:?}", e))?;
                Ok(Box::new(
                    akri_onvif::discovery_handler::DiscoveryHandlerImpl::new(None),
                ))
            }
            #[cfg(feature = "udev-feat")]
            akri_udev::DISCOVERY_HANDLER_NAME => {
                let _discovery_handler_config: UdevDiscoveryHandlerConfig = serde_yaml::from_str(discovery_handler_str).map_err(|e| anyhow::format_err!("udev Configuration discovery details improperly configured with error {:?}", e))?;
                Ok(Box::new(
                    akri_udev::discovery_handler::DiscoveryHandlerImpl::new(None),
                ))
            }
            #[cfg(feature = "opcua-feat")]
            akri_opcua::DISCOVERY_HANDLER_NAME => {
                let _discovery_handler_config: OpcuaDiscoveryHandlerConfig = serde_yaml::from_str(discovery_handler_str).map_err(|e| anyhow::format_err!("OPC UA Configuration discovery details improperly configured with error {:?}", e))?;
                Ok(Box::new(
                    akri_opcua::discovery_handler::DiscoveryHandlerImpl::new(None),
                ))
            }
            akri_debug_echo::DISCOVERY_HANDLER_NAME => {
                match query.get_env_var(super::constants::ENABLE_DEBUG_ECHO_LABEL) {
                    Ok(_) => {
                        let _discovery_handler_config: DebugEchoDiscoveryHandlerConfig = serde_yaml::from_str(discovery_handler_str).map_err(|e| anyhow::format_err!("debug echo Configuration discovery details improperly configured with error {:?}", e))?;
                        Ok(Box::new(
                        akri_debug_echo::discovery_handler::DiscoveryHandlerImpl::new(None)))
                    },
                    _ => Err(anyhow::format_err!("Debug echo discovery handler not configured")),
                }
            }
            // If the feature-gated discovery handlers are not included, this catch-all
            // should surface any invalid Configuration requests (i.e. udev-feat not
            // included at build-time ... but at runtime, a udev Configuration is
            // applied).  For the default build, where all features are included, this
            // code triggers an unreachable pattern warning.  #[allow] is added to
            // explicitly hide this warning.
            #[allow(unreachable_patterns)]
            _ => Err(anyhow::format_err!(
                "No embedded discovery handler found for configuration with discovery handler info {:?}",
                discovery_handler_info
            )),
        }
    } else {
        Err(anyhow::format_err!(
            "No embedded discovery handler configuration found in discovery details map with key 'discoveryHandlerConfig' for DiscoveryHandlerInfo {:?}",
            discovery_handler_info
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use akri_shared::{akri::configuration::DiscoveryHandlerInfo, os::env_var::MockEnvVarQuery};
    use std::env::VarError;

    #[test]
    fn test_inner_get_discovery_handler() {
        let _ = env_logger::builder().is_test(true).try_init();
        let mock_query = MockEnvVarQuery::new();
        let deserialized = serde_json::from_str::<DiscoveryHandlerInfo>(
            r#"{"name":"onvif", "discoveryDetails":{"discoveryHandlerConfig":"{}"}}"#,
        )
        .unwrap();
        assert!(inner_get_discovery_handler(&deserialized, &mock_query).is_ok());

        let udev_yaml = r#"
        name: udev
        discoveryDetails:
          discoveryHandlerConfig: |+
            udevRules: []
        "#;
        let deserialized: DiscoveryHandlerInfo = serde_yaml::from_str(&udev_yaml).unwrap();
        assert!(inner_get_discovery_handler(&deserialized, &mock_query).is_ok());

        let yaml = r#"
        name: opcua
        discoveryDetails:
          discoveryHandlerConfig: |+
            opcuaDiscoveryMethod: 
              standard: {}
        "#;
        let deserialized: DiscoveryHandlerInfo = serde_yaml::from_str(&yaml).unwrap();
        assert!(inner_get_discovery_handler(&deserialized, &mock_query).is_ok());

        let deserialized = serde_json::from_str::<DiscoveryHandlerInfo>(
            r#"{"name":"random", "discoveryDetails":{"key":"random protocol"}}"#,
        )
        .unwrap();
        assert!(inner_get_discovery_handler(&deserialized, &mock_query).is_err());

        let deserialized = serde_json::from_str::<DiscoveryHandlerInfo>(
            r#"{"name":"random", "discoveryDetails":{"discoveryHandlerConfig":"random protocol"}}"#,
        )
        .unwrap();
        assert!(inner_get_discovery_handler(&deserialized, &mock_query).is_err());
    }

    #[tokio::test]
    async fn test_factory_for_debug_echo() {
        let debug_echo_yaml = r#"
        discoveryHandler: 
        name: debugEcho
        discoveryDetails:
          discoveryHandlerConfig: |+
            descriptions:
            - "foo1"
        "#;
        let deserialized: DiscoveryHandlerInfo = serde_yaml::from_str(&debug_echo_yaml).unwrap();
        // Test that errors without environment var set
        let mut mock_query_without_var_set = MockEnvVarQuery::new();
        mock_query_without_var_set
            .expect_get_env_var()
            .returning(|_| Err(VarError::NotPresent));
        assert!(inner_get_discovery_handler(&deserialized, &mock_query_without_var_set,).is_err());
        // Test that succeeds when env var set
        let mut mock_query_with_var_set = MockEnvVarQuery::new();
        mock_query_with_var_set
            .expect_get_env_var()
            .returning(|_| Ok("1".to_string()));
        assert!(inner_get_discovery_handler(&deserialized, &mock_query_with_var_set).is_ok());
    }
}
