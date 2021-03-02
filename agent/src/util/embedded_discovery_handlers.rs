use akri_debug_echo::discovery_handler::DebugEchoDiscoveryHandlerConfig;
use akri_discovery_utils::discovery::{v0::discovery_server::Discovery, DiscoverStream};
#[cfg(feature = "onvif-feat")]
use akri_onvif::discovery_handler::OnvifDiscoveryHandlerConfig;
#[cfg(feature = "opcua-feat")]
use akri_opcua::discovery_handler::OpcuaDiscoveryHandlerConfig;
use akri_shared::{
    akri::configuration::ProtocolHandler,
    os::env_var::{ActualEnvVarQuery, EnvVarQuery},
};
#[cfg(feature = "udev-feat")]
use akri_udev::discovery_handler::UdevDiscoveryHandlerConfig;
use anyhow::Error;
use log::trace;

/// Returns the appropriate embedded Discovery Handler as determined by the deserialized contents
/// of the value of the discovery_details map at key "protocolHandler".
pub fn get_discovery_handler(
    protocol_handler: &ProtocolHandler,
) -> Result<Box<dyn Discovery<DiscoverStream = DiscoverStream>>, Error> {
    let query_var_set = ActualEnvVarQuery {};
    inner_get_discovery_handler(protocol_handler, &query_var_set)
}

fn inner_get_discovery_handler(
    protocol_handler: &ProtocolHandler,
    query: &impl EnvVarQuery,
) -> Result<Box<dyn Discovery<DiscoverStream = DiscoverStream>>, Error> {
    trace!(
        "inner_get_discovery_handler - for ProtocolHandler {:?}",
        protocol_handler
    );
    // Determine whether it is an embedded protocol
    if let Some(discovery_handler_str) = protocol_handler.discovery_details.get("protocolHandler") {
        match protocol_handler.name.as_str() {
            #[cfg(feature = "onvif-feat")]
            akri_onvif::PROTOCOL_NAME => {
                let _discovery_handler_config: OnvifDiscoveryHandlerConfig = serde_yaml::from_str(discovery_handler_str).map_err(|e| anyhow::format_err!("ONVIF Configuration discovery details improperly configured with error {:?}", e))?;
                Ok(Box::new(
                    akri_onvif::discovery_handler::DiscoveryHandler::new(None),
                ))
            }
            #[cfg(feature = "udev-feat")]
            akri_udev::PROTOCOL_NAME => {
                let _discovery_handler_config: UdevDiscoveryHandlerConfig = serde_yaml::from_str(discovery_handler_str).map_err(|e| anyhow::format_err!("udev Configuration discovery details improperly configured with error {:?}", e))?;
                Ok(Box::new(
                    akri_udev::discovery_handler::DiscoveryHandler::new(None),
                ))
            }
            #[cfg(feature = "opcua-feat")]
            akri_opcua::PROTOCOL_NAME => {
                let _discovery_handler_config: OpcuaDiscoveryHandlerConfig = serde_yaml::from_str(discovery_handler_str).map_err(|e| anyhow::format_err!("OPC UA Configuration discovery details improperly configured with error {:?}", e))?;
                Ok(Box::new(
                    akri_opcua::discovery_handler::DiscoveryHandler::new(None),
                ))
            }
            akri_debug_echo::PROTOCOL_NAME => {
                let _discovery_handler_config: DebugEchoDiscoveryHandlerConfig = serde_yaml::from_str(discovery_handler_str).map_err(|e| anyhow::format_err!("debug echo Configuration discovery details improperly configured with error {:?}", e))?;
                match query.get_env_var("ENABLE_DEBUG_ECHO") {
                    Ok(_) => Ok(Box::new(
                        akri_debug_echo::discovery_handler::DiscoveryHandler::new(None),
                    )),
                    _ => Err(anyhow::format_err!("Debug echo protocol not configured")),
                }
            }
            // If the feature-gated protocol handlers are not included, this catch-all
            // should surface any invalid Configuration requests (i.e. udev-feat not
            // included at build-time ... but at runtime, a udev Configuration is
            // applied).  For the default build, where all features are included, this
            // code triggers an unreachable pattern warning.  #[allow] is added to
            // explicitly hide this warning.
            #[allow(unreachable_patterns)]
            _ => Err(anyhow::format_err!(
                "No embedded discovery handler found for configuration with protocol handler {:?}",
                protocol_handler
            )),
        }
    } else {
        Err(anyhow::format_err!(
            "No embedded discovery handler configuration found in discovery details map with key 'protocolHandler' for ProtocolHandler {:?}",
            protocol_handler
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use akri_shared::{akri::configuration::ProtocolHandler, os::env_var::MockEnvVarQuery};
    use std::env::VarError;

    #[test]
    fn test_inner_get_discovery_handler() {
        let _ = env_logger::builder().is_test(true).try_init();
        let mock_query = MockEnvVarQuery::new();
        let deserialized = serde_json::from_str::<ProtocolHandler>(
            r#"{"name":"onvif", "discoveryDetails":{"protocolHandler":"{}"}}"#,
        )
        .unwrap();
        assert!(inner_get_discovery_handler(&deserialized, &mock_query).is_ok());

        let udev_yaml = r#"
        name: udev
        discoveryDetails:
          protocolHandler: |+
            udevRules: []
        "#;
        let deserialized: ProtocolHandler = serde_yaml::from_str(&udev_yaml).unwrap();
        assert!(inner_get_discovery_handler(&deserialized, &mock_query).is_ok());

        let yaml = r#"
        name: opcua
        discoveryDetails:
          protocolHandler: |+
            opcuaDiscoveryMethod: 
              standard: {}
        "#;
        let deserialized: ProtocolHandler = serde_yaml::from_str(&yaml).unwrap();
        assert!(inner_get_discovery_handler(&deserialized, &mock_query).is_ok());

        let deserialized = serde_json::from_str::<ProtocolHandler>(
            r#"{"name":"random", "discoveryDetails":{"key":"random protocol"}}"#,
        )
        .unwrap();
        assert!(inner_get_discovery_handler(&deserialized, &mock_query).is_err());

        let deserialized = serde_json::from_str::<ProtocolHandler>(
            r#"{"name":"random", "discoveryDetails":{"protocolHandler":"random protocol"}}"#,
        )
        .unwrap();
        assert!(inner_get_discovery_handler(&deserialized, &mock_query).is_err());
    }

    #[tokio::test]
    async fn test_factory_for_debug_echo() {
        let debug_echo_yaml = r#"
        protocol: 
        name: debugEcho
        discoveryDetails:
          protocolHandler: |+
            descriptions:
            - "foo1"
        "#;
        let deserialized: ProtocolHandler = serde_yaml::from_str(&debug_echo_yaml).unwrap();
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
