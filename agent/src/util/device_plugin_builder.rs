use super::{
    constants::{K8S_DEVICE_PLUGIN_VERSION, KUBELET_SOCKET},
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
use futures::stream::TryStreamExt;
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
        config_name: String,
        config_uid: String,
        config_namespace: String,
        config: Configuration,
        shared: bool,
        instance_map: InstanceMap,
        device_plugin_path: &str,
        device: Device,
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
        config_name: String,
        config_uid: String,
        config_namespace: String,
        config: Configuration,
        shared: bool,
        instance_map: InstanceMap,
        device_plugin_path: &str,
        device: Device,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        info!("build_device_plugin - entered for device {}", instance_name);
        let capability_id: String = format!("{}/{}", AKRI_PREFIX, instance_name);
        let unique_time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?;
        let device_endpoint: String = format!("{}-{}.sock", instance_name, unique_time.as_secs());
        let socket_path: String = Path::new(device_plugin_path)
            .join(device_endpoint.clone())
            .to_str()
            .unwrap()
            .to_string();
        // Channel capacity set to 6 because 3 possible senders (allocate, update_connectivity_status, and handle_config_delete)
        // and and receiver only periodically checks channel
        let (list_and_watch_message_sender, _) = broadcast::channel(6);
        // Channel capacity set to 2 because worst case both register and list_and_watch send messages at same time and receiver is always listening
        let (server_ender_sender, server_ender_receiver) = mpsc::channel(2);
        let device_plugin_service = DevicePluginService {
            instance_name: instance_name.clone(),
            endpoint: device_endpoint.clone(),
            config,
            config_name,
            config_uid,
            config_namespace,
            shared,
            node_name: env::var("AGENT_NODE_NAME")?,
            instance_map,
            list_and_watch_message_sender,
            server_ender_sender: server_ender_sender.clone(),
            device,
        };

        serve(
            device_plugin_service,
            socket_path.clone(),
            server_ender_receiver,
        )
        .await?;

        register(
            capability_id,
            device_endpoint,
            &instance_name,
            server_ender_sender,
        )
        .await?;

        Ok(())
    }
}

// This starts a DevicePluginServer
pub async fn serve(
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
    let mut uds = UnixListener::bind(socket_path.clone()).expect("Failed to bind to socket path");
    let service = DevicePluginServer::new(device_plugin_service);
    let socket_path_to_delete = socket_path.clone();
    task::spawn(async move {
        Server::builder()
            .add_service(service)
            .serve_with_incoming_shutdown(
                uds.incoming().map_ok(unix_stream::UnixStream),
                shutdown_signal(server_ender_receiver),
            )
            .await
            .unwrap();
        trace!(
            "serve - gracefully shutdown ... deleting socket {}",
            socket_path_to_delete
        );
        // Socket may already be deleted in the case of the kubelet restart
        std::fs::remove_file(socket_path_to_delete).unwrap_or(());
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
pub async fn register(
    capability_id: String,
    socket_name: String,
    instance_name: &str,
    mut server_ender_sender: mpsc::Sender<()>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    info!(
        "register - entered for Instance {} and socket_name: {}",
        capability_id, socket_name
    );
    let op = DevicePluginOptions {
        pre_start_required: false,
    };

    // lttp://... is a fake uri that is unused (in service_fn) but necessary for uds connection
    let channel = Endpoint::try_from("lttp://[::]:50051")?
        .connect_with_connector(service_fn(|_: Uri| UnixStream::connect(KUBELET_SOCKET)))
        .await?;
    let mut registration_client = registration_client::RegistrationClient::new(channel);

    let register_request = tonic::Request::new(v1beta1::RegisterRequest {
        version: K8S_DEVICE_PLUGIN_VERSION.into(),
        endpoint: socket_name,
        resource_name: capability_id,
        options: Some(op),
    });
    trace!(
        "register - before call to register with the kubelet at socket {}",
        KUBELET_SOCKET
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
