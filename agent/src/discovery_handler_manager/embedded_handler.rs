use std::sync::Arc;

use akri_discovery_utils::discovery::{
    DiscoverStream,
    v0::{DiscoverRequest, DiscoverResponse, discovery_handler_server::DiscoveryHandler},
};
use akri_shared::os::env_var::{ActualEnvVarQuery, EnvVarQuery};
use async_trait::async_trait;
use tokio::{select, sync::watch};
use tokio_stream::{StreamExt, wrappers::ReceiverStream};
use tonic::IntoRequest;

/// Label of environment variable that, when set, enables the embedded debug echo discovery handler
#[cfg(any(test, feature = "agent-full"))]
pub const ENABLE_DEBUG_ECHO_LABEL: &str = "ENABLE_DEBUG_ECHO";

use super::{
    DiscoveryError,
    discovery_handler_registry::{
        DiscoveredDevice, DiscoveryHandlerEndpoint, DiscoveryHandlerRegistry,
    },
};

struct EmbeddedHandlerEndpoint {
    name: String,
    shared: bool,
    handler: Box<dyn DiscoveryHandler<DiscoverStream = DiscoverStream>>,
    node_name: String,
}

impl EmbeddedHandlerEndpoint {
    async fn handle_stream(
        uid: String,
        node_name: String,
        shared: bool,
        sender: watch::Sender<Vec<Arc<DiscoveredDevice>>>,
        mut stream: ReceiverStream<Result<DiscoverResponse, tonic::Status>>,
    ) {
        loop {
            let msg = select! {
                _ = sender.closed() => return,
                msg = stream.try_next() =>  match msg {
                    Ok(Some(msg)) => msg,
                    Ok(None) => {
                        error!("Discovery Handler {} closed the stream unexpectedly", uid);
                        return
                    },
                    Err(e) => {
                        error!("Received error on gRPC stream for {}: {}", uid, e);
                        return
                    },
                },
            };
            let devices = msg
                .devices
                .into_iter()
                .map(|d| {
                    Arc::new(match shared {
                        true => DiscoveredDevice::SharedDevice(d),
                        false => DiscoveredDevice::LocalDevice(d, node_name.clone()),
                    })
                })
                .collect();
            sender.send_replace(devices);
        }
    }
}

#[async_trait]
impl DiscoveryHandlerEndpoint for EmbeddedHandlerEndpoint {
    async fn query(
        &self,
        sender: watch::Sender<Vec<Arc<DiscoveredDevice>>>,
        query_body: DiscoverRequest,
    ) -> Result<(), DiscoveryError> {
        let stream = match self.handler.discover(query_body.into_request()).await {
            Ok(r) => r.into_inner(),
            Err(e) => {
                match e.code() {
                    tonic::Code::InvalidArgument => {
                        warn!(
                            "NetworkEndpoint::query - invalid arguments provided to DiscoveryHandler"
                        );
                        return Err(DiscoveryError::InvalidDiscoveryDetails);
                    }
                    _ => {
                        error!(
                            "NetworkEndpoint::query - could not connect to DiscoveryHandler at endpoint {} with error {}",
                            self.get_uid(),
                            e
                        );
                        // We do not consider the DH as unavailable here, as this can be a temporary error
                        return Err(DiscoveryError::UnavailableDiscoveryHandler(self.get_uid()));
                    }
                }
            }
        };
        tokio::spawn(Self::handle_stream(
            self.get_uid(),
            self.node_name.to_owned(),
            self.shared.to_owned(),
            sender,
            stream,
        ));
        Ok(())
    }

    fn get_name(&self) -> String {
        self.name.to_owned()
    }
    fn get_uid(&self) -> String {
        format!("embedded-{}", self.name)
    }

    async fn closed(&self) {
        std::future::pending().await
    }
    fn is_closed(&self) -> bool {
        false
    }
}

pub(super) async fn register_handlers(reg: &dyn DiscoveryHandlerRegistry, node_name: String) {
    let env_var_query = ActualEnvVarQuery {};
    inner_register_discovery_handlers(reg, &env_var_query, node_name).await;
}

async fn inner_register_discovery_handlers(
    reg: &dyn DiscoveryHandlerRegistry,
    env: &dyn EnvVarQuery,
    node_name: String,
) {
    if env.get_env_var(ENABLE_DEBUG_ECHO_LABEL).is_ok() {
        let shared: bool = env
            .get_env_var(akri_debug_echo::DEBUG_ECHO_INSTANCES_SHARED_LABEL)
            .unwrap()
            .parse()
            .unwrap();
        reg.register_endpoint(Arc::new(EmbeddedHandlerEndpoint {
            name: akri_debug_echo::DISCOVERY_HANDLER_NAME.to_string(),
            shared,
            handler: Box::new(akri_debug_echo::discovery_handler::DiscoveryHandlerImpl::new(None)),
            node_name: node_name.clone(),
        }))
        .await;
    }
    #[cfg(feature = "onvif-feat")]
    reg.register_endpoint(Arc::new(EmbeddedHandlerEndpoint {
        name: akri_onvif::DISCOVERY_HANDLER_NAME.to_string(),
        shared: akri_onvif::SHARED,
        handler: Box::new(akri_onvif::discovery_handler::DiscoveryHandlerImpl::new(
            None,
        )),
        node_name: node_name.clone(),
    }))
    .await;
    #[cfg(feature = "udev-feat")]
    reg.register_endpoint(Arc::new(EmbeddedHandlerEndpoint {
        name: akri_udev::DISCOVERY_HANDLER_NAME.to_string(),
        shared: akri_udev::SHARED,
        handler: Box::new(akri_udev::discovery_handler::DiscoveryHandlerImpl::new(
            None,
        )),
        node_name: node_name.clone(),
    }))
    .await;
    #[cfg(feature = "opcua-feat")]
    reg.register_endpoint(Arc::new(EmbeddedHandlerEndpoint {
        name: akri_opcua::DISCOVERY_HANDLER_NAME.to_string(),
        shared: akri_opcua::SHARED,
        handler: Box::new(akri_opcua::discovery_handler::DiscoveryHandlerImpl::new(
            None,
        )),
        node_name: node_name.clone(),
    }))
    .await;
}
