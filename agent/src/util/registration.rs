use akri_discovery_utils::discovery::v0::{
    registration_server::{Registration, RegistrationServer},
    Empty, RegisterRequest,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::broadcast;
use tonic::{transport::Server, Request, Response, Status};
const REGISTRATION_ENDPOINT: &str = "[::1]:10000";
pub const DH_OFFLINE_GRACE_PERIOD: u64 = 300;
pub const EMBEDDED_DISCOVERY_HANDLER_ENDPOINT: &str = "embedded";

// Map of RegisterRequests for a specific protocol where key is the endpoint of the Discovery Handler
// and value is whether or not the discovered devices are local
pub type ProtcolDiscoveryHandlerMap = HashMap<String, DiscoveryHandlerDetails>;

/// Map of all registered Discovery Handlers where key is protocol
/// and value is a map of all Discovery Handlers for that protocol
pub type RegisteredDiscoveryHandlerMap = Arc<Mutex<HashMap<String, ProtcolDiscoveryHandlerMap>>>;

/// Describes the discoverability of an instance for this node
#[derive(PartialEq, Debug, Clone)]
pub enum DiscoveryHandlerConnectivityStatus {
    /// Has a client successfully using it
    HasClient,
    /// Registered but does not have a client
    Online,
    /// Not returning discovery results
    Offline(Instant),
}

#[derive(Debug, Clone)]
pub struct DiscoveryHandlerDetails {
    pub register_request: RegisterRequest,
    pub stop_discovery: broadcast::Sender<()>,
    pub connectivity_status: DiscoveryHandlerConnectivityStatus,
}

pub struct AgentRegistration {
    new_discovery_handler_sender: broadcast::Sender<String>,
    registered_discovery_handlers: RegisteredDiscoveryHandlerMap,
}

impl AgentRegistration {
    pub fn new(
        new_discovery_handler_sender: broadcast::Sender<String>,
        registered_discovery_handlers: RegisteredDiscoveryHandlerMap,
    ) -> Self {
        AgentRegistration {
            new_discovery_handler_sender,
            registered_discovery_handlers,
        }
    }
}

#[tonic::async_trait]
impl Registration for AgentRegistration {
    async fn register(&self, request: Request<RegisterRequest>) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        let protocol = req.protocol.clone();
        let endpoint = req.endpoint.clone();
        info!("register - called with register request {:?}", req);
        let (tx, _) = broadcast::channel(2);
        let discovery_handler_details = DiscoveryHandlerDetails {
            register_request: req.clone(),
            stop_discovery: tx,
            connectivity_status: DiscoveryHandlerConnectivityStatus::Online,
        };
        let mut registered_discovery_handlers = self.registered_discovery_handlers.lock().unwrap();
        // Check if the server is among the already registered servers for the protocol
        if let Some(register_request_map) = registered_discovery_handlers.get_mut(&protocol) {
            if let Some(dh_details) = register_request_map.get(&endpoint) {
                // check if DH at that endpoint is already registered but changed request
                if dh_details.register_request != req {
                    // stop current discovery
                    if dh_details.stop_discovery.send(()).is_err() {
                        error!(
                            "register - receiver for protocol {} and endpoint {} dropped",
                            protocol, endpoint
                        );
                    }
                } else {
                    // Already registered. Return early.
                    return Ok(Response::new(Empty {}));
                }
            }
            register_request_map.insert(endpoint, discovery_handler_details);
        } else {
            let mut register_request_map = HashMap::new();
            register_request_map.insert(endpoint, discovery_handler_details);
            registered_discovery_handlers.insert(protocol.clone(), register_request_map);
        }
        // If no configurations have been applied, no receivers can nor need to be updated about the new discovery handler
        if self
            .new_discovery_handler_sender
            .send(protocol.clone())
            .is_err()
        {
            trace!("register - new discovery handler registered for protocol {} but no active discovery operators to receive the message", protocol);
        }
        Ok(Response::new(Empty {}))
    }
}

pub async fn run_registration_server(
    discovery_hndlr_map: RegisteredDiscoveryHandlerMap,
    new_discovery_handler_sender: broadcast::Sender<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("run_registration_server - entered");
    let registration = AgentRegistration::new(new_discovery_handler_sender, discovery_hndlr_map);
    let addr = REGISTRATION_ENDPOINT.parse()?;
    trace!(
        "run_registration_server - registration server listening on {}",
        addr
    );
    // registration.register_local_dhs().await?;
    Server::builder()
        .add_service(RegistrationServer::new(registration))
        .serve(addr)
        .await?;
    Ok(())
}

#[cfg(feature = "agent-all-in-one")]
pub fn register_embedded_discovery_handlers(
    discovery_hndlr_map: RegisteredDiscoveryHandlerMap,
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    info!("register_embedded_discovery_handlers - entered");
    let mut register_requests: Vec<RegisterRequest> = Vec::new();
    register_requests.push(akri_debug_echo::get_register_request(
        EMBEDDED_DISCOVERY_HANDLER_ENDPOINT,
    ));
    #[cfg(feature = "onvif-feat")]
    register_requests.push(akri_onvif::get_register_request(
        EMBEDDED_DISCOVERY_HANDLER_ENDPOINT,
    ));

    register_requests.into_iter().for_each(|request| {
        let (tx, _) = broadcast::channel(2);
        let discovery_handler_details = DiscoveryHandlerDetails {
            register_request: request.clone(),
            stop_discovery: tx,
            connectivity_status: DiscoveryHandlerConnectivityStatus::Online,
        };
        let mut register_request_map = HashMap::new();
        register_request_map.insert(request.endpoint, discovery_handler_details);
        discovery_hndlr_map
            .lock()
            .unwrap()
            .insert(request.protocol, register_request_map);
    });
    Ok(())
}
