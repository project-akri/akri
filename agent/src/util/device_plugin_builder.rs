use super::{
    constants::{
        DEVICE_PLUGIN_PATH, DEVICE_PLUGIN_SERVER_ENDER_CHANNEL_CAPACITY, K8S_DEVICE_PLUGIN_VERSION,
        KUBELET_SOCKET, LIST_AND_WATCH_MESSAGE_CHANNEL_CAPACITY,
    },
    device_plugin_service::{DevicePluginService, InstanceMap},
    v1beta1,
    v1beta1::{device_plugin_server::DevicePluginServer, registration_client, DevicePluginOptions},
};
use akri_discovery_utils::discovery::v0::Device;
use akri_shared::{
    akri::{configuration::Configuration, AKRI_PREFIX},
    uds::unix_stream,
};
use async_trait::async_trait;
use futures::TryFutureExt;
use log::{info, trace};
#[cfg(test)]
use mockall::{automock, predicate::*};
use std::{convert::TryFrom, env, path::Path, time::SystemTime};
use tokio::{
    net::UnixListener,
    net::UnixStream,
    sync::{broadcast, mpsc},
    task,
};
use tonic::transport::{Endpoint, Server, Uri};
use tower::service_fn;

#[cfg_attr(test, automock)]
#[async_trait]
pub trait DevicePluginBuilderInterface: Send + Sync {
    async fn build_device_plugin(
        &self,
        instance_name: String,
        config: &Configuration,
        shared: bool,
        instance_map: InstanceMap,
        device: Device,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>>;

    async fn serve(
        &self,
        device_plugin_service: DevicePluginService,
        socket_path: String,
        server_ender_receiver: mpsc::Receiver<()>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>>;

    async fn register(
        &self,
        capability_id: &str,
        socket_name: &str,
        instance_name: &str,
        mut server_ender_sender: mpsc::Sender<()>,
        kubelet_socket: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>>;
}

/// For each Instance, builds a Device Plugin, registers it with the kubelet, and serves it over UDS.
pub struct DevicePluginBuilder {}

#[async_trait]
impl DevicePluginBuilderInterface for DevicePluginBuilder {
    /// This creates a new DevicePluginService for an instance and registers it with the kubelet
    async fn build_device_plugin(
        &self,
        instance_name: String,
        config: &Configuration,
        shared: bool,
        instance_map: InstanceMap,
        device: Device,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        info!("build_device_plugin - entered for device {}", instance_name);
        let capability_id: String = format!("{}/{}", AKRI_PREFIX, instance_name);
        let unique_time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?;
        let device_endpoint: String = format!("{}-{}.sock", instance_name, unique_time.as_secs());
        let socket_path: String = Path::new(DEVICE_PLUGIN_PATH)
            .join(device_endpoint.clone())
            .to_str()
            .unwrap()
            .to_string();
        let (list_and_watch_message_sender, _) =
            broadcast::channel(LIST_AND_WATCH_MESSAGE_CHANNEL_CAPACITY);
        let (server_ender_sender, server_ender_receiver) =
            mpsc::channel(DEVICE_PLUGIN_SERVER_ENDER_CHANNEL_CAPACITY);
        let device_plugin_service = DevicePluginService {
            instance_name: instance_name.clone(),
            endpoint: device_endpoint.clone(),
            config: config.spec.clone(),
            config_name: config.metadata.name.clone().unwrap(),
            config_uid: config.metadata.uid.as_ref().unwrap().clone(),
            config_namespace: config.metadata.namespace.as_ref().unwrap().clone(),
            shared,
            node_name: env::var("AGENT_NODE_NAME")?,
            instance_map,
            list_and_watch_message_sender,
            server_ender_sender: server_ender_sender.clone(),
            device,
        };

        self.serve(
            device_plugin_service,
            socket_path.clone(),
            server_ender_receiver,
        )
        .await?;

        self.register(
            &capability_id,
            &device_endpoint,
            &instance_name,
            server_ender_sender,
            KUBELET_SOCKET,
        )
        .await?;

        Ok(())
    }

    // This starts a DevicePluginServer
    async fn serve(
        &self,
        device_plugin_service: DevicePluginService,
        socket_path: String,
        server_ender_receiver: mpsc::Receiver<()>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        info!(
            "serve - creating a device plugin server that will listen at: {}",
            socket_path
        );
        tokio::fs::create_dir_all(Path::new(&socket_path[..]).parent().unwrap())
            .await
            .expect("Failed to create dir at socket path");
        let service = DevicePluginServer::new(device_plugin_service);
        let task_socket_path = socket_path.clone();
        task::spawn(async move {
            let socket_to_delete = task_socket_path.clone();
            let incoming = {
                let uds =
                    UnixListener::bind(task_socket_path).expect("Failed to bind to socket path");

                async_stream::stream! {
                    loop {
                        let item = uds.accept().map_ok(|(st, _)| unix_stream::UnixStream(st)).await;
                        yield item;
                    }
                }
            };
            Server::builder()
                .add_service(service)
                .serve_with_incoming_shutdown(incoming, shutdown_signal(server_ender_receiver))
                .await
                .unwrap();
            trace!(
                "serve - gracefully shutdown ... deleting socket {}",
                socket_to_delete
            );
            // Socket may already be deleted in the case of the kubelet restart
            std::fs::remove_file(socket_to_delete).unwrap_or(());
        });

        akri_shared::uds::unix_stream::try_connect(&socket_path).await?;
        Ok(())
    }

    /// This registers DevicePlugin with the kubelet.
    /// During registration, the device plugin must send
    /// (1) name of unix socket,
    /// (2) Device-Plugin API it was built against (v1beta1),
    /// (3) resource name akri.sh/device_id.
    /// If registration request to the kubelet fails, terminates DevicePluginService.
    async fn register(
        &self,
        capability_id: &str,
        socket_name: &str,
        instance_name: &str,
        server_ender_sender: mpsc::Sender<()>,
        kubelet_socket: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        info!(
            "register - entered for Instance {} and socket_name: {}",
            capability_id, socket_name
        );
        let op = DevicePluginOptions {
            pre_start_required: false,
        };

        // We will ignore this dummy uri because UDS does not use it.
        let kubelet_socket_closure = kubelet_socket.to_string();
        let channel = Endpoint::try_from("http://[::]:50051")?
            .connect_with_connector(service_fn(move |_: Uri| {
                UnixStream::connect(kubelet_socket_closure.clone())
            }))
            .await?;
        let mut registration_client = registration_client::RegistrationClient::new(channel);

        let register_request = tonic::Request::new(v1beta1::RegisterRequest {
            version: K8S_DEVICE_PLUGIN_VERSION.into(),
            endpoint: socket_name.to_string(),
            resource_name: capability_id.to_string(),
            options: Some(op),
        });
        trace!(
            "register - before call to register with the kubelet at socket {}",
            kubelet_socket
        );

        // If fail to register with the kubelet, terminate device plugin
        if registration_client
            .register(register_request)
            .await
            .is_err()
        {
            trace!(
                "register - failed to register Instance {} with the kubelet ... terminating device plugin",
                instance_name
            );
            server_ender_sender.send(()).await?;
        }
        Ok(())
    }
}

/// This acts as a signal future to gracefully shutdown DevicePluginServer upon its completion.
/// Ends when it receives message from `list_and_watch`.
async fn shutdown_signal(mut server_ender_receiver: mpsc::Receiver<()>) {
    match server_ender_receiver.recv().await {
        Some(_) => trace!(
            "shutdown_signal - received signal ... device plugin service gracefully shutting down"
        ),
        None => trace!("shutdown_signal - connection to server_ender_sender closed ... error"),
    }
}

#[cfg(test)]
pub mod tests {
    use super::super::v1beta1::{
        registration_server::{Registration, RegistrationServer},
        Empty, RegisterRequest,
    };
    use super::*;
    use tempfile::Builder;

    struct MockRegistration {
        pub return_error: bool,
    }

    // Mock implementation of kubelet's registration service for tests.
    // Can be configured with its `return_error` field to return an error.
    #[async_trait]
    impl Registration for MockRegistration {
        async fn register(
            &self,
            _request: tonic::Request<RegisterRequest>,
        ) -> Result<tonic::Response<Empty>, tonic::Status> {
            if self.return_error {
                Err(tonic::Status::invalid_argument(
                    "mock discovery handler error",
                ))
            } else {
                Ok(tonic::Response::new(Empty {}))
            }
        }
    }

    async fn serve_for_test<T: Registration, P: AsRef<Path>>(
        service: RegistrationServer<T>,
        socket: P,
    ) {
        let incoming = {
            let uds = UnixListener::bind(socket).expect("Failed to bind to socket path");

            async_stream::stream! {
                loop {
                    let item = uds.accept().map_ok(|(st, _)| unix_stream::UnixStream(st)).await;
                    yield item;
                }
            }
        };

        Server::builder()
            .add_service(service)
            .serve_with_incoming(incoming)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_register() {
        let device_plugins_dirs = Builder::new().prefix("device-plugins").tempdir().unwrap();
        let kubelet_socket = device_plugins_dirs.path().join("kubelet.sock");
        let kubelet_socket_clone = kubelet_socket.clone();
        let kubelet_socket_str = kubelet_socket_clone.to_str().unwrap();

        // Start kubelet registration server
        let registration = MockRegistration {
            return_error: false,
        };
        let service = RegistrationServer::new(registration);
        task::spawn(async move {
            serve_for_test(service, kubelet_socket).await;
        });

        // Make sure registration server has started
        akri_shared::uds::unix_stream::try_connect(kubelet_socket_str)
            .await
            .unwrap();

        let device_plugin_builder = DevicePluginBuilder {};
        let (server_ender_sender, _) = mpsc::channel(1);
        // Test successful registration
        assert!(device_plugin_builder
            .register(
                "random_instance_id",
                "socket.sock",
                "random_instance",
                server_ender_sender,
                kubelet_socket_str
            )
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn test_register_error() {
        let device_plugin_builder = DevicePluginBuilder {};
        let (server_ender_sender, mut server_ender_receiver) = mpsc::channel(1);
        let device_plugins_dirs = Builder::new().prefix("device-plugins").tempdir().unwrap();
        let kubelet_socket = device_plugins_dirs.path().join("kubelet.sock");
        let kubelet_socket_clone = kubelet_socket.clone();
        let kubelet_socket_str = kubelet_socket_clone.to_str().unwrap();

        // Try to register when no registration service exists
        assert!(device_plugin_builder
            .register(
                "random_instance_id",
                "socket.sock",
                "random_instance",
                server_ender_sender.clone(),
                kubelet_socket_str
            )
            .await
            .is_err());

        // Start kubelet registration server
        let registration = MockRegistration { return_error: true };
        let service = RegistrationServer::new(registration);
        task::spawn(async move {
            serve_for_test(service, kubelet_socket).await;
        });

        // Make sure registration server has started
        akri_shared::uds::unix_stream::try_connect(kubelet_socket_str)
            .await
            .unwrap();

        // Test that when registration fails, no error is thrown but the DevicePluginService is signaled to shutdown
        assert!(device_plugin_builder
            .register(
                "random_instance_id",
                "socket.sock",
                "random_instance",
                server_ender_sender,
                kubelet_socket_str
            )
            .await
            .is_ok());
        // Make sure DevicePluginService is signaled to shutdown
        server_ender_receiver.recv().await.unwrap();
    }
}
