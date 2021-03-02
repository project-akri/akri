use ::url::Url;
use akri_discovery_utils::discovery::{
    v0::{
        registration_server::{Registration, RegistrationServer},
        Empty, RegisterRequest,
    },
    AGENT_REGISTRATION_SOCKET, DISCOVERY_HANDLER_PATH,
};
use akri_shared::uds::unix_stream;
use futures::TryStreamExt;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::broadcast;
use tonic::{transport::Server, Request, Response, Status};

/// Maximum amount of time allowed to pass without being able to connect to a discovery handler
/// without it being removed from the map of registered Discovery Handlers.
pub const DISCOVERY_HANDLER_OFFLINE_GRACE_PERIOD_SECS: u64 = 300;

/// Fake endpoint that signals a DiscoveryOperator to use an embedded
/// discovery handler.
pub const EMBEDDED_DISCOVERY_HANDLER_ENDPOINT: &str = "embedded";

// Map of RegisterRequests for a specific protocol where key is the endpoint of the Discovery Handler
// and value is whether or not the discovered devices are local.
pub type ProtocolDiscoveryHandlerMap = HashMap<DiscoveryHandlerEndpoint, DiscoveryHandlerDetails>;

/// Map of all registered Discovery Handlers where key is protocol
/// and value is a map of all Discovery Handlers for that protocol.
pub type RegisteredDiscoveryHandlerMap = Arc<Mutex<HashMap<String, ProtocolDiscoveryHandlerMap>>>;

/// A Discovery Handler's endpoint, distinguished by URI type
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DiscoveryHandlerEndpoint {
    /// Embedded means the Discovery Handler is running inside the Agent
    Embedded,
    /// Uds means the Discovery Handler is running on a specified unix domain socket
    Uds(String),
    /// Network means the Discovery Handler is running at an specified URL
    Network(String),
}

/// Describes the connectivity status of a Discovery Handler.
#[derive(PartialEq, Debug, Clone)]
pub enum DiscoveryHandlerStatus {
    /// This discovery handler is currently doing discovery on behalf of the Agent
    Active,
    /// This discovery handler is available and waiting for a discover call from the Agent
    Waiting,
    /// Not returning discovery results
    Offline(Instant),
}

#[derive(Debug, Clone)]
pub struct DiscoveryHandlerDetails {
    pub register_request: RegisterRequest,
    pub stop_discovery: broadcast::Sender<()>,
    pub connectivity_status: DiscoveryHandlerStatus,
}

pub fn get_external_endpoint_type(
    endpoint: &str,
) -> Result<DiscoveryHandlerEndpoint, url::ParseError> {
    if endpoint.starts_with(DISCOVERY_HANDLER_PATH) {
        Ok(DiscoveryHandlerEndpoint::Uds(endpoint.to_string()))
    } else {
        match Url::parse(endpoint) {
            Ok(_) => Ok(DiscoveryHandlerEndpoint::Network(endpoint.to_string())),
            Err(e) => Err(e),
        }
    }
}

/// Hosts a register service that external Discovery Handlers can call in order to be added to
/// the RegisteredDiscoveryHandlerMap that is shared with DiscoveryOperators.
/// When a new Discovery Handler is registered, a message is broadcast to inform any running DiscoveryOperators
/// in case they should use the new Discovery Handler.
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
    /// Adds new Discovery Handlers to the RegisteredDiscoveryHandlerMap and broadcasts a message to
    /// any running DiscoveryOperators that a new Discovery Handler exists.
    /// If the discovery handler is already registered at an endpoint and the register request has changed,
    /// the previously registered DH is told to stop discovery and is removed from the map. Then, the updated
    /// DH is registered.
    async fn register(&self, request: Request<RegisterRequest>) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        let protocol = req.protocol.clone();
        let endpoint = req.endpoint.clone();
        let dh_endpoint = get_external_endpoint_type(&endpoint).map_err(|e| {
            Status::new(
                tonic::Code::InvalidArgument,
                format!(
                    "Discovery Handler registered with invalid endpoint {} with parse error: {:?}",
                    endpoint, e
                ),
            )
        })?;
        info!("register - called with register request {:?}", req);
        let (tx, _) = broadcast::channel(2);
        let discovery_handler_details = DiscoveryHandlerDetails {
            register_request: req.clone(),
            stop_discovery: tx,
            connectivity_status: DiscoveryHandlerStatus::Waiting,
        };
        let mut registered_discovery_handlers = self.registered_discovery_handlers.lock().unwrap();
        // Check if the server is among the already registered servers for the protocol
        if let Some(register_request_map) = registered_discovery_handlers.get_mut(&protocol) {
            if let Some(dh_details) = register_request_map.get(&dh_endpoint) {
                // Check if DH at that endpoint is already registered but changed request
                if dh_details.register_request != req {
                    // Stop current discovery with this DH if any. A receiver may not exist if
                    // 1) no configuration has been applied that uses this DH or
                    // 2) a connection cannot be made with the DH's endpoint
                    dh_details.stop_discovery.send(()).unwrap_or_default();
                } else {
                    // Already registered. Return early.
                    return Ok(Response::new(Empty {}));
                }
            }
            // New or updated Discovery Handler for this protocol
            register_request_map.insert(dh_endpoint, discovery_handler_details);
        } else {
            // First Discovery Handler registered for this protocol
            let mut register_request_map = HashMap::new();
            register_request_map.insert(dh_endpoint, discovery_handler_details);
            registered_discovery_handlers.insert(protocol.clone(), register_request_map);
        }
        // Notify of new Discovery Handler
        if self
            .new_discovery_handler_sender
            .send(protocol.clone())
            .is_err()
        {
            // If no configurations have been applied, no receivers can nor need to be updated about the new discovery handler
            trace!("register - new discovery handler registered for protocol {} but no active discovery operators to receive the message", protocol);
        }
        Ok(Response::new(Empty {}))
    }
}

/// Serves the Agent registration service over UDS.
pub async fn run_registration_server(
    discovery_handler_map: RegisteredDiscoveryHandlerMap,
    new_discovery_handler_sender: broadcast::Sender<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    internal_run_registration_server(
        discovery_handler_map,
        new_discovery_handler_sender,
        AGENT_REGISTRATION_SOCKET,
    )
    .await
}

pub async fn internal_run_registration_server(
    discovery_handler_map: RegisteredDiscoveryHandlerMap,
    new_discovery_handler_sender: broadcast::Sender<String>,
    socket_path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("internal_run_registration_server - entered");
    let registration = AgentRegistration::new(new_discovery_handler_sender, discovery_handler_map);
    trace!(
        "internal_run_registration_server - registration server listening on socket {}",
        socket_path
    );
    // Delete socket in case previously created/used
    std::fs::remove_file(&socket_path).unwrap_or(());
    let mut uds =
        tokio::net::UnixListener::bind(socket_path).expect("Failed to bind to socket path");
    Server::builder()
        .add_service(RegistrationServer::new(registration))
        .serve_with_incoming(uds.incoming().map_ok(unix_stream::UnixStream))
        .await?;
    trace!(
        "serve - gracefully shutdown ... deleting socket {}",
        socket_path
    );
    std::fs::remove_file(socket_path).unwrap_or(());
    Ok(())
}

/// Adds all embedded Discovery Handlers to the RegisteredDiscoveryHandlerMap,
/// specifying an endpoint of Endpoint::Embedded to signal that it is an embedded Discovery Handler.
#[cfg(any(test, feature = "agent-all-in-one"))]
pub fn register_embedded_discovery_handlers(
    discovery_handler_map: RegisteredDiscoveryHandlerMap,
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    info!("register_embedded_discovery_handlers - entered");
    let mut register_requests: Vec<RegisterRequest> = Vec::new();
    register_requests.push(create_register_request(
        akri_debug_echo::PROTOCOL_NAME,
        EMBEDDED_DISCOVERY_HANDLER_ENDPOINT,
        akri_debug_echo::IS_LOCAL,
    ));
    #[cfg(feature = "onvif-feat")]
    register_requests.push(create_register_request(
        akri_onvif::PROTOCOL_NAME,
        EMBEDDED_DISCOVERY_HANDLER_ENDPOINT,
        akri_onvif::IS_LOCAL,
    ));
    #[cfg(feature = "udev-feat")]
    register_requests.push(create_register_request(
        akri_udev::PROTOCOL_NAME,
        EMBEDDED_DISCOVERY_HANDLER_ENDPOINT,
        akri_udev::IS_LOCAL,
    ));
    #[cfg(feature = "opcua-feat")]
    register_requests.push(create_register_request(
        akri_opcua::PROTOCOL_NAME,
        EMBEDDED_DISCOVERY_HANDLER_ENDPOINT,
        akri_opcua::IS_LOCAL,
    ));

    register_requests.into_iter().for_each(|request| {
        let (tx, _) = broadcast::channel(2);
        let discovery_handler_details = DiscoveryHandlerDetails {
            register_request: request.clone(),
            stop_discovery: tx,
            connectivity_status: DiscoveryHandlerStatus::Waiting,
        };
        let mut register_request_map = HashMap::new();
        register_request_map.insert(
            DiscoveryHandlerEndpoint::Embedded,
            discovery_handler_details,
        );
        discovery_handler_map
            .lock()
            .unwrap()
            .insert(request.protocol, register_request_map);
    });
    Ok(())
}

fn create_register_request(protocol: &str, endpoint: &str, is_local: bool) -> RegisterRequest {
    RegisterRequest {
        protocol: protocol.to_string(),
        endpoint: endpoint.to_string(),
        is_local,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use akri_discovery_utils::discovery::v0::registration_client::RegistrationClient;
    use std::convert::TryFrom;
    use tempfile::Builder;
    use tokio::net::UnixStream;
    use tonic::transport::{Endpoint, Uri};

    #[test]
    fn test_register_embedded_discovery_handlers() {
        let discovery_handler_map = Arc::new(Mutex::new(HashMap::new()));
        register_embedded_discovery_handlers(discovery_handler_map.clone()).unwrap();
        assert_eq!(discovery_handler_map.lock().unwrap().len(), 4);
        assert!(discovery_handler_map
            .lock()
            .unwrap()
            .get("debugEcho")
            .is_some());
        #[cfg(feature = "onvif-feat")]
        assert!(discovery_handler_map.lock().unwrap().get("onvif").is_some());
        #[cfg(feature = "opcua-feat")]
        assert!(discovery_handler_map.lock().unwrap().get("opcua").is_some());
        #[cfg(feature = "udev-feat")]
        assert!(discovery_handler_map.lock().unwrap().get("udev").is_some());
    }

    #[tokio::test]
    async fn test_run_registration_server_reregister() {
        let registration_socket_dir = Builder::new().tempdir().unwrap();
        let registration_socket_path = registration_socket_dir
            .path()
            .join("agent-registration.sock");
        let registration_socket_path_string_thread =
            registration_socket_path.to_str().unwrap().to_string();
        let registration_socket_path_string =
            registration_socket_path.to_str().unwrap().to_string();
        let (new_discovery_handler_sender, mut new_discovery_handler_receiver) =
            broadcast::channel(4);
        let discovery_handler_map = Arc::new(Mutex::new(HashMap::new()));
        let thread_discovery_handler_map = discovery_handler_map.clone();

        // Run registration service
        tokio::spawn(async move {
            internal_run_registration_server(
                thread_discovery_handler_map,
                new_discovery_handler_sender,
                &registration_socket_path_string_thread,
            )
            .await
            .unwrap();
        });

        // Make sure registration service is running
        assert!(unix_stream::try_connect(&registration_socket_path_string)
            .await
            .is_ok());
        // Connect to registration service
        let channel = Endpoint::try_from("dummy://[::]:50051")
            .unwrap()
            .connect_with_connector(tower::service_fn(move |_: Uri| {
                UnixStream::connect(registration_socket_path_string.clone())
            }))
            .await
            .unwrap();
        // Create registration client
        let mut registration_client = RegistrationClient::new(channel);

        // Test registering a discovery handler with appropriate UDS endpoint
        // Registration expects uds endpoint to begin with DISCOVERY_HANDLER_PATH
        let endpoint_string = format!("{}/protocol.sock", DISCOVERY_HANDLER_PATH);
        let discovery_handler_endpoint = DiscoveryHandlerEndpoint::Uds(endpoint_string.clone());
        let request = RegisterRequest {
            protocol: "protocol".to_string(),
            endpoint: endpoint_string.clone(),
            is_local: false,
        };
        assert!(registration_client.register(request.clone()).await.is_ok());
        assert_eq!(
            new_discovery_handler_receiver.recv().await.unwrap(),
            "protocol"
        );
        let discovery_handler_details = discovery_handler_map
            .lock()
            .unwrap()
            .get("protocol")
            .unwrap()
            .get(&discovery_handler_endpoint)
            .unwrap()
            .clone();
        assert_eq!(discovery_handler_details.register_request, request);

        // When a discovery handler is re-registered with the same register request, no message should be
        // sent to terminate any existing discovery clients.
        let mut stop_discovery_receiver = discovery_handler_details.stop_discovery.subscribe();
        assert!(registration_client.register(request).await.is_ok());
        assert!(stop_discovery_receiver.try_recv().is_err());

        // When a discovery handler at a specified endpoint re-registers at the same endpoint but with a different locality
        // current discovery handler clients should be notified to terminate and the entry in the
        // RegisteredDiscoveryHandlersMap should be replaced.
        let local_request = RegisterRequest {
            protocol: "protocol".to_string(),
            endpoint: endpoint_string,
            is_local: true,
        };
        assert!(registration_client
            .register(local_request.clone())
            .await
            .is_ok());
        assert!(stop_discovery_receiver.try_recv().is_ok());
        let discovery_handler_details = discovery_handler_map
            .lock()
            .unwrap()
            .get("protocol")
            .unwrap()
            .get(&discovery_handler_endpoint)
            .unwrap()
            .clone();
        assert_eq!(discovery_handler_details.register_request, local_request);
    }

    async fn assert_registration_of_valid_endpoint(
        endpoint_str: &str,
        endpoint_dh: DiscoveryHandlerEndpoint,
    ) {
        let (new_discovery_handler_sender, mut new_discovery_handler_receiver) =
            broadcast::channel(2);
        let discovery_handler_map = Arc::new(Mutex::new(HashMap::new()));
        let registration_server =
            AgentRegistration::new(new_discovery_handler_sender, discovery_handler_map.clone());
        let request = RegisterRequest {
            protocol: "protocol".to_string(),
            endpoint: endpoint_str.to_string(),
            is_local: false,
        };
        let tonic_request = tonic::Request::new(request.clone());
        assert!(registration_server.register(tonic_request).await.is_ok());
        assert_eq!(
            new_discovery_handler_receiver.recv().await.unwrap(),
            "protocol"
        );
        let discovery_handler_details = discovery_handler_map
            .lock()
            .unwrap()
            .get("protocol")
            .unwrap()
            .get(&endpoint_dh)
            .unwrap()
            .clone();
        assert_eq!(discovery_handler_details.register_request, request);
    }

    // Test register for all the proper and improper endpoint types
    #[tokio::test]
    async fn test_register_factory() {
        // Test that a network endpoint is successfully registered
        let endpoint = "http://10.1.2.3:1000";
        let discovery_handler_endpoint = DiscoveryHandlerEndpoint::Network(endpoint.to_string());
        assert_registration_of_valid_endpoint(endpoint, discovery_handler_endpoint).await;

        // Test that a UDS endpoint with proper path is successfully registered
        let endpoint = format!("{}/socket.sock", DISCOVERY_HANDLER_PATH);
        let discovery_handler_endpoint = DiscoveryHandlerEndpoint::Uds(endpoint.clone());
        assert_registration_of_valid_endpoint(&endpoint, discovery_handler_endpoint).await;

        // Test registering a discovery handler with IMPROPER UDS endpoint
        // Registration expects uds endpoint to begin with DISCOVERY_HANDLER_PATH
        let (new_discovery_handler_sender, _) = broadcast::channel(2);
        let discovery_handler_map = Arc::new(Mutex::new(HashMap::new()));
        let registration_server =
            AgentRegistration::new(new_discovery_handler_sender, discovery_handler_map.clone());
        let endpoint = "/improper/socket/path/protocol.sock";
        let request = tonic::Request::new(RegisterRequest {
            protocol: "protocol".to_string(),
            endpoint: endpoint.to_string(),
            is_local: false,
        });
        match registration_server.register(request).await {
            Ok(_) => panic!("register should panic if given unexpected uds"),
            Err(e) => assert_eq!(e.code(), tonic::Code::InvalidArgument),
        }

        // Test registering a discovery handler with IMPROPER network address
        let (new_discovery_handler_sender, _) = broadcast::channel(2);
        let discovery_handler_map = Arc::new(Mutex::new(HashMap::new()));
        let registration_server =
            AgentRegistration::new(new_discovery_handler_sender, discovery_handler_map.clone());
        let endpoint = "10.1.2.3:1000";
        let request = tonic::Request::new(RegisterRequest {
            protocol: "protocol".to_string(),
            endpoint: endpoint.to_string(),
            is_local: false,
        });
        match registration_server.register(request).await {
            Ok(_) => panic!("register should panic if given unexpected uds"),
            Err(e) => assert_eq!(e.code(), tonic::Code::InvalidArgument),
        }
    }
}
