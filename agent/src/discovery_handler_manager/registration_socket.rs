use std::{convert::TryFrom, pin::Pin, sync::Arc};

use akri_discovery_utils::discovery::v0::{
    DiscoverRequest, DiscoverResponse, Empty, RegisterDiscoveryHandlerRequest,
    discovery_handler_client::DiscoveryHandlerClient,
    register_discovery_handler_request::EndpointType, registration_server::Registration,
};
use akri_shared::uds::unix_stream;
use async_trait::async_trait;
use futures::{Stream, StreamExt, TryFutureExt};
use tokio::{select, sync::watch};
use tokio_stream::StreamExt as _;
use tonic::{Request, Response, Status, transport::Channel};

use crate::util::stopper::Stopper;

use super::{
    DiscoveryError,
    discovery_handler_registry::{
        DiscoveredDevice, DiscoveryHandlerEndpoint, DiscoveryHandlerRegistry,
    },
};

struct NetworkEndpoint {
    name: String,
    endpoint: String,
    endpoint_type: EndpointType,
    stopped: Stopper,
    shared: bool,
    node_name: String,
}

impl NetworkEndpoint {
    fn new(req: RegisterDiscoveryHandlerRequest, node_name: String) -> Self {
        NetworkEndpoint {
            name: req.name,
            endpoint: req.endpoint,
            stopped: Stopper::new(),
            shared: req.shared,
            endpoint_type: EndpointType::try_from(req.endpoint_type).unwrap(),
            node_name,
        }
    }

    async fn get_client(&self) -> Result<DiscoveryHandlerClient<Channel>, tonic::transport::Error> {
        match self.endpoint_type {
            EndpointType::Uds => {
                let socket = self.endpoint.clone();
                Ok(DiscoveryHandlerClient::new(
                    tonic::transport::Endpoint::try_from("http://[::1]:50051")
                        .unwrap()
                        .connect_with_connector(tower::service_fn(move |_: hyper::Uri| {
                            tokio::net::UnixStream::connect(socket.clone())
                        }))
                        .await?,
                ))
            }
            EndpointType::Network => DiscoveryHandlerClient::connect(self.endpoint.clone()).await,
        }
    }

    async fn handle_stream(
        stopper: Stopper,
        uid: String,
        node_name: String,
        shared: bool,
        sender: watch::Sender<Vec<Arc<DiscoveredDevice>>>,
        mut stream: Pin<Box<dyn Stream<Item = Result<DiscoverResponse, tonic::Status>> + Send>>,
    ) {
        loop {
            let msg = select! {
                // This means all queries for this endpoint must end.
                _ = stopper.stopped() => return,
                // This means all receiver dropped (i.e no one cares about this query anymore)
                _ = sender.closed() => return,
                msg = stream.try_next() => match msg {
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
            trace!("Received new message from discovery handler: {:?}", msg);
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
impl DiscoveryHandlerEndpoint for NetworkEndpoint {
    async fn query(
        &self,
        sender: watch::Sender<Vec<Arc<DiscoveredDevice>>>,
        query_body: DiscoverRequest,
    ) -> Result<(), DiscoveryError> {
        if self.stopped.is_stopped() {
            return Err(DiscoveryError::UnavailableDiscoveryHandler(self.get_uid()));
        }
        let stream = match self.get_client().await {
            Ok(mut discovery_handler_client) => {
                trace!(
                    "NetworkEndpoint::query - connecting to external {} discovery handler over network",
                    self.name
                );
                match discovery_handler_client.discover(query_body).await {
                    Ok(device_update_receiver) => device_update_receiver.into_inner(),
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
                                return Err(DiscoveryError::UnavailableDiscoveryHandler(
                                    self.get_uid(),
                                ));
                            }
                        }
                    }
                }
            }
            Err(e) => {
                error!(
                    "NetworkEndpoint::query - failed to connect to {} discovery handler over network with error {}",
                    self.name, e
                );
                // We failed to connect to Discovery Handler, consider it offline now
                self.stopped.stop();
                return Err(DiscoveryError::UnavailableDiscoveryHandler(self.get_uid()));
            }
        };
        tokio::spawn(Self::handle_stream(
            self.stopped.to_owned(),
            self.get_uid(),
            self.node_name.to_owned(),
            self.shared.to_owned(),
            sender,
            stream.boxed(),
        ));
        Ok(())
    }

    fn get_name(&self) -> String {
        self.name.to_owned()
    }
    fn get_uid(&self) -> String {
        format!("{}@{}", self.name, self.endpoint)
    }

    async fn closed(&self) {
        self.stopped.stopped().await
    }
    fn is_closed(&self) -> bool {
        self.stopped.is_stopped()
    }
}

struct RegistrationEndpoint {
    inner: Arc<dyn DiscoveryHandlerRegistry>,
    node_name: String,
}
#[async_trait]
impl Registration for RegistrationEndpoint {
    async fn register_discovery_handler(
        &self,
        request: Request<RegisterDiscoveryHandlerRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        self.inner
            .register_endpoint(Arc::new(NetworkEndpoint::new(req, self.node_name.clone())))
            .await;
        Ok(Response::new(Empty {}))
    }
}

pub async fn run_registration_server(
    dh_registry: Arc<dyn DiscoveryHandlerRegistry>,
    socket_path: &str,
    node_name: String,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("internal_run_registration_server - entered");
    trace!(
        "internal_run_registration_server - registration server listening on socket {}",
        socket_path
    );

    #[cfg(any(test, feature = "agent-full"))]
    super::embedded_handler::register_handlers(dh_registry.as_ref(), node_name.clone()).await;
    // Delete socket in case previously created/used
    std::fs::remove_file(socket_path).unwrap_or(());
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
    tonic::transport::Server::builder()
        .add_service(
            akri_discovery_utils::discovery::v0::registration_server::RegistrationServer::new(
                RegistrationEndpoint {
                    inner: dh_registry,
                    node_name,
                },
            ),
        )
        .serve_with_incoming(incoming)
        .await?;
    trace!(
        "internal_run_registration_server - gracefully shutdown ... deleting socket {}",
        socket_path
    );
    std::fs::remove_file(socket_path).unwrap_or(());
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use akri_discovery_utils::discovery::v0::Device;
    use tokio::sync::mpsc;

    use super::*;

    #[tokio::test]
    async fn test_handle_stream_local() {
        let stopper = Stopper::new();
        let uid = "foo".to_owned();
        let node_name = "node-a".to_owned();
        let shared = false;
        let (sender, mut receiver) = watch::channel(Default::default());
        let (st_sender, st_rec) = mpsc::channel(1);
        let stream = tokio_stream::wrappers::ReceiverStream::new(st_rec);

        let task = tokio::spawn(NetworkEndpoint::handle_stream(
            stopper,
            uid,
            node_name.clone(),
            shared,
            sender,
            stream.boxed(),
        ));
        assert!(
            st_sender
                .send(Ok(DiscoverResponse {
                    devices: vec![Device {
                        id: "bar".to_string(),
                        ..Default::default()
                    }]
                }))
                .await
                .is_ok()
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(500), receiver.changed())
                .await
                .is_ok()
        );
        let val = receiver.borrow_and_update().clone();
        assert_eq!(
            val,
            vec![Arc::new(DiscoveredDevice::LocalDevice(
                Device {
                    id: "bar".to_string(),
                    ..Default::default()
                },
                node_name.to_owned()
            ))]
        );

        drop(receiver);
        assert!(
            tokio::time::timeout(Duration::from_millis(500), task)
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn test_handle_stream_shared() {
        let stopper = Stopper::new();
        let uid = "foo".to_owned();
        let node_name = "node-a".to_owned();
        let shared = true;
        let (sender, mut receiver) = watch::channel(Default::default());
        let (st_sender, st_rec) = mpsc::channel(1);
        let stream = tokio_stream::wrappers::ReceiverStream::new(st_rec);

        let task = tokio::spawn(NetworkEndpoint::handle_stream(
            stopper.clone(),
            uid,
            node_name.clone(),
            shared,
            sender,
            stream.boxed(),
        ));
        assert!(
            st_sender
                .send(Ok(DiscoverResponse {
                    devices: vec![Device {
                        id: "bar".to_string(),
                        ..Default::default()
                    }]
                }))
                .await
                .is_ok()
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(500), receiver.changed())
                .await
                .is_ok()
        );
        let val = receiver.borrow_and_update().clone();
        assert_eq!(
            val,
            vec![Arc::new(DiscoveredDevice::SharedDevice(Device {
                id: "bar".to_string(),
                ..Default::default()
            }))]
        );

        stopper.stop();
        assert!(
            tokio::time::timeout(Duration::from_millis(500), task)
                .await
                .is_ok()
        );
    }
}
