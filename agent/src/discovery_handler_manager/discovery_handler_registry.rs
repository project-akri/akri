//! This module is the heart of the discovery process handled by the agent, it is based around the [DiscoveryHandlerRegistry]
//! and uses several other structure to represent and help handle discovery related operations.
//!
//! The [DiscoveryHandlerRegistry] keeps track of registered discovery handlers. Note, multiple endpoints/instances of a given
//! handler can be registered at the same time.
//!
//! The [DiscoveryHandlerRegistry] also keeps track of ongoing discovery requests against those discovery handlers. There is one discovery request (a [DiscoveryHandlerRequest] object) per Configuration.
//!   
//! Here are some simple diagrams showing how the components interact with each other in different situations:
//!   
//! A new DiscoverHandler gets registered (after it connects to and registers with the agent registration Unix socket):
#![doc=simple_mermaid::mermaid!("diagrams/dh_registration.mmd")]
//!
//! A new query is made by the Configuration Controller:
#![doc=simple_mermaid::mermaid!("diagrams/dh_query.mmd")]
//!
//! A Discovery Handler's instance/endpoint sends a new list of discovered devices for a Request:
#![doc=simple_mermaid::mermaid!("diagrams/dh_device.mmd")]

use std::collections::HashMap;
use std::sync::Arc;

use akri_discovery_utils::discovery::v0::{ByteData, Device, DiscoverRequest};
use akri_shared::akri::configuration::{Configuration, DiscoveryProperty};
use akri_shared::akri::instance::Instance;

use akri_shared::akri::instance::InstanceSpec;
use akri_shared::akri::AKRI_PREFIX;
use async_trait::async_trait;
use blake2::digest::{Update, VariableOutput};
use blake2::VarBlake2b;
use futures::future::select_all;
use futures::future::try_join_all;
use futures::FutureExt;
use itertools::Itertools;
use kube::core::ObjectMeta;
use kube::runtime::reflector::ObjectRef;
use tokio::select;
use tokio::sync::mpsc;
use tokio::sync::watch;
use tokio::sync::RwLock;
use tokio::sync::{broadcast, Mutex, Notify};

use super::discovery_property_solver::PropertySolver;
use super::{DiscoveryError, DiscoveryManagerKubeInterface};
use crate::device_manager::cdi::ContainerEdit;

#[cfg(test)]
use mockall::automock;

#[derive(Clone, Debug, PartialEq)]
pub enum DiscoveredDevice {
    LocalDevice(Device, String),
    SharedDevice(Device),
}

impl DiscoveredDevice {
    /// Generates a digest of an Instance's id. There should be a unique digest and Instance for each discovered device.
    /// This means that the id of non-local devices that could be visible to multiple nodes should always resolve
    /// to the same instance name (which is suffixed with this digest).
    /// However, local devices' Instances should have unique hashes even if they have the same id.
    /// To ensure this, the node's name is added to the id before it is hashed.
    fn device_hash(&self) -> String {
        let (id_to_digest, shared, node_name) = match self {
            DiscoveredDevice::LocalDevice(d, n) => (d.id.to_owned(), false, n.as_str()),
            DiscoveredDevice::SharedDevice(d) => (d.id.to_owned(), true, ""),
        };
        let mut id_to_digest = id_to_digest.to_string();
        // For local devices, include node hostname in id_to_digest so instances have unique names
        if !shared {
            id_to_digest = format!("{}{}", id_to_digest, node_name);
        }
        let mut digest = String::new();
        let mut hasher = VarBlake2b::new(3).unwrap();
        hasher.update(id_to_digest);
        hasher.finalize_variable(|var| {
            digest = var
                .iter()
                .map(|num| format!("{:02x}", num))
                .collect::<Vec<String>>()
                .join("")
        });
        digest
    }

    fn inner(self) -> Device {
        match self {
            DiscoveredDevice::LocalDevice(d, _) => d,
            DiscoveredDevice::SharedDevice(d) => d,
        }
    }
}

impl From<DiscoveredDevice> for crate::device_manager::cdi::Device {
    fn from(value: DiscoveredDevice) -> Self {
        let hash = value.device_hash();
        let dev = value.inner();
        Self {
            name: hash,
            annotations: Default::default(),
            container_edits: crate::device_manager::cdi::ContainerEdit {
                env: dev
                    .properties
                    .into_iter()
                    .map(|(k, v)| format!("{}={}", k, v))
                    .collect(),
                device_nodes: dev.device_specs.into_iter().map_into().collect(),
                mounts: dev.mounts.into_iter().map_into().collect(),
                hooks: Default::default(),
            },
        }
    }
}

/// This trait represents a discovery handler, no matter if it is an embedded or remote one
#[async_trait]
#[cfg_attr(test, automock)]
pub trait DiscoveryHandlerEndpoint: Send + Sync {
    async fn query(
        &self,
        sender: watch::Sender<Vec<Arc<DiscoveredDevice>>>,
        query_body: DiscoverRequest,
    ) -> Result<(), DiscoveryError>;

    fn get_name(&self) -> String;
    fn get_uid(&self) -> String;

    async fn closed(&self);
    fn is_closed(&self) -> bool;
}

/// This trait is here to help with testing for code that interract with the discovery handler registry.
/// This trait represent a request made to a DH (either locally or through gRPC call), it will aggregate the
/// results across the different registered handlers of that type, and generate the Instance objects for discovered
/// devices.
#[cfg_attr(test, automock)]
#[async_trait]
pub trait DiscoveryHandlerRequest: Sync + Send {
    async fn get_instances(&self) -> Result<Vec<Instance>, DiscoveryError>;
    async fn set_extra_device_properties(&self, extra_device_properties: HashMap<String, String>);
}

/// This trait is here to help with testing for code that interract with the discovery handler registry
/// In the context of this trait, a "request" is a DiscoveryHandlerRequest,
#[cfg_attr(test, automock)]
#[async_trait]
pub trait DiscoveryHandlerRegistry: Sync + Send {
    /// Create a new request against a specific Discovery Handler type, the DH Registry will ensure it
    /// gets sent to all registered handlers with this name, present and future, if no DH with that name
    /// is registered, returns an error.
    async fn new_request(
        &self,
        key: &str,
        dh_name: &str,
        dh_details: &str,
        dh_properties: &[DiscoveryProperty],
        extra_device_properties: HashMap<String, String>,
        namespace: &str,
    ) -> Result<(), DiscoveryError>;

    /// Get a reference to a specific request, allowing one to get the related Instances
    async fn get_request(&self, key: &str) -> Option<Arc<dyn DiscoveryHandlerRequest>>;

    /// Terminate a specific request, will trigger removal of linked devices
    async fn terminate_request(&self, key: &str);

    /// Register a new endpoint to make it available to all current and future queries
    async fn register_endpoint(&self, endpoint: Arc<dyn DiscoveryHandlerEndpoint>);
}

/// Real world implementation of the Discovery Handler Request
struct DHRequestImpl {
    endpoints: RwLock<Vec<watch::Receiver<Vec<Arc<DiscoveredDevice>>>>>,
    notifier: watch::Sender<crate::device_manager::cdi::Kind>,
    key: String,
    handler_name: String,
    details: String,
    properties: Vec<DiscoveryProperty>,
    extra_device_properties: RwLock<HashMap<String, String>>,
    kube_client: Arc<dyn DiscoveryManagerKubeInterface>,
    termination_notifier: Arc<Notify>,
}

#[async_trait]
impl DiscoveryHandlerRequest for DHRequestImpl {
    async fn get_instances(&self) -> Result<Vec<Instance>, DiscoveryError> {
        let properties = self.extra_device_properties.read().await;
        Ok(self
            .endpoints
            .read()
            .await
            .iter()
            .flat_map(|r| r.borrow().clone().into_iter())
            .map(|i| self.device_to_instance(i.as_ref(), &properties))
            .collect())
    }

    async fn set_extra_device_properties(&self, extra_device_properties: HashMap<String, String>) {
        let mut current = self.extra_device_properties.write().await;
        if extra_device_properties != *current {
            let edit = extra_device_properties
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect();
            *current = extra_device_properties;
            self.notifier
                .send_modify(|k| k.container_edits.first_mut().unwrap().env = edit);
        }
    }
}

impl DHRequestImpl {
    fn device_to_instance(
        &self,
        dev: &DiscoveredDevice,
        extra_device_properties: &HashMap<String, String>,
    ) -> Instance {
        let (rdev, shared) = match dev {
            DiscoveredDevice::LocalDevice(d, _) => (d, false),
            DiscoveredDevice::SharedDevice(d) => (d, true),
        };
        let mut properties = rdev.properties.clone();
        properties.extend(
            extra_device_properties
                .iter()
                .map(|(k, v)| (k.clone(), v.clone())),
        );
        Instance {
            spec: InstanceSpec {
                cdi_name: self.get_device_cdi_fqdn(dev),
                configuration_name: self.key.clone(),
                broker_properties: properties,
                shared,
                nodes: Default::default(),
                device_usage: Default::default(),
                capacity: Default::default(),
            },
            metadata: ObjectMeta {
                name: Some(format!("{}-{}", self.key, dev.device_hash())),
                ..Default::default()
            },
        }
    }

    fn get_device_cdi_fqdn(&self, dev: &DiscoveredDevice) -> String {
        format!("{}/{}={}", AKRI_PREFIX, self.key, dev.device_hash())
    }

    async fn watch_devices(
        &self,
        mut new_dh_receiver: broadcast::Receiver<Arc<dyn DiscoveryHandlerEndpoint>>,
    ) {
        loop {
            let mut local_endpoints = self.endpoints.write().await.clone();
            let futures = local_endpoints.iter_mut().map(|e| e.changed().boxed());
            select! {
                (a, index, _) = select_all(futures) => {
                    if a.is_err() {
                        let mut write_endpoint = self.endpoints.write().await;
                        write_endpoint.remove(index);
                        if write_endpoint.is_empty() {
                            return;
                        }
                    }
                },
                Ok(new_dh_endpoint) = new_dh_receiver.recv() => {
                    if new_dh_endpoint.get_name() != self.handler_name {
                        // We woke up for another kind of DH, let's get back to sleep
                        continue
                    }
                    if let Ok(q) = self.query(new_dh_endpoint).await {
                        self.endpoints.write().await.push(q);
                    }
                },
                _ = self.notifier.closed() => {
                    return;
                },
            }
            let devices: Vec<Arc<DiscoveredDevice>> = self
                .endpoints
                .write()
                .await
                .iter_mut()
                .flat_map(|r| r.borrow_and_update().clone().into_iter())
                .unique_by(|d| self.get_device_cdi_fqdn(d))
                .collect();
            self.notifier
                .send_replace(crate::device_manager::cdi::Kind {
                    kind: format!("{}/{}", AKRI_PREFIX, self.key),
                    annotations: Default::default(),
                    devices: devices
                        .into_iter()
                        .map(|d| d.as_ref().clone().into())
                        .collect(),
                    container_edits: vec![ContainerEdit {
                        env: self
                            .extra_device_properties
                            .read()
                            .await
                            .iter()
                            .map(|(k, v)| format!("{}={}", k, v))
                            .collect(),
                        ..Default::default()
                    }],
                });
        }
    }

    async fn query(
        &self,
        discovery_handler: Arc<dyn DiscoveryHandlerEndpoint>,
    ) -> Result<watch::Receiver<Vec<Arc<DiscoveredDevice>>>, DiscoveryError> {
        let (q_sender, q_receiver) = watch::channel(vec![]);
        let query_body = DiscoverRequest {
            discovery_details: self.details.clone(),
            discovery_properties: self.solve_discovery_properties().await?,
        };
        discovery_handler.query(q_sender, query_body).await?;
        Ok(q_receiver)
    }

    async fn solve_discovery_properties(
        &self,
    ) -> Result<HashMap<String, ByteData>, DiscoveryError> {
        let solved_properties_futures = self
            .properties
            .iter()
            .map(|p| p.solve(self.kube_client.clone()));
        Ok(try_join_all(solved_properties_futures)
            .await?
            .into_iter()
            .flatten()
            .collect())
    }
}

pub(super) type LockedMap<T> = Arc<RwLock<HashMap<String, T>>>;

pub(super) struct DHRegistryImpl {
    requests: LockedMap<Arc<DHRequestImpl>>,
    handlers: LockedMap<HashMap<String, Arc<dyn DiscoveryHandlerEndpoint>>>,
    endpoint_notifier: broadcast::Sender<Arc<dyn DiscoveryHandlerEndpoint>>,
    configuration_notifier: mpsc::Sender<ObjectRef<Configuration>>,
    cdi_notifier: Arc<Mutex<watch::Sender<HashMap<String, crate::device_manager::cdi::Kind>>>>,
    kube_client: Arc<dyn DiscoveryManagerKubeInterface>,
}

impl DHRegistryImpl {
    pub(super) fn new(
        kube_client: Arc<dyn DiscoveryManagerKubeInterface>,
        cdi_notifier: watch::Sender<HashMap<String, crate::device_manager::cdi::Kind>>,
        configuration_notifier: mpsc::Sender<ObjectRef<Configuration>>,
    ) -> Self {
        let (endpoint_notifier, _) = broadcast::channel(10);

        Self {
            requests: Default::default(),
            handlers: Default::default(),
            endpoint_notifier,
            configuration_notifier,
            cdi_notifier: Arc::new(Mutex::new(cdi_notifier)),
            kube_client,
        }
    }
}

async fn handle_request(
    mut req_notifier: watch::Receiver<crate::device_manager::cdi::Kind>,
    key: &String,
    namespace: &String,
    cdi_sender: Arc<Mutex<watch::Sender<HashMap<String, crate::device_manager::cdi::Kind>>>>,
    local_config_sender: mpsc::Sender<ObjectRef<Configuration>>,
) {
    let cdi_kind = format!("{}/{}", AKRI_PREFIX, key);
    loop {
        match req_notifier.changed().await {
            Ok(_) => {
                let kind = req_notifier.borrow_and_update().clone();
                cdi_sender.lock().await.send_modify(|kinds| {
                    kinds.insert(cdi_kind.clone(), kind);
                });
                trace!("Ask for reconciliation of {}::{}", namespace, key);
                let res = local_config_sender
                    .send(ObjectRef::<Configuration>::new(key).within(namespace))
                    .await;
                if res.is_err() {
                    cdi_sender.lock().await.send_modify(|kind| {
                        kind.remove(&cdi_kind);
                    });
                    return;
                }
            }
            Err(_) => {
                trace!("Ask for reconciliation of {}::{}", namespace, key);
                let _ = local_config_sender
                    .send(ObjectRef::<Configuration>::new(key).within(namespace))
                    .await;
                cdi_sender.lock().await.send_modify(|kind| {
                    kind.remove(&cdi_kind);
                });
                return;
            }
        }
    }
}

#[async_trait]
impl DiscoveryHandlerRegistry for DHRegistryImpl {
    async fn new_request(
        &self,
        key: &str,
        dh_name: &str,
        dh_details: &str,
        dh_properties: &[DiscoveryProperty],
        extra_device_properties: HashMap<String, String>,
        namespace: &str,
    ) -> Result<(), DiscoveryError> {
        match self.handlers.read().await.get(dh_name) {
            Some(handlers) => {
                let (notifier, _) = watch::channel(Default::default());
                let terminated = Arc::new(Notify::new());
                let mut dh_req = DHRequestImpl {
                    endpoints: Default::default(),
                    notifier,
                    key: key.to_string(),
                    handler_name: dh_name.to_string(),
                    details: dh_details.to_string(),
                    properties: dh_properties.to_vec(),
                    extra_device_properties: RwLock::new(extra_device_properties),
                    kube_client: self.kube_client.clone(),
                    termination_notifier: terminated.clone(),
                };
                let dh_futures = handlers
                    .iter()
                    .map(|(_, handler)| dh_req.query(handler.clone()));
                let dh_streams: Vec<watch::Receiver<Vec<Arc<DiscoveredDevice>>>> =
                    try_join_all(dh_futures).await?;
                dh_req.endpoints = RwLock::new(dh_streams);
                {
                    let mut req_w = self.requests.write().await;
                    req_w.insert(key.to_string(), Arc::new(dh_req));
                }
                let dh_req_ref = self.requests.read().await.get(key).unwrap().to_owned();
                let local_req_notifier = self
                    .requests
                    .read()
                    .await
                    .get(key)
                    .unwrap()
                    .notifier
                    .subscribe();
                let local_config_sender = self.configuration_notifier.to_owned();
                let local_cdi_sender = self.cdi_notifier.to_owned();
                let local_key = key.to_owned();
                let namespace = namespace.to_owned();
                tokio::spawn(async move {
                    handle_request(
                        local_req_notifier,
                        &local_key,
                        &namespace,
                        local_cdi_sender,
                        local_config_sender,
                    )
                    .await
                });

                let local_key = key.to_owned();
                let notifier_receiver = self.endpoint_notifier.subscribe();
                let local_req = self.requests.clone();
                tokio::spawn(async move {
                    select! {
                        _ = dh_req_ref
                        .watch_devices(notifier_receiver) => {},
                        _ = terminated.notified() => {},
                    }
                    local_req.write().await.remove(&local_key);
                });
                Ok(())
            }
            None => Err(DiscoveryError::NoHandler(dh_name.to_string())),
        }
    }

    async fn get_request(&self, key: &str) -> Option<Arc<dyn DiscoveryHandlerRequest>> {
        let req_read = self.requests.read().await;
        match req_read.get(key) {
            Some(r) => Some(r.to_owned()),
            None => None,
        }
    }

    async fn terminate_request(&self, key: &str) {
        if let Some(r) = self.requests.write().await.remove(key) {
            r.termination_notifier.notify_waiters()
        }
    }

    async fn register_endpoint(&self, endpoint: Arc<dyn DiscoveryHandlerEndpoint>) {
        let name = endpoint.get_name();
        let uid = endpoint.get_uid();
        let _ = self.endpoint_notifier.send(endpoint.clone());
        {
            let mut w_handlers = self.handlers.write().await;
            match w_handlers.get_mut(&name) {
                Some(v) => {
                    v.insert(uid.clone(), endpoint.clone());
                }
                None => {
                    w_handlers.insert(
                        name.clone(),
                        HashMap::from([(uid.clone(), endpoint.clone())]),
                    );
                }
            }
        }
        // Spawn a task to remove it from the list when it gets closed. It is the responsibility of the
        // endpoint to close itself when it cannot accept new requests, it is ok for the endpoint to do so
        // reactively after a failure on a new request.
        let local_handlers = self.handlers.clone();
        tokio::spawn(async move {
            endpoint.closed().await;
            let mut w_handlers = local_handlers.write().await;
            if let Some(v) = w_handlers.get_mut(&name) {
                // Remove all closed endpoints, we can't remove just the one with our uid, as it
                // may have registered again in the meantime.
                v.retain(|_, e| !e.is_closed());
                if v.is_empty() {
                    w_handlers.remove(&name);
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use crate::{
        device_manager::cdi::{self, Kind},
        discovery_handler_manager::mock::MockDiscoveryManagerKubeInterface,
    };
    use akri_discovery_utils::discovery::v0 as discovery_utils;

    use super::*;

    #[test]
    fn test_discovered_device() {
        let local_device = DiscoveredDevice::LocalDevice(
            Device {
                id: "my_local_device".to_owned(),
                properties: Default::default(),
                mounts: Default::default(),
                device_specs: Default::default(),
            },
            "my_node".to_owned(),
        );
        let other_local_device = DiscoveredDevice::LocalDevice(
            Device {
                id: "my_local_device".to_owned(),
                properties: Default::default(),
                mounts: Default::default(),
                device_specs: Default::default(),
            },
            "my_other_node".to_owned(),
        );
        let shared_device = DiscoveredDevice::SharedDevice(Device {
            id: "my_shared_device".to_owned(),
            properties: HashMap::from([("ENV_KEY".to_owned(), "env_value".to_owned())]),
            mounts: vec![discovery_utils::Mount {
                container_path: "container".to_owned(),
                host_path: "host".to_owned(),
                read_only: false,
            }],
            device_specs: vec![discovery_utils::DeviceSpec {
                container_path: "container".to_owned(),
                host_path: "host".to_owned(),
                permissions: "perms".to_owned(),
            }],
        });

        assert_eq!(
            Into::<cdi::Device>::into(local_device),
            cdi::Device {
                name: "e77db4".to_owned(),
                annotations: Default::default(),
                container_edits: ContainerEdit {
                    env: vec![],
                    device_nodes: vec![],
                    mounts: vec![],
                    hooks: Default::default()
                },
            }
        );
        assert_eq!(
            Into::<cdi::Device>::into(other_local_device),
            cdi::Device {
                name: "099763".to_owned(),
                annotations: Default::default(),
                container_edits: ContainerEdit {
                    env: vec![],
                    device_nodes: vec![],
                    mounts: vec![],
                    hooks: Default::default()
                },
            }
        );
        assert_eq!(
            Into::<cdi::Device>::into(shared_device),
            cdi::Device {
                name: "4294ea".to_owned(),
                annotations: Default::default(),
                container_edits: ContainerEdit {
                    env: vec!["ENV_KEY=env_value".to_owned()],
                    device_nodes: vec![cdi::DeviceNode {
                        path: "container".to_owned(),
                        host_path: Some("host".to_owned()),
                        permissions: Some("perms".to_owned()),
                        ..Default::default()
                    }],
                    mounts: vec![cdi::Mount {
                        host_path: "host".to_owned(),
                        container_path: "container".to_owned(),
                        mount_type: None,
                        options: Default::default()
                    }],
                    hooks: Default::default()
                },
            }
        );
    }

    #[tokio::test]
    async fn test_dh_request_impl_get_instances() {
        let (_, notifier) = watch::channel(vec![Arc::new(DiscoveredDevice::LocalDevice(
            Device {
                id: "my_local_device".to_owned(),
                properties: HashMap::from([(
                    "MY_DEVICE_KEY".to_owned(),
                    "device_value".to_owned(),
                )]),
                mounts: Default::default(),
                device_specs: Default::default(),
            },
            "my_node".to_owned(),
        ))]);
        let endpoints = RwLock::new(vec![notifier]);
        let (cdi_notifier, _) = watch::channel(Default::default());
        let req = DHRequestImpl {
            endpoints,
            notifier: cdi_notifier,
            key: "my_config".to_owned(),
            handler_name: "mock_handler".to_string(),
            details: Default::default(),
            properties: Default::default(),
            extra_device_properties: RwLock::new(HashMap::from([(
                "MY_EXTRA_KEY".to_owned(),
                "value".to_owned(),
            )])),
            kube_client: Arc::new(MockDiscoveryManagerKubeInterface::new()),
            termination_notifier: Arc::new(Notify::new()),
        };

        assert_eq!(
            req.get_instances().await.unwrap(),
            vec![Instance {
                metadata: ObjectMeta {
                    name: Some("my_config-e77db4".to_owned()),
                    ..Default::default()
                },
                spec: InstanceSpec {
                    configuration_name: "my_config".to_owned(),
                    cdi_name: "akri.sh/my_config=e77db4".to_owned(),
                    capacity: 0,
                    broker_properties: HashMap::from([
                        ("MY_EXTRA_KEY".to_owned(), "value".to_owned()),
                        ("MY_DEVICE_KEY".to_owned(), "device_value".to_owned())
                    ]),
                    shared: false,
                    nodes: Default::default(),
                    device_usage: Default::default(),
                }
            }]
        );
    }

    #[tokio::test]
    async fn test_dh_request_impl_watch_devices() {
        let (notifier, mut n_rec) = watch::channel(Default::default());
        let (dh_send, dh_rec) = watch::channel(Default::default());
        let req = Arc::new(DHRequestImpl {
            endpoints: RwLock::new(vec![dh_rec]),
            notifier,
            key: "my_config".to_owned(),
            handler_name: "mock_handler".to_string(),
            details: "discovery details".to_string(),
            properties: vec![DiscoveryProperty {
                name: "property_1".to_string(),
                value: Some("value_1".to_string()),
                value_from: None,
            }],
            extra_device_properties: RwLock::new(HashMap::from([(
                "MY_EXTRA_KEY".to_owned(),
                "value".to_owned(),
            )])),
            kube_client: Arc::new(MockDiscoveryManagerKubeInterface::new()),
            termination_notifier: Arc::new(Notify::new()),
        });
        let req_ref = req.clone();

        let (new_dh_sen, rec) = broadcast::channel(1);

        let task = tokio::spawn(async move { req_ref.watch_devices(rec).await });
        assert!(n_rec.borrow_and_update().devices.is_empty());

        let new_device = Arc::new(DiscoveredDevice::SharedDevice(Device {
            id: "my_shared_device".to_owned(),
            properties: HashMap::from([("ENV_KEY".to_owned(), "env_value".to_owned())]),
            mounts: vec![],
            device_specs: vec![],
        }));
        dh_send.send(vec![new_device.clone()]).unwrap();

        tokio::time::sleep(Duration::from_millis(500)).await;
        assert_eq!(
            n_rec.borrow_and_update().devices.clone(),
            vec![new_device.as_ref().clone().into()]
        );

        let mut new_dh = MockDiscoveryHandlerEndpoint::new();
        let new_dh_senders = Arc::new(std::sync::Mutex::new(vec![]));
        let senders_vec = new_dh_senders.clone();
        new_dh
            .expect_get_name()
            .returning(|| "mock_handler".to_string());
        new_dh
            .expect_query()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(DiscoverRequest {
                    discovery_details: "discovery details".to_string(),
                    discovery_properties: HashMap::from([(
                        "property_1".to_owned(),
                        ByteData {
                            vec: Some(b"value_1".to_vec()),
                        },
                    )]),
                }),
            )
            .returning(move |s, _| {
                senders_vec.lock().unwrap().push(s);
                async { Ok(()) }.boxed()
            });
        assert!(new_dh_sen.send(Arc::new(new_dh)).is_ok());
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert_eq!(req.endpoints.read().await.len(), 2);
        new_dh_senders.lock().unwrap().pop();
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert_eq!(req.endpoints.read().await.len(), 1);
        drop(n_rec);
        assert!(task.await.is_ok())
    }

    #[tokio::test]
    async fn test_dh_reg_register_endpoint() {
        let (cdi_notifier, _) = watch::channel(Default::default());
        let (configuration_notifier, _) = mpsc::channel(2);
        let dh_reg = DHRegistryImpl::new(
            Arc::new(MockDiscoveryManagerKubeInterface::new()),
            cdi_notifier,
            configuration_notifier,
        );
        let mut endpoint = MockDiscoveryHandlerEndpoint::new();
        let (close_1, closed) = tokio::sync::oneshot::channel::<()>();
        endpoint.expect_get_name().return_const("mock_handler");
        endpoint.expect_get_uid().return_const("mock_handler_local");
        endpoint.expect_closed().return_once(|| {
            Box::pin(async {
                let _ = closed.await;
            })
        });
        endpoint.expect_is_closed().return_const(true);
        dh_reg.register_endpoint(Arc::new(endpoint)).await;
        assert!(dh_reg
            .handlers
            .read()
            .await
            .get("mock_handler")
            .unwrap()
            .get("mock_handler_local")
            .is_some());

        let mut endpoint = MockDiscoveryHandlerEndpoint::new();
        let (close_2, closed) = tokio::sync::oneshot::channel::<()>();
        endpoint.expect_get_name().return_const("mock_handler");
        endpoint
            .expect_get_uid()
            .return_const("mock_handler_local_2");
        endpoint.expect_closed().return_once(|| {
            Box::pin(async {
                let _ = closed.await;
            })
        });
        endpoint.expect_is_closed().once().return_const(false);
        endpoint.expect_is_closed().once().return_const(true);
        dh_reg.register_endpoint(Arc::new(endpoint)).await;
        assert!(dh_reg
            .handlers
            .read()
            .await
            .get("mock_handler")
            .unwrap()
            .get("mock_handler_local_2")
            .is_some());

        close_1.send(()).unwrap();
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert!(dh_reg
            .handlers
            .read()
            .await
            .get("mock_handler")
            .unwrap()
            .get("mock_handler_local")
            .is_none());
        assert!(dh_reg
            .handlers
            .read()
            .await
            .get("mock_handler")
            .unwrap()
            .get("mock_handler_local_2")
            .is_some());

        close_2.send(()).unwrap();
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert!(!dh_reg.handlers.read().await.contains_key("mock_handler"))
    }

    #[tokio::test]
    async fn test_dh_reg_get_terminate_request() {
        let (cdi_notifier, _) = watch::channel(Default::default());
        let (configuration_notifier, _) = mpsc::channel(2);
        let kube_client = Arc::new(MockDiscoveryManagerKubeInterface::new());
        let dh_reg = DHRegistryImpl::new(kube_client.clone(), cdi_notifier, configuration_notifier);
        let (req_not, _) = watch::channel(Default::default());
        let request = Arc::new(DHRequestImpl {
            endpoints: Default::default(),
            notifier: req_not,
            key: "my-config".to_owned(),
            handler_name: Default::default(),
            details: Default::default(),
            properties: Default::default(),
            extra_device_properties: Default::default(),
            kube_client,
            termination_notifier: Arc::new(Notify::new()),
        });
        dh_reg
            .requests
            .write()
            .await
            .insert("my-config".to_string(), request.clone());

        assert!(dh_reg.get_request("my-config").await.is_some());
        assert!(dh_reg.get_request("my-other-config").await.is_none());

        assert!(tokio::time::timeout(
            Duration::from_millis(500),
            request.termination_notifier.notified()
        )
        .await
        .is_err());
        let notif = request.termination_notifier.notified();

        dh_reg.terminate_request("my-config").await;
        assert!(tokio::time::timeout(Duration::from_millis(500), notif)
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn test_dh_reg_new_request() {
        let (cdi_notifier, mut cdi_rec) = watch::channel(Default::default());
        let (configuration_notifier, mut config_rec) = mpsc::channel(2);
        let kube_client = Arc::new(MockDiscoveryManagerKubeInterface::new());
        let dh_reg = DHRegistryImpl::new(kube_client.clone(), cdi_notifier, configuration_notifier);

        assert!(dh_reg
            .new_request(
                "my-config",
                "mock_handler",
                "discovery details",
                &[],
                HashMap::from([]),
                "namespace"
            )
            .await
            .is_err_and(|e| {
                matches!(e,
                    DiscoveryError::NoHandler(s) if s == *"mock_handler"
                )
            }));

        let dev_senders = Arc::new(std::sync::Mutex::new(vec![]));

        let mut endpoint = MockDiscoveryHandlerEndpoint::new();
        let (close_1, closed) = tokio::sync::oneshot::channel::<()>();
        let local_senders = dev_senders.clone();
        endpoint.expect_get_name().return_const("mock_handler");
        endpoint.expect_get_uid().return_const("mock_handler_local");
        endpoint.expect_closed().return_once(|| {
            Box::pin(async {
                let _ = closed.await;
            })
        });
        endpoint.expect_is_closed().return_const(true);
        endpoint.expect_query().returning(move |s, _| {
            local_senders.lock().unwrap().push(s);
            async { Ok(()) }.boxed()
        });
        dh_reg.register_endpoint(Arc::new(endpoint)).await;
        let mut endpoint = MockDiscoveryHandlerEndpoint::new();
        let (close_2, closed) = tokio::sync::oneshot::channel::<()>();
        let local_senders = dev_senders.clone();
        endpoint.expect_get_name().return_const("mock_handler");
        endpoint
            .expect_get_uid()
            .return_const("mock_handler_local_2");
        endpoint.expect_closed().return_once(|| {
            Box::pin(async {
                let _ = closed.await;
            })
        });
        endpoint.expect_is_closed().return_const(true);
        endpoint.expect_query().returning(move |s, _| {
            local_senders.lock().unwrap().push(s);
            async { Ok(()) }.boxed()
        });
        dh_reg.register_endpoint(Arc::new(endpoint)).await;

        assert!(dh_reg
            .new_request(
                "my-config",
                "mock_handler",
                "discovery details",
                &[],
                HashMap::from([]),
                "namespace"
            )
            .await
            .is_ok());

        assert!(cdi_rec.borrow_and_update().is_empty());
        assert_eq!(config_rec.try_recv(), Err(mpsc::error::TryRecvError::Empty));

        dev_senders
            .lock()
            .unwrap()
            .first()
            .unwrap()
            .send(vec![Arc::new(DiscoveredDevice::SharedDevice(Device {
                id: "dev_1".to_owned(),
                properties: Default::default(),
                mounts: Default::default(),
                device_specs: Default::default(),
            }))])
            .unwrap();
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert_eq!(
            config_rec.try_recv(),
            Ok(ObjectRef::new("my-config").within("namespace"))
        );

        dev_senders
            .lock()
            .unwrap()
            .get(1)
            .unwrap()
            .send(vec![Arc::new(DiscoveredDevice::SharedDevice(Device {
                id: "dev_2".to_owned(),
                properties: Default::default(),
                mounts: Default::default(),
                device_specs: Default::default(),
            }))])
            .unwrap();
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert_eq!(
            config_rec.try_recv(),
            Ok(ObjectRef::new("my-config").within("namespace"))
        );

        assert_eq!(
            cdi_rec.borrow_and_update().clone(),
            HashMap::from([(
                "akri.sh/my-config".to_owned(),
                Kind {
                    kind: "akri.sh/my-config".to_owned(),
                    annotations: Default::default(),
                    container_edits: vec![ContainerEdit::default()],
                    devices: vec![
                        crate::device_manager::cdi::Device {
                            name: "cb2ad7".to_owned(),
                            annotations: Default::default(),
                            container_edits: Default::default(),
                        },
                        crate::device_manager::cdi::Device {
                            name: "7bbc11".to_owned(),
                            annotations: Default::default(),
                            container_edits: Default::default(),
                        },
                    ]
                }
            )])
        );

        dev_senders.lock().unwrap().pop();
        close_2.send(()).unwrap();
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert_eq!(
            config_rec.try_recv(),
            Ok(ObjectRef::new("my-config").within("namespace"))
        );
        assert_eq!(
            cdi_rec.borrow_and_update().clone(),
            HashMap::from([(
                "akri.sh/my-config".to_owned(),
                Kind {
                    kind: "akri.sh/my-config".to_owned(),
                    annotations: Default::default(),
                    container_edits: vec![Default::default()],
                    devices: vec![crate::device_manager::cdi::Device {
                        name: "cb2ad7".to_owned(),
                        annotations: Default::default(),
                        container_edits: Default::default(),
                    },]
                }
            )])
        );

        dev_senders.lock().unwrap().pop();
        close_1.send(()).unwrap();
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert_eq!(
            config_rec.try_recv(),
            Ok(ObjectRef::new("my-config").within("namespace"))
        );
        assert!(cdi_rec.borrow_and_update().clone().is_empty());
    }
}
