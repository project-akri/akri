use super::super::INSTANCE_COUNT_METRIC;
use super::{
    constants::SHARED_INSTANCE_OFFLINE_GRACE_PERIOD_SECS,
    device_plugin_service,
    device_plugin_service::{
        get_device_instance_name, InstanceConnectivityStatus, InstanceInfo, InstanceMap,
    },
    embedded_discovery_handlers::get_discovery_handler,
    registration::{
        DiscoveryHandlerConnectivityStatus, DiscoveryHandlerDetails, RegisteredDiscoveryHandlerMap,
        DISCOVERY_HANDLER_OFFLINE_GRACE_PERIOD_SECS, EMBEDDED_DISCOVERY_HANDLER_ENDPOINT,
    },
    streaming_extension::StreamingExt,
};
use akri_discovery_utils::discovery::{
    v0::{discovery_client::DiscoveryClient, Device, DiscoverRequest, DiscoverResponse},
    DISCOVERY_HANDLER_PATH,
};
use akri_shared::{akri::configuration::Configuration, k8s};
use blake2::{
    digest::{Update, VariableOutput},
    VarBlake2b,
};
use log::{error, info, trace};
use std::{
    collections::HashMap,
    convert::TryFrom,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{broadcast, mpsc};
use tonic::{
    transport::{Endpoint, Uri},
    Status,
};

/// StreamType provides a wrapper around the two different types of streams returned from embedded
/// or internal discovery handlers and ones running externally.
enum StreamType {
    Internal(mpsc::Receiver<std::result::Result<DiscoverResponse, Status>>),
    External(tonic::Streaming<DiscoverResponse>),
}

/// A DiscoveryOperator is created for each Configuration that is applied to the cluster.
/// It handles discovery of the devices specified in a Configuration by calling `Discover` on
/// all registered discovery handlers that are using the same protocol as specified in `Configuration.protocol.name.`
/// For each device discovered by the discovery handlers, it creates a device plugin.
/// If a device disappears, it deletes the associated instance after a grace period (for non-local devices).
#[derive(Clone)]
pub struct DiscoveryOperator {
    discovery_handler_map: RegisteredDiscoveryHandlerMap,
    config_name: String,
    config_uid: String,
    config_namespace: String,
    config_spec: Configuration,
    protocol: String,
    discovery_details: HashMap<String, String>,
    instance_map: InstanceMap,
    device_plugin_path: String,
}

impl DiscoveryOperator {
    pub fn new(
        discovery_handler_map: RegisteredDiscoveryHandlerMap,
        config_name: &str,
        config_uid: &str,
        config_namespace: &str,
        config_spec: Configuration,
        protocol: &str,
        discovery_details: HashMap<String, String>,
        instance_map: InstanceMap,
        device_plugin_path: String,
    ) -> Self {
        DiscoveryOperator {
            discovery_handler_map,
            config_name: config_name.to_string(),
            config_uid: config_uid.to_string(),
            config_namespace: config_namespace.to_string(),
            config_spec,
            protocol: protocol.to_string(),
            discovery_details,
            instance_map,
            device_plugin_path,
        }
    }

    /// This is spawned as a task for each Configuration and continues to run
    /// until the Configuration is deleted, at which point, this function is signaled to stop.
    /// In a separate task, it calls connects to each discovery handler in the RegisteredDiscoveryHandlerMap
    /// with the same protocol name as the Configuration (Configuration.protocol.name). Then, it listens for
    /// updates from the discovery handler on what devices are currently visible to the node.
    /// Passes this list to a function that updates the InstanceConnectivityStatus of the Configuration's Instances
    /// or deletes Instance CRs if needed. If a new instance becomes visible that isn't in the Configuration's InstanceMap,
    /// a DevicePluginService and Instance CRD are created for it, and it is added to the InstanceMap.
    ///
    /// It also spawns a task to check whether Offline Instances have exceeded their grace period, in which case it
    /// deletes the Instance.
    pub async fn start_discovery(
        self,
        new_discovery_handler_sender: broadcast::Sender<String>,
        stop_all_discovery_sender: broadcast::Sender<()>,
        finished_all_discovery_sender: &mut broadcast::Sender<()>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        info!("start_discovery - entered for protocol {}", self.protocol);
        let config_name = self.config_name.clone();
        let mut tasks = Vec::new();
        let discovery_operator = Arc::new(self.clone());
        let task1_discovery_operator = discovery_operator.clone();
        // Call discover on already registered Discovery Handlers for this Configuration's protocol
        tasks.push(tokio::spawn(async move {
            task1_discovery_operator
                .do_discover(Arc::new(k8s::create_kube_interface()))
                .await
                .unwrap();
        }));
        let mut stop_all_discovery_receiver = stop_all_discovery_sender.subscribe();
        let mut new_discovery_handler_receiver = new_discovery_handler_sender.subscribe();
        let task2_discovery_operator = discovery_operator.clone();
        tasks.push(tokio::spawn(async move {
            let mut inner_tasks = Vec::new();
            loop {
                tokio::select! {
                    _ = try_receive(&mut stop_all_discovery_receiver) => {
                        trace!("start_discovery - received message to stop discovery for configuration {}", self.config_name);
                        // stop_offline_checks_sender.send(()).unwrap();
                        self.stop_all_discovery().await;
                        break;
                    },
                    result = try_receive(&mut new_discovery_handler_receiver) => {
                        // check if it is this protocol
                        if let Ok(protocol) = result {
                            if protocol == self.protocol {
                                trace!("start_discovery - received new registered discovery handler for configuration {}", self.config_name);
                                let new_discovery_operator = task2_discovery_operator.clone();
                                inner_tasks.push(tokio::spawn(async move {
                                    new_discovery_operator.do_discover(Arc::new(k8s::create_kube_interface())).await.unwrap();
                                }));
                            }
                        }
                    }
                }
            }
            futures::future::try_join_all(inner_tasks).await.unwrap();
        }));
        let kube_interface = Arc::new(k8s::create_kube_interface());
        let mut stop_all_discovery_receiver = stop_all_discovery_sender.subscribe();
        let task3_discovery_operator = discovery_operator.clone();
        // Non-local devices are only allowed to be offline for `SHARED_INSTANCE_OFFLINE_GRACE_PERIOD_SECS` minutes before being removed.
        // This task periodically checks if devices have been offline for too long.
        tasks.push(tokio::spawn(async move {
            loop {
                task3_discovery_operator
                    .check_offline_status(kube_interface.clone())
                    .await
                    .unwrap();
                if tokio::time::timeout(
                    Duration::from_secs(30),
                    stop_all_discovery_receiver.recv(),
                )
                .await.is_ok()
                {
                    trace!("start_discovery - received message to stop checking connectivity status for configuration {}", config_name);
                    break;
                }
            }
        }));
        futures::future::try_join_all(tasks).await?;
        finished_all_discovery_sender.send(()).unwrap();
        Ok(())
    }

    async fn stop_all_discovery(&self) {
        let mut discovery_handler_map = self.discovery_handler_map.lock().unwrap().clone();
        if let Some(protocol_dhs_map) = discovery_handler_map.get_mut(&self.protocol) {
            for (endpoint, dh_details) in protocol_dhs_map.clone() {
                match dh_details.stop_discovery.send(()) {
                    Ok(_) => trace!("stop_all_discovery - discovery client for protocol {} at endpoint {} told to stop", self.protocol, endpoint),
                    Err(e) => error!("stop_all_discovery - discovery client for protocol {} at endpoint {} could not receive stop message with error {:?}", self.protocol, endpoint, e)
                }
            }
        }
    }

    pub async fn do_discover(
        &self,
        kube_interface: Arc<impl k8s::KubeInterface + 'static>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        trace!("do_discover - entered for protocol {}", self.protocol);
        let mut tasks = Vec::new();
        let mut discovery_operator = Arc::new(self.clone());
        // get clone of map
        let mut discovery_handler_map = self.discovery_handler_map.lock().unwrap().clone();
        trace!(
            "do_discover - discovery_handler_map is {:?}",
            discovery_handler_map
        );
        // discover on already registered DHs
        if let Some(protocol_dhs_map) = discovery_handler_map.get_mut(&self.protocol) {
            for (endpoint, dh_details) in protocol_dhs_map.clone() {
                trace!(
                    "do_discover - for protocol {} and endpoint {}",
                    self.protocol,
                    endpoint
                );
                discovery_operator = discovery_operator.clone();
                // Check if there is already a client
                if dh_details.connectivity_status != DiscoveryHandlerConnectivityStatus::HasClient {
                    trace!(
                        "do_discover - endpoint {} for protocol {} doesnt have client",
                        endpoint,
                        discovery_operator.protocol
                    );
                    let discovery_operator = discovery_operator.clone();
                    let kube_interface = kube_interface.clone();
                    tasks.push(tokio::spawn(async move {
                        loop {
                            match discovery_operator.get_stream(&endpoint).await {
                                Some(stream_type) => {
                                    match stream_type {
                                        StreamType::External(mut stream) => {
                                            match discovery_operator.internal_do_discover(kube_interface.clone(), &endpoint, &dh_details, &mut stream).await {
                                                Ok(_) => {break;},
                                                Err(status) => {
                                                    if status.message().contains("broken pipe") {
                                                        let deregistered = discovery_operator.mark_offline_or_deregister(&endpoint).await.unwrap();
                                                        if deregistered {
                                                            break;
                                                        }
                                                    } else {
                                                        println!("do_discover - DH server returned other error status {}", status);
                                                        // TODO: Check for config error
                                                        break;
                                                    }
                                                }
                                            }
                                        },
                                        StreamType::Internal(mut stream) => {
                                            discovery_operator.internal_do_discover(kube_interface.clone(), &endpoint, &dh_details, &mut stream).await.unwrap();
                                            break;
                                        }
                                    }
                                },
                                None => {
                                    let deregistered = discovery_operator.mark_offline_or_deregister(&endpoint).await.unwrap();
                                    if deregistered {
                                        break;
                                    }
                                },
                            }
                        }
                    }));
                }
            }
        }
        futures::future::try_join_all(tasks).await?;
        Ok(())
    }

    async fn get_stream(&self, endpoint: &str) -> Option<StreamType> {
        let discover_request = tonic::Request::new(DiscoverRequest {
            discovery_details: self.discovery_details.clone(),
        });
        trace!("get_stream - endpoint is {}", endpoint);
        if endpoint == EMBEDDED_DISCOVERY_HANDLER_ENDPOINT
            && get_discovery_handler(&self.discovery_details).is_ok()
        {
            trace!(
                "get_stream - internal discovery handler for protocol {}",
                self.protocol
            );
            let discovery_handler = get_discovery_handler(&self.discovery_details).unwrap();
            Some(StreamType::Internal(
                discovery_handler
                    .discover(discover_request)
                    .await
                    .unwrap()
                    .into_inner(),
            ))
        // Check if is a UDS connection
        } else if endpoint.starts_with(DISCOVERY_HANDLER_PATH) {
            let path = endpoint.to_string();
            match Endpoint::try_from("lttp://[::]:50051")
                .unwrap()
                .connect_with_connector(tower::service_fn(move |_: Uri| {
                    let endpoint = path.clone();
                    tokio::net::UnixStream::connect(endpoint)
                }))
                .await
            {
                Ok(channel) => {
                    trace!(
                        "get_stream - external discovery handler for protocol {}",
                        self.protocol
                    );
                    let mut discovery_client = DiscoveryClient::new(channel);
                    Some(StreamType::External(
                        discovery_client
                            .discover(discover_request)
                            .await
                            .unwrap()
                            .into_inner(),
                    ))
                }
                Err(e) => {
                    error!("get_stream - failed to connect to client with error {}", e);
                    None
                }
            }
        } else {
            match DiscoveryClient::connect(endpoint.to_string()).await {
                Ok(mut discovery_client) => {
                    trace!(
                        "get_stream - external discovery handler for protocol {}",
                        self.protocol
                    );
                    Some(StreamType::External(
                        discovery_client
                            .discover(discover_request)
                            .await
                            .unwrap()
                            .into_inner(),
                    ))
                }
                Err(e) => {
                    error!("get_stream - failed to connect to client with error {}", e);
                    None
                }
            }
        }
    }
    async fn internal_do_discover(
        &self,
        kube_interface: Arc<impl k8s::KubeInterface + 'static>,
        endpoint: &str,
        dh_details: &DiscoveryHandlerDetails,
        stream: &mut impl StreamingExt,
    ) -> Result<(), Status> {
        // clone objects for thread
        let discovery_operator = Arc::new(self.clone());
        let stop_discovery_receiver: &mut broadcast::Receiver<()> =
            &mut dh_details.stop_discovery.subscribe();
        loop {
            // Wait for either new discovery results or a message to stop discovery
            tokio::select! {
                _ = try_receive(stop_discovery_receiver) => {
                    trace!("internal_do_discover - received message to stop discovery for endpoint {} serving protocol {}", endpoint, discovery_operator.protocol);
                    break;
                },
                result = stream.get_message() => {
                    let message = result?;
                    if let Some(response) = message {
                        trace!("internal_do_discover - got discovery results {:?}", response.devices);
                        self.handle_discovery_results(
                            kube_interface.clone(),
                            response.devices,
                            dh_details.register_request.is_local,
                        )
                        .await
                        .unwrap();
                    } else {
                        error!("internal_do_discover - received result of type None");
                    }
                }
            }
        }

        Ok(())
    }

    async fn mark_offline_or_deregister(
        &self,
        endpoint: &str,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync + 'static>> {
        trace!("mark_offline_or_deregister - discovery handler at endpoint {} and protocol {} is offline", endpoint, self.protocol);
        let mut deregistered = false;
        if let Some(registered_dh_map) = self
            .discovery_handler_map
            .lock()
            .unwrap()
            .get_mut(&self.protocol)
        {
            if let Some(dh_details) = registered_dh_map.get(endpoint) {
                if let DiscoveryHandlerConnectivityStatus::Offline(instant) =
                    dh_details.connectivity_status
                {
                    if instant.elapsed().as_secs() > DISCOVERY_HANDLER_OFFLINE_GRACE_PERIOD_SECS {
                        trace!("mark_offline_or_deregister - de-registering discovery handler for protocol {} at endpoint {} since been offline for longer than 5 minutes", self.protocol, endpoint);
                        // Remove discovery handler from map if timed out
                        registered_dh_map.remove(endpoint);
                        deregistered = true;
                    }
                } else {
                    let mut dh_details = dh_details.clone();
                    dh_details.connectivity_status =
                        DiscoveryHandlerConnectivityStatus::Offline(Instant::now());
                    registered_dh_map.insert(endpoint.to_string(), dh_details);
                }
            }
        }
        if !deregistered {
            tokio::time::delay_for(Duration::from_secs(60)).await;
        }
        Ok(deregistered)
    }

    /// Checks if any of this DiscoveryOperator's Configuration's Instances have been offline for too long.
    /// If a non-local device has not come back online before `SHARED_INSTANCE_OFFLINE_GRACE_PERIOD_SECS`,
    /// the associated device plugin and instance are terminated and deleted.
    pub async fn check_offline_status(
        &self,
        kube_interface: Arc<impl k8s::KubeInterface + 'static>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        trace!(
            "check_offline_status - entered for configuration {}",
            self.config_name
        );
        // Find all visible instances that do not have Instance CRs yet
        let kube_interface_clone = kube_interface.clone();
        let instance_map = self.instance_map.lock().await.clone();
        for (instance, instance_info) in instance_map.clone() {
            trace!("loop for instance {}", instance);
            match instance_info.connectivity_status {
                InstanceConnectivityStatus::Online => {}
                InstanceConnectivityStatus::Offline(instant) => {
                    let time_offline = instant.elapsed().as_secs();
                    // If instance has been offline for longer than the grace period or it is unshared, terminate the associated device plugin
                    // TODO: make grace period configurable
                    if time_offline >= SHARED_INSTANCE_OFFLINE_GRACE_PERIOD_SECS {
                        trace!("check_offline_status - instance {} has been offline too long ... terminating device plugin", instance);
                        device_plugin_service::terminate_device_plugin_service(
                            &instance,
                            self.instance_map.clone(),
                        )
                        .await
                        .unwrap();
                        k8s::try_delete_instance_arc(
                            kube_interface_clone.clone(),
                            &instance,
                            &self.config_namespace,
                        )
                        .await?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Takes in a list of discovered devices and determines if there are any new devices or no longer visible devices.
    /// For each new device, it creates a DevicePluginService.
    /// For each previously visible device that was no longer discovered, it calls a function that updates the InstanceConnectivityStatus
    /// of the instance or deletes it if it is a local device.
    async fn handle_discovery_results(
        &self,
        kube_interface: Arc<impl k8s::KubeInterface + 'static>,
        discovery_results: Vec<Device>,
        is_local: bool,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        trace!(
            "handle_discovery_results - for config {} with discovery results {:?}",
            self.config_name,
            discovery_results
        );
        let currently_visible_instances: HashMap<String, Device> = discovery_results
            .iter()
            .map(|discovery_result| {
                let id = generate_instance_digest(&discovery_result.id, !is_local);
                let instance_name = get_device_instance_name(&id, &self.config_name);
                (instance_name, discovery_result.clone())
            })
            .collect();
        INSTANCE_COUNT_METRIC
            .with_label_values(&[&self.config_name, &is_local.to_string()])
            .set(currently_visible_instances.len() as i64);
        // Update the connectivity status of instances and return list of visible instances that don't have Instance CRs
        let instance_map = self.instance_map.lock().await.clone();
        // Find all visible instances that do not have Instance CRDs yet
        let new_discovery_results: Vec<Device> = currently_visible_instances
            .iter()
            .filter(|(name, _)| !instance_map.contains_key(*name))
            .map(|(_, p)| p.clone())
            .collect();
        self.update_connectivity_status(kube_interface, currently_visible_instances, is_local)
            .await?;

        // If there are newly visible instances associated with a Config, make a device plugin and Instance CR for them
        if !new_discovery_results.is_empty() {
            for discovery_result in new_discovery_results {
                let id = generate_instance_digest(&discovery_result.id, !is_local);
                let instance_name = get_device_instance_name(&id, &self.config_name);
                trace!(
                    "handle_discovery_results - new instance {} came online",
                    instance_name
                );
                let config_spec = self.config_spec.clone();
                let instance_map = self.instance_map.clone();
                if let Err(e) = device_plugin_service::build_device_plugin(
                    instance_name,
                    self.config_name.clone(),
                    self.config_uid.clone(),
                    self.config_namespace.clone(),
                    config_spec,
                    !is_local,
                    instance_map,
                    &self.device_plugin_path,
                    discovery_result.clone(),
                )
                .await
                {
                    error!("handle_discovery_results - error {} building device plugin ... trying again on next iteration", e);
                }
            }
        }
        Ok(())
    }

    /// Takes in a list of currently visible instances and either updates an Instance's InstanceConnectivityStatus or deletes an Instance.
    /// If a non-local/network based device is not longer visible it's InstanceConnectivityStatus is changed to Offline(time now).
    /// The associated DevicePluginService checks its InstanceConnectivityStatus before sending a response back to kubelet
    /// and will send all unhealthy devices if its status is Offline, preventing kubelet from allocating any more pods to it.
    /// An Instance CRD is deleted and it's DevicePluginService shutdown if its:
    /// (A) non-local Instance is still not visible after 5 minutes or (B) local instance is still not visible.
    pub async fn update_connectivity_status(
        &self,
        kube_interface: Arc<impl k8s::KubeInterface + 'static>,
        currently_visible_instances: HashMap<String, Device>,
        is_local: bool,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        let instance_map = self.instance_map.lock().await.clone();
        for (instance, instance_info) in instance_map {
            trace!(
                "update_connectivity_status - checking connectivity status of instance {}",
                instance
            );
            if currently_visible_instances.contains_key(&instance) {
                let connectivity_status = instance_info.connectivity_status;
                // If instance is visible, make sure connectivity status is (updated to be) Online
                if let InstanceConnectivityStatus::Offline(_instant) = connectivity_status {
                    trace!(
                        "update_connectivity_status - instance {} that was temporarily offline is back online",
                        instance
                    );
                    let list_and_watch_message_sender = instance_info.list_and_watch_message_sender;
                    let updated_instance_info = InstanceInfo {
                        connectivity_status: InstanceConnectivityStatus::Online,
                        list_and_watch_message_sender: list_and_watch_message_sender.clone(),
                    };
                    self.instance_map
                        .lock()
                        .await
                        .insert(instance.clone(), updated_instance_info);
                    // Signal list_and_watch to update kubelet that the devices are healthy.
                    list_and_watch_message_sender
                        .send(device_plugin_service::ListAndWatchMessageKind::Continue)
                        .unwrap();
                } else {
                    trace!(
                        "update_connectivity_status - instance {} still online",
                        instance
                    );
                }
            } else {
                // If the instance is not visible:
                // // If the instance is local, remove it
                // // If the instance is not local
                // // // If it has not already been labeled offline, label it
                // // // If the instance has already been labeled offline
                // // // remove instance from map if grace period has elapsed without the instance coming back online
                let mut remove_instance = false;
                match instance_info.connectivity_status {
                    InstanceConnectivityStatus::Online => {
                        if is_local {
                            remove_instance = true;
                        } else {
                            let sender = instance_info.list_and_watch_message_sender.clone();
                            let updated_instance_info = InstanceInfo {
                                connectivity_status: InstanceConnectivityStatus::Offline(
                                    Instant::now(),
                                ),
                                list_and_watch_message_sender: instance_info
                                    .list_and_watch_message_sender
                                    .clone(),
                            };
                            self.instance_map
                                .lock()
                                .await
                                .insert(instance.clone(), updated_instance_info);
                            trace!(
                                "update_connectivity_status - instance {} went offline ... starting timer and forcing list_and_watch to continue",
                                instance
                            );
                            sender
                                .send(device_plugin_service::ListAndWatchMessageKind::Continue)
                                .unwrap();
                        }
                    }
                    InstanceConnectivityStatus::Offline(instant) => {
                        let time_offline = instant.elapsed().as_secs();
                        println!("time elapsed {}", time_offline);
                        // If instance has been offline for longer than the grace period, terminate the associated device plugin
                        if time_offline >= SHARED_INSTANCE_OFFLINE_GRACE_PERIOD_SECS {
                            remove_instance = true;
                        }
                    }
                }
                if remove_instance {
                    trace!("update_connectivity_status - instance {} has been offline too long ... terminating device plugin", instance);
                    device_plugin_service::terminate_device_plugin_service(
                        &instance,
                        self.instance_map.clone(),
                    )
                    .await
                    .unwrap();
                    k8s::try_delete_instance_arc(
                        kube_interface.clone(),
                        &instance,
                        &self.config_namespace,
                    )
                    .await
                    .unwrap();
                }
            }
        }
        Ok(())
    }
}

async fn try_receive<T>(
    receiver: &mut broadcast::Receiver<T>,
) -> Result<T, tokio::sync::broadcast::RecvError>
where
    T: std::clone::Clone,
{
    receiver.recv().await
}

/// Generates an digest of an Instance's id. There should be a unique digest and Instance for each discovered device.
/// This means that the id of non-local devices that could be visible to multiple nodes should always resolve
/// to the same instance name (which is suffixed with this digest).
/// However, local devices' Instances should have unique hashes even if they have the same id.
/// To ensure this, the node's name is added to the id before it is hashed.
pub fn generate_instance_digest(id_to_digest: &str, shared: bool) -> String {
    let mut id_to_digest = id_to_digest.to_string();
    // For local devices, include node hostname in id_to_digest so instances have unique names
    if !shared {
        id_to_digest = format!(
            "{}{}",
            &id_to_digest,
            std::env::var("AGENT_NODE_NAME").unwrap()
        );
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
