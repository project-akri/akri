use tonic::{transport::Server, Request, Response, Status};

use super::discovery::registration_server::{Registration, RegistrationServer};
use super::discovery::{Empty, RegisterRequest};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
const REGISTRATION_ENDPOINT: &str = "[::1]:10000";

// Map of RegisterRequests for a specific protocol where key is the endpoint of the Discovery Handler
// and value is whether or not the discovered devices are local
pub type RegisterRequestMap = HashMap<String, bool>;

// Map of all registered Discovery Handlers
pub type RegisteredDiscoveryHandlerMap = Arc<Mutex<HashMap<String, RegisterRequestMap>>>;

pub enum DiscoveryHandlerStatus {
    HasClient,
    Offline,
    Unused,
}

pub struct AgentRegistration {
    registered_discovery_handlers: RegisteredDiscoveryHandlerMap,
}

impl AgentRegistration {
    fn new(registered_discovery_handlers: RegisteredDiscoveryHandlerMap) -> Self {
        AgentRegistration {
            registered_discovery_handlers,
        }
    }
}

#[tonic::async_trait]
impl Registration for AgentRegistration {
    async fn register(&self, request: Request<RegisterRequest>) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        let protocol = req.protocol;
        let endpoint = req.endpoint;
        let is_local = req.is_local;
        let mut registered_discovery_handlers = self.registered_discovery_handlers.lock().unwrap();
        // Check if the server is among the already registered servers for the protocol
        if let Some(register_request_map) = registered_discovery_handlers.get_mut(&protocol) {
            if let Some(is_local) = register_request_map.get(&endpoint) {
                // check if locality changed
                if is_local != is_local {
                    panic!("change is locality is not supported yet");
                }
            } else {
                register_request_map.insert(endpoint, is_local);
            }
        } else {
            let mut register_request_map = HashMap::new();
            register_request_map.insert(endpoint, is_local);
            registered_discovery_handlers.insert(protocol, register_request_map);
        }
        Ok(Response::new(Empty {}))
    }
}

pub async fn run_registration_server(
    registered_discovery_handlers: RegisteredDiscoveryHandlerMap,
) -> Result<(), Box<dyn std::error::Error>> {
    let addr = REGISTRATION_ENDPOINT.parse()?;
    let registration = AgentRegistration::new(registered_discovery_handlers);
    Server::builder()
        .add_service(RegistrationServer::new(registration))
        .serve(addr)
        .await?;
    Ok(())
}
