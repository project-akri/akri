use akri_discovery_utils::discovery::v0::{DiscoverResponse, DiscoverRequest, discovery_server::Discovery};
use async_trait::async_trait;
use anyhow::Error;
use log::trace;
use std::collections::HashMap;
// TODO: decide where to put discover stream
use akri_debug_echo::discovery_handler::{DebugEchoDiscoveryHandler, DebugEchoDiscoveryHandlerConfig, DiscoverStream};
#[cfg(feature = "onvif-feat")]
use akri_onvif::discovery_handler::{OnvifDiscoveryHandler, OnvifDiscoveryHandlerConfig};
use tonic::{Response, Status};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolHandler2 {
    pub name: String,
    #[serde(default)]
    pub discovery_details: HashMap<String, String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub enum DiscoveryHandlerType {
    #[cfg(feature = "onvif-feat")]
    Onvif(OnvifDiscoveryHandler),
    // udev(UdevDiscoveryHandler),
    // opcua(OpcuaDiscoveryHandler),
    DebugEcho(DebugEchoDiscoveryHandlerConfig),
}
pub fn get_discovery_handler(
    discovery_details: &HashMap<String, String>,
) -> Result<Box<dyn Discovery<DiscoverStream = DiscoverStream>>, Error> {
    // let query_var_set = ActualEnvVarQuery {};
    inner_get_discovery_handler(discovery_details)
}

fn inner_get_discovery_handler(
    discovery_details: &HashMap<String, String>,
)  -> Result<Box<dyn Discovery<DiscoverStream = DiscoverStream>>, Error>{
    trace!("inner_get_discovery_handler - for discovery details {:?}", discovery_details);
    // Determine whether it is an embedded protocol
    if let Some(discovery_handler_str) = discovery_details.get("protocolHandler") {
        trace!("inner_get_discovery_handler - protocol handler: {:?}",discovery_handler_str);
        if let Ok(discovery_handler) = serde_yaml::from_str(discovery_handler_str) {
            match discovery_handler {
                DiscoveryHandlerType::DebugEcho(_) => Ok(Box::new(DebugEchoDiscoveryHandler::new())),
                #[cfg(feature = "onvif-feat")]
                DiscoveryHandlerType::Onvif(_) => Ok(Box::new(OnvifDiscoveryHandler::new())),
                // #[cfg(feature = "udev-feat")]
                // ProtocolHandler::udev(udev) => Ok(Box::new(udev::UdevDiscoveryHandler::new(&udev))),
                // #[cfg(feature = "opcua-feat")]
                // ProtocolHandler::opcua(opcua) => Ok(Box::new(opcua::OpcuaDiscoveryHandler::new(&opcua))),
                // ProtocolHandler::debugEcho(dbg) => match query.get_env_var("ENABLE_DEBUG_ECHO") {
                //     Ok(_) => Ok(Box::new(debug_echo::DebugEchoDiscoveryHandler::new(&dbg))),
                //     _ => Err(anyhow::format_err!("No protocol configured")),
                // },
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
        Err(anyhow::format_err!("Generic discovery handlers not supported. Discovery details: {:?}", discovery_details))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_inner_get_discovery_handler() {
        let debug_echo_yaml = r#"
        protocol: 
        name: debug echo
        discoveryDetails:
          protocolHandler: |+
            debugEcho:
              descriptions:
              - "foo0"
              - "foo1"
              shared: false
        "#;
        let protocol_handler: ProtocolHandler2 = serde_yaml::from_str(&debug_echo_yaml).unwrap();
        let discovery_details = protocol_handler.discovery_details;
        assert!(inner_get_discovery_handler(&discovery_details).is_ok());
    }

}