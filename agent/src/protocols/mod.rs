use akri_shared::{
    akri::configuration::ProtocolHandler,
    os::env_var::{ActualEnvVarQuery, EnvVarQuery},
};
use async_trait::async_trait;
use blake2::digest::{Input, VariableOutput};
use blake2::VarBlake2b;
use failure::Error;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct DiscoveryResult {
    pub digest: String,
    pub properties: HashMap<String, String>,
}
impl DiscoveryResult {
    fn new(id_to_digest: &str, properties: HashMap<String, String>, shared: bool) -> Self {
        let mut id_to_digest = id_to_digest.to_string();
        // For unshared devices, include node hostname in id_to_digest so instances have unique names
        if !shared {
            id_to_digest = format!(
                "{}{}",
                &id_to_digest,
                std::env::var("AGENT_NODE_NAME").unwrap()
            );
        }
        let mut hasher = VarBlake2b::new(3).unwrap();
        hasher.input(id_to_digest);
        let digest = hasher
            .vec_result()
            .iter()
            .map(|num| format!("{:02x}", num))
            .collect::<Vec<String>>()
            .join("");
        DiscoveryResult { digest, properties }
    }
}

/// DiscoveryHandler describes anything that can find available instances and define
/// whether they are shared.
///
/// DiscoveryHandler provides an abstraction to help in Instance
/// creation: search/find for instances, specify whether the instance
/// should be shared, etc.
///
/// # Examples
///
/// ```
/// pub struct SampleDiscoveryHandler {}
/// #[async_trait]
/// impl DiscoveryHandler for SampleDiscoveryHandler {
///     async fn discover(&self) -> Result<Vec<DiscoveryResult>, failure::Error> {
///         Ok(Vec::new())
///     }
///     fn are_shared(&self) -> Result<bool, Error> {
///         Ok(true)
///     }
/// }
/// ```
#[async_trait]
pub trait DiscoveryHandler {
    async fn discover(&self) -> Result<Vec<DiscoveryResult>, Error>;
    fn are_shared(&self) -> Result<bool, Error>;
}

pub mod debug_echo;
mod nessie;
mod onvif;
mod opcua;
mod udev;

pub fn get_discovery_handler(
    discovery_handler_config: &ProtocolHandler,
) -> Result<Box<dyn DiscoveryHandler + Sync + Send>, Error> {
    let query_var_set = ActualEnvVarQuery {};
    inner_get_discovery_handler(discovery_handler_config, &query_var_set)
}

fn inner_get_discovery_handler(
    discovery_handler_config: &ProtocolHandler,
    query: &impl EnvVarQuery,
) -> Result<Box<dyn DiscoveryHandler + Sync + Send>, Error> {
    match discovery_handler_config {
        ProtocolHandler::nessie(nessie) => {
            Ok(Box::new(nessie::NessieDiscoveryHandler::new(&nessie)))
        }
        ProtocolHandler::onvif(onvif) => Ok(Box::new(onvif::OnvifDiscoveryHandler::new(&onvif))),
        ProtocolHandler::udev(udev) => Ok(Box::new(udev::UdevDiscoveryHandler::new(&udev))),
        ProtocolHandler::opcua(opcua) => Ok(Box::new(opcua::OpcuaDiscoveryHandler::new(&opcua))),
        ProtocolHandler::debugEcho(dbg) => match query.get_env_var("ENABLE_DEBUG_ECHO") {
            Ok(_) => Ok(Box::new(debug_echo::DebugEchoDiscoveryHandler::new(dbg))),
            _ => Err(failure::format_err!("No protocol configured")),
        },
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use akri_shared::{
        akri::configuration::{Configuration, ProtocolHandler},
        os::env_var::MockEnvVarQuery,
    };
    use std::env::VarError;

    #[tokio::test]
    async fn test_inner_get_discovery_handler() {
        let mock_query = MockEnvVarQuery::new();

        let onvif_json = r#"{"onvif":{}}"#;
        let deserialized: ProtocolHandler = serde_json::from_str(onvif_json).unwrap();
        assert!(inner_get_discovery_handler(&deserialized, &mock_query).is_ok());

        let udev_json = r#"{"udev":{"udevRules":[]}}"#;
        let deserialized: ProtocolHandler = serde_json::from_str(udev_json).unwrap();
        assert!(inner_get_discovery_handler(&deserialized, &mock_query).is_ok());

        let opcua_json = r#"{"opcua":{}}"#;
        let deserialized: ProtocolHandler = serde_json::from_str(opcua_json).unwrap();
        let discovery_handler = inner_get_discovery_handler(&deserialized, &mock_query).unwrap();
        assert!(
            discovery_handler.discover().await.is_err(),
            "OPC protocol handler not implemented"
        );

        let json = r#"{}"#;
        assert!(serde_json::from_str::<Configuration>(json).is_err());
    }

    #[tokio::test]
    async fn test_udev_discover_no_rules() {
        let mock_query = MockEnvVarQuery::new();

        let json = r#"{"udev":{"udevRules":[]}}"#;
        let deserialized: ProtocolHandler = serde_json::from_str(json).unwrap();
        let discovery_handler = inner_get_discovery_handler(&deserialized, &mock_query).unwrap();
        assert_eq!(discovery_handler.discover().await.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_factory_for_debug_echo_when_no_env_var_set() {
        let json = r#"{"protocol":{"debugEcho":{"descriptions":["foo1"],"shared":true}}}"#;
        let deserialized: Configuration = serde_json::from_str(json).unwrap();

        let mut mock_query_without_var_set = MockEnvVarQuery::new();
        mock_query_without_var_set
            .expect_get_env_var()
            .returning(|_| Err(VarError::NotPresent));
        if inner_get_discovery_handler(&deserialized.protocol, &mock_query_without_var_set).is_ok()
        {
            panic!("protocol configuration as debugEcho should return error when 'ENABLE_DEBUG_ECHO' env var is not set")
        }

        let mut mock_query_with_var_set = MockEnvVarQuery::new();
        mock_query_with_var_set
            .expect_get_env_var()
            .returning(|_| Ok("1".to_string()));
        let pi = DiscoveryResult::new(&"foo1".to_string(), HashMap::new(), true);
        let debug_echo_discovery_handler =
            inner_get_discovery_handler(&deserialized.protocol, &mock_query_with_var_set).unwrap();
        assert_eq!(true, debug_echo_discovery_handler.are_shared().unwrap());
        assert_eq!(
            1,
            debug_echo_discovery_handler.discover().await.unwrap().len()
        );
        assert_eq!(
            pi.digest,
            debug_echo_discovery_handler
                .discover()
                .await
                .unwrap()
                .get(0)
                .unwrap()
                .digest
        );
    }
}
