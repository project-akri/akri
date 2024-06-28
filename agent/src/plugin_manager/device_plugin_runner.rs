use std::{convert::TryFrom, path::Path, sync::Arc, time::SystemTime};

use akri_shared::uds::unix_stream;
use async_trait::async_trait;
use futures::{StreamExt, TryFutureExt};
use thiserror::Error;
use tokio::net::{UnixListener, UnixStream};
use tokio_stream::wrappers::WatchStream;
use tonic::{
    transport::{Endpoint, Server, Uri},
    Request,
};
use tower::service_fn;

/// Current version of the API supported by kubelet.
pub const K8S_DEVICE_PLUGIN_VERSION: &str = "v1beta1";

/// DevicePluginPath is the folder the kubelet expects to find Device-Plugin sockets.
pub const DEVICE_PLUGIN_PATH: &str = "/var/lib/kubelet/device-plugins";

/// Path of the Kubelet registry socket
pub const KUBELET_SOCKET: &str = "/var/lib/kubelet/device-plugins/kubelet.sock";

use super::v1beta1::{
    device_plugin_server::{DevicePlugin, DevicePluginServer},
    registration_client, AllocateRequest, AllocateResponse, DevicePluginOptions, Empty,
    ListAndWatchResponse, RegisterRequest,
};

#[async_trait]
pub(super) trait InternalDevicePlugin: Sync + Send {
    type DeviceStore: Clone + Send + Sync + 'static;
    async fn list_and_watch(
        &self,
    ) -> Result<tonic::Response<DeviceUsageStream<Self::DeviceStore>>, tonic::Status>;
    async fn allocate(
        &self,
        requests: Request<AllocateRequest>,
    ) -> Result<tonic::Response<AllocateResponse>, tonic::Status>;

    fn get_name(&self) -> String;

    async fn stopped(&self);
    fn stop(&self);
}

pub(super) struct DeviceUsageStream<T: Clone + 'static + Send + Sync> {
    pub device_usage_to_device: fn(&str, &str, T) -> Result<ListAndWatchResponse, tonic::Status>,
    pub input_stream: futures::stream::Abortable<WatchStream<T>>,
    pub device_name: String,
    pub node_name: String,
}

impl<T: Clone + 'static + Send + Sync> futures::Stream for DeviceUsageStream<T> {
    type Item = Result<ListAndWatchResponse, tonic::Status>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        match self.input_stream.poll_next_unpin(cx) {
            std::task::Poll::Ready(Some(i)) => std::task::Poll::Ready(Some((self
                .device_usage_to_device)(
                &self.device_name,
                &self.node_name,
                i,
            ))),
            std::task::Poll::Ready(None) => {
                trace!("Stream Stopped");
                std::task::Poll::Ready(None)
            }
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }
}

struct DevicePluginImpl<T: Clone + 'static + Send + Sync> {
    inner: Arc<dyn InternalDevicePlugin<DeviceStore = T>>,
}

#[async_trait]
impl<T: Clone + 'static + Send + Sync> DevicePlugin for DevicePluginImpl<T> {
    async fn get_device_plugin_options(
        &self,
        _request: tonic::Request<Empty>,
    ) -> Result<tonic::Response<DevicePluginOptions>, tonic::Status> {
        Ok(tonic::Response::new(DevicePluginOptions {
            pre_start_required: false,
            get_preferred_allocation_available: false,
        }))
    }

    type ListAndWatchStream = DeviceUsageStream<T>;

    async fn list_and_watch(
        &self,
        _request: tonic::Request<Empty>,
    ) -> Result<tonic::Response<Self::ListAndWatchStream>, tonic::Status> {
        self.inner.list_and_watch().await
    }

    async fn allocate(
        &self,
        requests: Request<AllocateRequest>,
    ) -> Result<tonic::Response<AllocateResponse>, tonic::Status> {
        trace!("kubelet called allocate {:?}", requests);
        self.inner.allocate(requests).await
    }

    async fn pre_start_container(
        &self,
        _request: Request<super::v1beta1::PreStartContainerRequest>,
    ) -> Result<tonic::Response<super::v1beta1::PreStartContainerResponse>, tonic::Status> {
        error!("pre_start_container - kubelet called pre_start_container !",);
        Ok(tonic::Response::new(
            super::v1beta1::PreStartContainerResponse {},
        ))
    }

    async fn get_preferred_allocation(
        &self,
        _request: tonic::Request<super::v1beta1::PreferredAllocationRequest>,
    ) -> Result<tonic::Response<super::v1beta1::PreferredAllocationResponse>, tonic::Status> {
        error!("get_preferred_allocation - kubelet called get_prefered_allocation",);
        Err(tonic::Status::unimplemented(
            "Get preferred allocation is not implemented for this plugin",
        ))
    }
}

#[derive(Error, Debug)]
pub enum RunnerError {
    #[error("Unable to get current time")]
    TimeError,

    #[error("Unable to register plugin to kubelet")]
    RegistrationError,
}

pub(super) async fn serve_and_register_plugin<T: Clone + 'static + Send + Sync>(
    plugin: Arc<dyn InternalDevicePlugin<DeviceStore = T>>,
) -> Result<(), RunnerError> {
    let device_plugin_name = plugin.get_name();
    let plugin_impl = DevicePluginImpl {
        inner: plugin.clone(),
    };

    let unique_time = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_err(|_| RunnerError::TimeError)?;
    let device_endpoint: String = format!("{}-{}.sock", device_plugin_name, unique_time.as_secs());
    let socket_path: String = Path::new(DEVICE_PLUGIN_PATH)
        .join(device_endpoint.clone())
        .to_str()
        .unwrap()
        .to_string();

    info!(
        "serve - creating a device plugin server that will listen at: {}",
        socket_path
    );
    tokio::fs::create_dir_all(Path::new(&socket_path[..]).parent().unwrap())
        .await
        .expect("Failed to create dir at socket path");
    let service = DevicePluginServer::new(plugin_impl);
    let task_socket_path = socket_path.clone();
    let task_plugin = plugin.clone();
    tokio::task::spawn(async move {
        let socket_to_delete = task_socket_path.clone();
        let incoming = {
            let uds = UnixListener::bind(task_socket_path).expect("Failed to bind to socket path");

            async_stream::stream! {
                loop {
                    let item = uds.accept().map_ok(|(st, _)| unix_stream::UnixStream(st)).await;
                    yield item;
                }
            }
        };
        Server::builder()
            .add_service(service)
            .serve_with_incoming_shutdown(incoming, task_plugin.stopped())
            .await
            .unwrap();
        trace!(
            "serve - gracefully shutdown ... deleting socket {}",
            socket_to_delete
        );
        // Socket may already be deleted in the case of the kubelet restart
        std::fs::remove_file(socket_to_delete).unwrap_or(());
    });

    if let Err(e) = register_plugin(device_plugin_name, device_endpoint, socket_path).await {
        plugin.stop();
        return Err(e);
    }
    Ok(())
}

async fn register_plugin(
    device_plugin_name: String,
    device_endpoint: String,
    socket_path: String,
) -> Result<(), RunnerError> {
    let capability_id: String = format!("akri.sh/{}", device_plugin_name);

    akri_shared::uds::unix_stream::try_connect(&socket_path)
        .await
        .map_err(|_| RunnerError::RegistrationError)?;

    info!(
        "register - entered for Instance {} and socket_name: {}",
        capability_id, device_endpoint
    );
    let op = DevicePluginOptions {
        pre_start_required: false,
        get_preferred_allocation_available: false,
    };

    // We will ignore this dummy uri because UDS does not use it.
    // Some servers will check the uri content so the uri needs to
    // be in valid format even it's not used, the scheme part is used
    // to specific what scheme to use, such as http or https
    let kubelet_socket_closure = KUBELET_SOCKET.to_string();
    let channel = Endpoint::try_from("http://[::1]:50051")
        .unwrap()
        .connect_with_connector(service_fn(move |_: Uri| {
            UnixStream::connect(kubelet_socket_closure.clone())
        }))
        .await
        .map_err(|_| RunnerError::RegistrationError)?;
    let mut registration_client = registration_client::RegistrationClient::new(channel);

    let register_request = tonic::Request::new(RegisterRequest {
        version: K8S_DEVICE_PLUGIN_VERSION.into(),
        endpoint: device_endpoint.to_string(),
        resource_name: capability_id.to_string(),
        options: Some(op),
    });
    trace!(
        "register - before call to register with the kubelet at socket {}",
        KUBELET_SOCKET
    );

    // If fail to register with the kubelet, terminate device plugin
    registration_client
        .register(register_request)
        .await
        .map_err(|_| RunnerError::RegistrationError)?;
    Ok(())
}
