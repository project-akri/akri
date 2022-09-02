use super::constants::CLOSE_DISCOVERY_HANDLER_CONNECTION_CHANNEL_CAPACITY;
#[cfg(any(test, feature = "agent-full"))]
use super::constants::ENABLE_DEBUG_ECHO_LABEL;
use akri_discovery_utils::discovery::v0::{
    register_discovery_handler_request::EndpointType,
    registration_server::{Registration, RegistrationServer},
    Empty, RegisterDiscoveryHandlerRequest,
};
#[cfg(any(test, feature = "agent-full"))]
use akri_shared::os::env_var::{ActualEnvVarQuery, EnvVarQuery};
use akri_shared::uds::unix_stream;
use futures::TryFutureExt;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;
use tonic::{transport::Server, Request, Response, Status};

/// Map of `DiscoveryHandlers` of the same type (registered with the same name) where key is the endpoint of the
/// Discovery Handler and value is `DiscoveryDetails`.
pub type DiscoveryHandlerDetailsMap = HashMap<DiscoveryHandlerEndpoint, DiscoveryDetails>;

/// Map of all registered `DiscoveryHandlers` where key is `DiscoveryHandler` name and value is a map of all
/// `DiscoveryHandlers` with that name.
pub type RegisteredDiscoveryHandlerMap =
    Arc<Mutex<HashMap<DiscoveryHandlerName, DiscoveryHandlerDetailsMap>>>;

/// Alias illustrating that `AgentRegistration.new_discovery_handler_sender`, sends the Discovery Handler name of the
/// newly registered Discovery Handler.
pub type DiscoveryHandlerName = String;

/// A Discovery Handler's endpoint, distinguished by URI type
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DiscoveryHandlerEndpoint {
    /// Embedded means the Discovery Handler is running inside the Agent
    #[cfg(any(test, feature = "agent-full"))]
    Embedded,
    /// Uds means the Discovery Handler is running on a specified unix domain socket
    Uds(String),
    /// Network means the Discovery Handler is running at an specified URL
    Network(String),
}

/// Details about a `DiscoveryHandler` and a sender for terminating its clients when needed.
#[derive(Debug, Clone)]
pub struct DiscoveryDetails {
    /// Name of the `DiscoveryHandler`
    pub name: String,
    /// Endpoint of the `DiscoveryHandler`
    pub endpoint: DiscoveryHandlerEndpoint,
    /// Whether instances discovered by the `DiscoveryHandler` can be shared/seen by multiple nodes.
    pub shared: bool,
    /// Channel over which the Registration service tells a DiscoveryOperator client to close a connection with a
    /// `DiscoveryHandler`, if any. A broadcast channel is used so both the sending and receiving ends can be cloned.
    pub close_discovery_handler_connection: broadcast::Sender<()>,
}

/// This maps the endpoint string and endpoint type of a `RegisterDiscoveryHandlerRequest` into a
/// `DiscoveryHandlerEndpoint` so as to support embedded `DiscoveryHandlers`.
pub fn create_discovery_handler_endpoint(
    endpoint: &str,
    endpoint_type: EndpointType,
) -> DiscoveryHandlerEndpoint {
    match endpoint_type {
        EndpointType::Network => DiscoveryHandlerEndpoint::Network(endpoint.to_string()),
        EndpointType::Uds => DiscoveryHandlerEndpoint::Uds(endpoint.to_string()),
    }
}

/// Hosts a register service that external Discovery Handlers can call in order to be added to the
/// RegisteredDiscoveryHandlerMap that is shared with DiscoveryOperators. When a new Discovery Handler is registered, a
/// message is broadcast to inform any running DiscoveryOperators in case they should use the new Discovery Handler.
pub struct AgentRegistration {
    new_discovery_handler_sender: broadcast::Sender<DiscoveryHandlerName>,
    registered_discovery_handlers: RegisteredDiscoveryHandlerMap,
}

impl AgentRegistration {
    pub fn new(
        new_discovery_handler_sender: broadcast::Sender<DiscoveryHandlerName>,
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
    /// Adds new `DiscoveryHandler`s to the RegisteredDiscoveryHandlerMap and broadcasts a message to any running
    /// DiscoveryOperators that a new `DiscoveryHandler` exists. If the discovery handler is already registered at an
    /// endpoint and the register request has changed, the previously registered DH is told to stop discovery and is
    /// removed from the map. Then, the updated DH is registered.
    async fn register_discovery_handler(
        &self,
        request: Request<RegisterDiscoveryHandlerRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        let dh_name = req.name.clone();
        let endpoint = req.endpoint.clone();
        let dh_endpoint = create_discovery_handler_endpoint(
            &endpoint,
            EndpointType::from_i32(req.endpoint_type).unwrap(),
        );
        info!(
            "register_discovery_handler - called with register request {:?}",
            req
        );
        let (close_discovery_handler_connection, _) =
            broadcast::channel(CLOSE_DISCOVERY_HANDLER_CONNECTION_CHANNEL_CAPACITY);
        let discovery_handler_details = DiscoveryDetails {
            name: dh_name.clone(),
            endpoint: dh_endpoint.clone(),
            shared: req.shared,
            close_discovery_handler_connection,
        };
        let mut registered_discovery_handlers = self.registered_discovery_handlers.lock().unwrap();
        // Check if any DiscoveryHandlers have been registered under this name
        if let Some(register_request_map) = registered_discovery_handlers.get_mut(&dh_name) {
            if let Some(dh_details) = register_request_map.get(&dh_endpoint) {
                // Check if DH at that endpoint is already registered but changed request
                if dh_details.shared != req.shared || dh_details.endpoint != dh_endpoint {
                    // Stop current discovery with this DH if any. A receiver may not exist if
                    // 1) no configuration has been applied that uses this DH or
                    // 2) a connection cannot be made with the DH's endpoint
                    dh_details
                        .close_discovery_handler_connection
                        .send(())
                        .unwrap_or_default();
                } else {
                    // Already registered. Return early.
                    return Ok(Response::new(Empty {}));
                }
            }
            // New or updated Discovery Handler
            register_request_map.insert(dh_endpoint, discovery_handler_details);
        } else {
            // First Discovery Handler registered under this name
            let mut register_request_map = HashMap::new();
            register_request_map.insert(dh_endpoint, discovery_handler_details);
            registered_discovery_handlers.insert(dh_name.clone(), register_request_map);
        }
        // Notify of new Discovery Handler
        if self
            .new_discovery_handler_sender
            .send(dh_name.clone())
            .is_err()
        {
            // If no configurations have been applied, no receivers can nor need to be updated about the new discovery
            // handler
            trace!("register_discovery_handler - new {} discovery handler registered but no active discovery operators to receive the message", dh_name);
        }
        Ok(Response::new(Empty {}))
    }
}

/// Serves the Agent registration service over UDS.
pub async fn run_registration_server(
    discovery_handler_map: RegisteredDiscoveryHandlerMap,
    new_discovery_handler_sender: broadcast::Sender<DiscoveryHandlerName>,
) -> Result<(), Box<dyn std::error::Error>> {
    internal_run_registration_server(
        discovery_handler_map,
        new_discovery_handler_sender,
        &akri_discovery_utils::get_registration_socket(),
    )
    .await
}

pub async fn internal_run_registration_server(
    discovery_handler_map: RegisteredDiscoveryHandlerMap,
    new_discovery_handler_sender: broadcast::Sender<DiscoveryHandlerName>,
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
    let incoming = {
        let uds =
            tokio::net::UnixListener::bind(socket_path).expect("Failed to bind to socket path");

        async_stream::stream! {
            loop {
                let item = uds.accept().map_ok(|(st, _)| unix_stream::UnixStream(st)).await;
                yield item;
            }
        }
    };
    Server::builder()
        .add_service(RegistrationServer::new(registration))
        .serve_with_incoming(incoming)
        .await?;
    trace!(
        "internal_run_registration_server - gracefully shutdown ... deleting socket {}",
        socket_path
    );
    std::fs::remove_file(socket_path).unwrap_or(());
    Ok(())
}

#[cfg(any(test, feature = "agent-full"))]
pub fn register_embedded_discovery_handlers(
    discovery_handler_map: RegisteredDiscoveryHandlerMap,
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    info!("register_embedded_discovery_handlers - entered");
    let env_var_query = ActualEnvVarQuery {};
    inner_register_embedded_discovery_handlers(discovery_handler_map, &env_var_query)?;
    Ok(())
}

/// Adds all embedded Discovery Handlers to the RegisteredDiscoveryHandlerMap, specifying an endpoint of
/// Endpoint::Embedded to signal that it is an embedded Discovery Handler.
#[cfg(any(test, feature = "agent-full"))]
pub fn inner_register_embedded_discovery_handlers(
    discovery_handler_map: RegisteredDiscoveryHandlerMap,
    query: &impl EnvVarQuery,
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    type Details = (String, bool);
    let mut embedded_discovery_handlers: Vec<Details> = Vec::new();
    if query.get_env_var(ENABLE_DEBUG_ECHO_LABEL).is_ok() {
        let shared: bool = query
            .get_env_var(akri_debug_echo::DEBUG_ECHO_INSTANCES_SHARED_LABEL)
            .unwrap()
            .parse()
            .unwrap();
        embedded_discovery_handlers
            .push((akri_debug_echo::DISCOVERY_HANDLER_NAME.to_string(), shared));
    }
    #[cfg(feature = "onvif-feat")]
    embedded_discovery_handlers.push((
        akri_onvif::DISCOVERY_HANDLER_NAME.to_string(),
        akri_onvif::SHARED,
    ));
    #[cfg(feature = "udev-feat")]
    embedded_discovery_handlers.push((
        akri_udev::DISCOVERY_HANDLER_NAME.to_string(),
        akri_udev::SHARED,
    ));
    #[cfg(feature = "opcua-feat")]
    embedded_discovery_handlers.push((
        akri_opcua::DISCOVERY_HANDLER_NAME.to_string(),
        akri_opcua::SHARED,
    ));

    embedded_discovery_handlers.into_iter().for_each(|dh| {
        let (name, shared) = dh;
        let (close_discovery_handler_connection, _) =
            broadcast::channel(CLOSE_DISCOVERY_HANDLER_CONNECTION_CHANNEL_CAPACITY);
        let discovery_handler_details = DiscoveryDetails {
            name: name.clone(),
            endpoint: DiscoveryHandlerEndpoint::Embedded,
            shared,
            close_discovery_handler_connection,
        };
        let mut register_request_map = HashMap::new();
        register_request_map.insert(
            DiscoveryHandlerEndpoint::Embedded,
            discovery_handler_details,
        );
        discovery_handler_map
            .lock()
            .unwrap()
            .insert(name, register_request_map);
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use akri_discovery_utils::discovery::v0::registration_client::RegistrationClient;
    use akri_shared::os::env_var::MockEnvVarQuery;
    use std::convert::TryFrom;
    use tempfile::Builder;
    use tokio::net::UnixStream;
    use tonic::transport::{Endpoint, Uri};

    #[test]
    fn test_register_embedded_discovery_handlers() {
        let mut seq = mockall::Sequence::new();
        // Enable debug echo and set environment variable to set whether debug echo instances are shared
        let mut mock_env_var = MockEnvVarQuery::new();
        mock_env_var
            .expect_get_env_var()
            .times(1)
            .withf(|label: &str| label == ENABLE_DEBUG_ECHO_LABEL)
            .in_sequence(&mut seq)
            .returning(|_| Ok("1".to_string()));
        mock_env_var
            .expect_get_env_var()
            .times(1)
            .withf(|label: &str| label == akri_debug_echo::DEBUG_ECHO_INSTANCES_SHARED_LABEL)
            .in_sequence(&mut seq)
            .returning(|_| Ok("false".to_string()));
        let discovery_handler_map = Arc::new(Mutex::new(HashMap::new()));
        inner_register_embedded_discovery_handlers(discovery_handler_map.clone(), &mock_env_var)
            .unwrap();
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

    #[test]
    fn test_register_embedded_discovery_handlers_no_debug_echo() {
        let mut mock_env_var = MockEnvVarQuery::new();
        mock_env_var
            .expect_get_env_var()
            .times(1)
            .withf(|label: &str| label == ENABLE_DEBUG_ECHO_LABEL)
            .returning(|_| Err(std::env::VarError::NotPresent));
        let discovery_handler_map = Arc::new(Mutex::new(HashMap::new()));
        inner_register_embedded_discovery_handlers(discovery_handler_map.clone(), &mock_env_var)
            .unwrap();
        assert!(discovery_handler_map
            .lock()
            .unwrap()
            .get("debugEcho")
            .is_none());
    }

    #[tokio::test]
    async fn test_run_registration_server_reregister_discovery_handler() {
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
        let channel = Endpoint::try_from("http://[::]:50051")
            .unwrap()
            .connect_with_connector(tower::service_fn(move |_: Uri| {
                UnixStream::connect(registration_socket_path_string.clone())
            }))
            .await
            .unwrap();
        // Create registration client
        let mut registration_client = RegistrationClient::new(channel);

        // Test registering a discovery handler with UDS endpoint
        let endpoint_string = "/path/to/socket/name.sock".to_string();
        let discovery_handler_endpoint = DiscoveryHandlerEndpoint::Uds(endpoint_string.clone());
        let request = RegisterDiscoveryHandlerRequest {
            name: "name".to_string(),
            endpoint: endpoint_string.clone(),
            endpoint_type: EndpointType::Uds as i32,
            shared: true,
        };
        assert!(registration_client
            .register_discovery_handler(request.clone())
            .await
            .is_ok());
        assert_eq!(new_discovery_handler_receiver.recv().await.unwrap(), "name");
        let discovery_handler_details = discovery_handler_map
            .lock()
            .unwrap()
            .get("name")
            .unwrap()
            .get(&discovery_handler_endpoint)
            .unwrap()
            .clone();
        assert_eq!(
            discovery_handler_details.endpoint,
            DiscoveryHandlerEndpoint::Uds(request.endpoint.clone())
        );
        assert_eq!(discovery_handler_details.shared, request.shared);

        // When a discovery handler is re-registered with the same register request, no message should be sent to
        // terminate any existing discovery clients.
        let mut stop_discovery_receiver = discovery_handler_details
            .close_discovery_handler_connection
            .subscribe();
        assert!(registration_client
            .register_discovery_handler(request)
            .await
            .is_ok());
        assert!(stop_discovery_receiver.try_recv().is_err());

        // When a discovery handler at a specified endpoint re-registers at the same endpoint but with a different
        // locality current discovery handler clients should be notified to terminate and the entry in the
        // RegisteredDiscoveryHandlersMap should be replaced.
        let local_request = RegisterDiscoveryHandlerRequest {
            name: "name".to_string(),
            endpoint: endpoint_string,
            endpoint_type: EndpointType::Uds as i32,
            shared: false,
        };
        assert!(registration_client
            .register_discovery_handler(local_request.clone())
            .await
            .is_ok());
        assert!(stop_discovery_receiver.try_recv().is_ok());
        let discovery_handler_details = discovery_handler_map
            .lock()
            .unwrap()
            .get("name")
            .unwrap()
            .get(&discovery_handler_endpoint)
            .unwrap()
            .clone();
        assert_eq!(
            discovery_handler_details.endpoint,
            DiscoveryHandlerEndpoint::Uds(local_request.endpoint)
        );
        assert_eq!(discovery_handler_details.shared, local_request.shared);
    }

    #[test]
    fn test_create_discovery_handler_endpoint() {
        // Assert the endpoint with EndpointType::Uds in converted to DiscoveryHandlerEndpoint::Uds(endpoint)
        assert_eq!(
            create_discovery_handler_endpoint("/path/to/socket.sock", EndpointType::Uds),
            DiscoveryHandlerEndpoint::Uds("/path/to/socket.sock".to_string())
        );

        // Assert the endpoint with EndpointType::Network in converted to DiscoveryHandlerEndpoint::Network(endpoint)
        assert_eq!(
            create_discovery_handler_endpoint("http://10.1.2.3:1000", EndpointType::Network),
            DiscoveryHandlerEndpoint::Network("http://10.1.2.3:1000".to_string())
        );
    }
}
