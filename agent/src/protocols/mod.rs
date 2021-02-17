use akri_discovery_utils::discovery::{v0::discovery_server::Discovery, DiscoverStream};
use akri_shared::os::env_var::{ActualEnvVarQuery, EnvVarQuery};
use anyhow::Error;
use log::trace;
use std::collections::HashMap;
// TODO: decide where to put discover stream
use akri_debug_echo::discovery_handler::DebugEchoDiscoveryHandlerConfig;
#[cfg(feature = "onvif-feat")]
use akri_onvif::discovery_handler::OnvifDiscoveryHandlerConfig;
#[cfg(feature = "opcua-feat")]
use akri_opcua::discovery_handler::OpcuaDiscoveryHandlerConfig;
#[cfg(feature = "udev-feat")]
use akri_udev::discovery_handler::UdevDiscoveryHandlerConfig;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub enum DiscoveryHandlerType {
    #[cfg(feature = "onvif-feat")]
    Onvif(OnvifDiscoveryHandlerConfig),
    #[cfg(feature = "udev-feat")]
    Udev(UdevDiscoveryHandlerConfig),
    #[cfg(feature = "opcua-feat")]
    Opcua(OpcuaDiscoveryHandlerConfig),
    DebugEcho(DebugEchoDiscoveryHandlerConfig),
}
pub fn get_discovery_handler(
    discovery_details: &HashMap<String, String>,
) -> Result<Box<dyn Discovery<DiscoverStream = DiscoverStream>>, Error> {
    let query_var_set = ActualEnvVarQuery {};
    inner_get_discovery_handler(discovery_details, &query_var_set)
}

fn inner_get_discovery_handler(
    discovery_details: &HashMap<String, String>,
    query: &impl EnvVarQuery,
) -> Result<Box<dyn Discovery<DiscoverStream = DiscoverStream>>, Error> {
    trace!(
        "inner_get_discovery_handler - for discovery details {:?}",
        discovery_details
    );
    // Determine whether it is an embedded protocol
    if let Some(discovery_handler_str) = discovery_details.get("protocolHandler") {
        trace!(
            "inner_get_discovery_handler - protocol handler: {:?}",
            discovery_handler_str
        );
        if let Ok(discovery_handler) = serde_yaml::from_str(discovery_handler_str) {
            match discovery_handler {
                DiscoveryHandlerType::DebugEcho(_) => {
                    match query.get_env_var("ENABLE_DEBUG_ECHO") {
                        Ok(_) => Ok(Box::new(
                            akri_debug_echo::discovery_handler::DiscoveryHandler::new(None),
                        )),
                        _ => Err(anyhow::format_err!("Debug echo protocol not configured")),
                    }
                }
                #[cfg(feature = "onvif-feat")]
                DiscoveryHandlerType::Onvif(_) => Ok(Box::new(
                    akri_onvif::discovery_handler::DiscoveryHandler::new(None),
                )),
                #[cfg(feature = "udev-feat")]
                DiscoveryHandlerType::Udev(_) => Ok(Box::new(
                    akri_udev::discovery_handler::DiscoveryHandler::new(None),
                )),
                #[cfg(feature = "opcua-feat")]
                DiscoveryHandlerType::Opcua(_) => Ok(Box::new(
                    akri_opcua::discovery_handler::DiscoveryHandler::new(None),
                )),
                // If the feature-gated protocol handlers are not included, this catch-all
                // should surface any invalid Configuration requests (i.e. udev-feat not
                // included at build-time ... but at runtime, a udev Configuration is
                // applied).  For the default build, where all features are included, this
                // code triggers an unreachable pattern warning.  #[allow] is added to
                // explicitly hide this warning.
                #[allow(unreachable_patterns)]
                config => Err(anyhow::format_err!(
                    "No handler found for configuration {:?}",
                    config
                )),
            }
        } else {
            error!("err1");
            Err(anyhow::format_err!("Discovery details had protocol handler but does not have embedded support. Discovery details: {:?}", discovery_details))
        }
    } else {
        error!("err2");
        Err(anyhow::format_err!(
            "Generic discovery handlers not supported. Discovery details: {:?}",
            discovery_details
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use akri_discovery_utils::discovery::v0::DiscoverRequest;
    use akri_shared::{akri::configuration::ProtocolHandler, os::env_var::MockEnvVarQuery};
    use std::env::VarError;

    #[test]
    fn test_inner_get_discovery_handler() {
        let _ = env_logger::builder().is_test(true).try_init();
        let mock_query = MockEnvVarQuery::new();
        let deserialized = serde_json::from_str::<ProtocolHandler>(
            r#"{"name":"onvif", "discoveryDetails":{"protocolHandler":"{\"onvif\":{}}"}}"#,
        )
        .unwrap();
        assert!(inner_get_discovery_handler(&deserialized.discovery_details, &mock_query).is_ok());

        let udev_yaml = r#"
        name: udev
        discoveryDetails:
          protocolHandler: |+
            udev:
              udevRules: []
        "#;
        let deserialized: ProtocolHandler = serde_yaml::from_str(&udev_yaml).unwrap();
        assert!(inner_get_discovery_handler(&deserialized.discovery_details, &mock_query).is_ok());

        let yaml = r#"
        name: opcua
        discoveryDetails:
          protocolHandler: |+
            opcua:
              opcuaDiscoveryMethod: 
                standard: {}
        "#;
        let deserialized: ProtocolHandler = serde_yaml::from_str(&yaml).unwrap();
        assert!(inner_get_discovery_handler(&deserialized.discovery_details, &mock_query).is_ok());

        let deserialized = serde_json::from_str::<ProtocolHandler>(
            r#"{"name":"random", "discoveryDetails":{"key":"random protocol"}}"#,
        )
        .unwrap();
        assert!(inner_get_discovery_handler(&deserialized.discovery_details, &mock_query).is_err());

        let deserialized = serde_json::from_str::<ProtocolHandler>(
            r#"{"name":"random", "discoveryDetails":{"protocolHandler":"random protocol"}}"#,
        )
        .unwrap();
        assert!(inner_get_discovery_handler(&deserialized.discovery_details, &mock_query).is_err());
    }

    #[tokio::test]
    async fn test_factory_for_debug_echo_when_no_env_var_set() {
        let debug_echo_yaml = r#"
        protocol: 
        name: debugEcho
        discoveryDetails:
          protocolHandler: |+
            debugEcho:
              descriptions:
              - "foo1"
        "#;
        let deserialized: ProtocolHandler = serde_yaml::from_str(&debug_echo_yaml).unwrap();

        let mut mock_query_without_var_set = MockEnvVarQuery::new();
        mock_query_without_var_set
            .expect_get_env_var()
            .returning(|_| Err(VarError::NotPresent));
        if inner_get_discovery_handler(
            &deserialized.discovery_details.clone(),
            &mock_query_without_var_set,
        )
        .is_ok()
        {
            panic!("protocol configuration as debugEcho should return error when 'ENABLE_DEBUG_ECHO' env var is not set")
        }

        let mut mock_query_with_var_set = MockEnvVarQuery::new();
        mock_query_with_var_set
            .expect_get_env_var()
            .returning(|_| Ok("1".to_string()));
        let device = akri_discovery_utils::discovery::v0::Device {
            id: "foo1".to_string(),
            properties: HashMap::new(),
            mounts: Vec::default(),
            device_specs: Vec::default(),
        };
        let discovery_handler =
            inner_get_discovery_handler(&deserialized.discovery_details, &mock_query_with_var_set)
                .unwrap();
        let discover_request = tonic::Request::new(DiscoverRequest {
            discovery_details: deserialized.discovery_details.clone(),
        });
        let devices = discovery_handler
            .discover(discover_request)
            .await
            .unwrap()
            .into_inner()
            .recv()
            .await
            .unwrap()
            .unwrap()
            .devices;
        assert_eq!(1, devices.len());
        assert_eq!(devices[0], device);
    }
}
