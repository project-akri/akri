pub mod v0;

/// Path of the Agent registration socket
pub const AGENT_REGISTRATION_SOCKET: &str = "/var/lib/akri/agent-registration.sock";

/// Folder in which the Agent expects to find discovery handler sockets.
pub const DISCOVERY_HANDLER_PATH: &str = "/var/lib/akri";

/// Definition of the DiscoverStream type expected for supported embedded Akri DiscoveryHandlers
pub type DiscoverStream = tokio::sync::mpsc::Receiver<Result<v0::DiscoverResponse, tonic::Status>>;

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
        shutdown_receiver: tokio::sync::mpsc::Receiver<()>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        info!("run_discovery_server - entered");

        if discovery_endpoint.starts_with(super::DISCOVERY_HANDLER_PATH) {
            tokio::fs::create_dir_all(Path::new(&discovery_endpoint[..]).parent().unwrap())
                .await
                .expect("Failed to create dir at socket path");
            // Delete socket if it already exists
            std::fs::remove_file(discovery_endpoint).unwrap_or(());
            let mut uds =
                UnixListener::bind(discovery_endpoint).expect("Failed to bind to socket path");
            Server::builder()
                .add_service(DiscoveryServer::new(discovery_handler))
                .serve_with_incoming_shutdown(
                    uds.incoming().map_ok(unix_stream::UnixStream),
                    shutdown_signal(shutdown_receiver),
                )
                .await?;
        } else {
            let addr = discovery_endpoint.parse()?;
            Server::builder()
                .add_service(DiscoveryServer::new(discovery_handler))
                .serve_with_shutdown(addr, shutdown_signal(shutdown_receiver))
                .await?;
        }
        info!("run_discovery_server - finished");
        Ok(())
    }

    /// This acts as a signal future to gracefully shutdown Discovery Handlers.
    async fn shutdown_signal(mut server_ender_receiver: tokio::sync::mpsc::Receiver<()>) {
        match server_ender_receiver.recv().await {
            Some(_) => info!(
                "shutdown_signal - received signal ... discovery handler gracefully shutting down"
            ),
            None => info!("shutdown_signal - connection to server_ender_sender closed ... error"),
        }
    }
}
