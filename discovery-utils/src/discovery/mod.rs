pub mod v0;

/// Path of the Agent registration socket
pub const AGENT_REGISTRATION_SOCKET: &str = "/var/lib/akri/agent-registration.sock";

/// Folder in which the Agent expects to find discovery handler sockets.
pub const DISCOVERY_HANDLER_PATH: &str = "/var/lib/akri";

/// Definition of the DiscoverStream type expected for supported embedded Akri DiscoveryHandlers
pub type DiscoverStream = tokio::sync::mpsc::Receiver<Result<v0::DiscoverResponse, tonic::Status>>;

pub mod discovery_handler {
    use super::super::registration_client::{register, register_again};
    use super::{
        server::run_discovery_server,
        v0::{discovery_server::Discovery, RegisterRequest},
        DISCOVERY_HANDLER_PATH,
    };
    use log::trace;
    use tokio::sync::mpsc;
    const DISCOVERY_PORT: i16 = 10000;
    pub async fn run_discovery_handler(
        discovery_handler: impl Discovery,
        register_receiver: mpsc::Receiver<()>,
        protocol_name: &str,
        is_local: bool,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        let mut use_uds = true;
        let mut endpoint: String = match std::env::var("POD_IP") {
            Ok(pod_ip) => {
                trace!("main - registering with Agent with IP endpoint");
                use_uds = false;
                format!("{}:{}", pod_ip, DISCOVERY_PORT)
            }
            Err(_) => {
                trace!("main - registering with Agent with uds endpoint");
                format!("{}/{}.sock", DISCOVERY_HANDLER_PATH, protocol_name)
            }
        };
        let endpoint_clone = endpoint.clone();
        let discovery_handle = tokio::spawn(async move {
            run_discovery_server(discovery_handler, &endpoint_clone)
                .await
                .unwrap();
        });
        if !use_uds {
            endpoint.insert_str(0, "http://");
        }
        let register_request = RegisterRequest {
            protocol: protocol_name.to_string(),
            endpoint,
            is_local,
        };
        register(&register_request).await?;
        let registration_handle = tokio::spawn(async move {
            register_again(register_receiver, &register_request).await;
        });
        tokio::try_join!(discovery_handle, registration_handle)?;
        Ok(())
    }

    /// This obtains the expected type `T` from a discovery details map
    /// by running it through function `f` which will attempt to deserialize the String.
    /// It expects `T` to be serialized yaml stored in the map as
    /// the String value associated with the key `protocolHandler`.
    pub fn deserialize_discovery_details<T>(
        discovery_details: &std::collections::HashMap<String, String>,
    ) -> Result<T, anyhow::Error>
    where
        T: serde::de::DeserializeOwned,
    {
        if let Some(discovery_handler_str) = discovery_details.get("protocolHandler") {
            let discovery_handler_config: T =
                serde_yaml::from_str(discovery_handler_str).map_err(|e| {
                    anyhow::format_err!(
                        "Configuration discovery details improperly configured with error {:?}",
                        e
                    )
                })?;
            Ok(discovery_handler_config)
        } else {
            Err(anyhow::format_err!(
                "Expected discovery information to be stored under key 'protocolHandler' in Config discovery details: {:?}",
                discovery_details
            ))
        }
    }
}

#[cfg(any(feature = "mock-discovery-handler", test))]
pub mod mock_discovery_handler {
    use super::v0::{discovery_server::Discovery, DiscoverRequest, DiscoverResponse};
    use akri_shared::uds::unix_stream;
    use async_trait::async_trait;
    use tempfile::Builder;
    use tokio::sync::mpsc;

    /// Simple discovery handler for tests
    pub struct MockDiscoveryHandler {}

    #[async_trait]
    impl Discovery for MockDiscoveryHandler {
        type DiscoverStream = super::DiscoverStream;
        async fn discover(
            &self,
            _: tonic::Request<DiscoverRequest>,
        ) -> Result<tonic::Response<Self::DiscoverStream>, tonic::Status> {
            let (mut tx, rx) = mpsc::channel(4);
            tokio::spawn(async move {
                tx.send(Ok(DiscoverResponse {
                    devices: Vec::new(),
                }))
                .await
                .unwrap();
            });
            Ok(tonic::Response::new(rx))
        }
    }

    pub fn get_mock_discovery_handler_dir_and_endpoint(socket_name: &str) -> (String, String) {
        let discovery_handler_temp_dir = Builder::new()
            .prefix("discovery-handlers")
            .tempdir()
            .unwrap();
        let discovery_handler_temp_dir_path = discovery_handler_temp_dir.path().join(socket_name);
        (
            discovery_handler_temp_dir
                .path()
                .to_str()
                .unwrap()
                .to_string(),
            discovery_handler_temp_dir_path
                .to_str()
                .unwrap()
                .to_string(),
        )
    }

    pub async fn run_mock_discovery_handler(
        discovery_handler_dir: &str,
        discovery_handler_endpoint: &str,
    ) -> tokio::task::JoinHandle<()> {
        let discovery_handler = MockDiscoveryHandler {};
        let discovery_handler_dir_string = discovery_handler_dir.to_string();
        let discovery_handler_endpoint_string = discovery_handler_endpoint.to_string();
        let handle = tokio::spawn(async move {
            super::server::internal_run_discovery_server(
                discovery_handler,
                &discovery_handler_endpoint_string,
                &discovery_handler_dir_string,
            )
            .await
            .unwrap();
        });

        // Try to connect in loop until first thread has served Discovery Handler
        unix_stream::try_connect(discovery_handler_endpoint)
            .await
            .unwrap();
        handle
    }
}

pub mod server {
    use super::v0::discovery_server::{Discovery, DiscoveryServer};
    use akri_shared::uds::unix_stream;
    use futures::stream::TryStreamExt;
    use log::info;
    use std::path::Path;
    use tokio::net::UnixListener;
    use tonic::transport::Server;

    pub async fn run_discovery_server(
        discovery_handler: impl Discovery,
        discovery_endpoint: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        internal_run_discovery_server(
            discovery_handler,
            discovery_endpoint,
            super::DISCOVERY_HANDLER_PATH,
        )
        .await
    }

    /// Creates a DiscoveryServer for the given Discovery Handler at the specified endpoint
    /// Verifies the endpoint by checking that it is in the discovery handler directory if it is
    /// UDS or that it is a valid IP address and port.
    pub async fn internal_run_discovery_server(
        discovery_handler: impl Discovery,
        discovery_endpoint: &str,
        discovery_handler_directory: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        info!("internal_run_discovery_server - entered");

        if discovery_endpoint.starts_with(discovery_handler_directory) {
            tokio::fs::create_dir_all(Path::new(&discovery_endpoint[..]).parent().unwrap()).await?;
            // Delete socket if it already exists
            std::fs::remove_file(discovery_endpoint).unwrap_or(());
            let mut uds = UnixListener::bind(discovery_endpoint)?;
            Server::builder()
                .add_service(DiscoveryServer::new(discovery_handler))
                .serve_with_incoming(uds.incoming().map_ok(unix_stream::UnixStream))
                .await?;
            std::fs::remove_file(discovery_endpoint).unwrap_or(());
        } else {
            let addr = discovery_endpoint.parse()?;
            Server::builder()
                .add_service(DiscoveryServer::new(discovery_handler))
                .serve(addr)
                .await?;
        }
        info!("internal_run_discovery_server - finished");
        Ok(())
    }

    #[cfg(test)]
    pub mod tests {
        use super::super::{
            mock_discovery_handler::{
                get_mock_discovery_handler_dir_and_endpoint, run_mock_discovery_handler,
                MockDiscoveryHandler,
            },
            v0::{discovery_client::DiscoveryClient, DiscoverRequest},
        };
        use super::*;
        use std::convert::TryFrom;
        use tempfile::Builder;
        use tokio::net::UnixStream;
        use tonic::{
            transport::{Endpoint, Uri},
            Request,
        };

        #[tokio::test]
        async fn test_run_discovery_server_uds() {
            let (discovery_handler_dir, discovery_handler_socket) =
                get_mock_discovery_handler_dir_and_endpoint("protocol.sock");
            let _handle: tokio::task::JoinHandle<()> =
                run_mock_discovery_handler(&discovery_handler_dir, &discovery_handler_socket).await;
            let channel = Endpoint::try_from("lttp://[::]:50051")
                .unwrap()
                .connect_with_connector(tower::service_fn(move |_: Uri| {
                    UnixStream::connect(discovery_handler_socket.clone())
                }))
                .await
                .unwrap();
            let mut discovery_client = DiscoveryClient::new(channel);
            let mut stream = discovery_client
                .discover(Request::new(DiscoverRequest {
                    discovery_details: std::collections::HashMap::new(),
                }))
                .await
                .unwrap()
                .into_inner();
            assert!(stream.message().await.unwrap().unwrap().devices.is_empty());
        }

        // Test when improper socket path or IP address is given as an endpoint
        #[tokio::test]
        async fn test_run_discovery_server_error_invalid_ip_addr() {
            let discovery_handler = MockDiscoveryHandler {};
            let discovery_handler_temp_dir = Builder::new()
                .prefix("discovery-handlers")
                .tempdir()
                .unwrap();
            if let Err(e) = internal_run_discovery_server(
                discovery_handler,
                "random",
                discovery_handler_temp_dir.path().to_str().unwrap(),
            )
            .await
            {
                assert!((*e).to_string().contains("invalid IP address syntax"))
            } else {
                panic!("should be invalid IP address error")
            }
        }
    }
}
