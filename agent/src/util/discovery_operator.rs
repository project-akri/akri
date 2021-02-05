use super::super::protocols::get_discovery_handler;
use super::{
    constants::{DEVICE_PLUGIN_PATH, SHARED_INSTANCE_OFFLINE_GRACE_PERIOD_SECS},
    device_plugin_service,
    device_plugin_service::{
        get_device_instance_name, ConnectivityStatus, InstanceInfo, InstanceMap,
    },
    registration::{
        DiscoveryHandlerConnectivityStatus, DiscoveryHandlerDetails, RegisteredDiscoveryHandlerMap,
        DH_OFFLINE_GRACE_PERIOD, EMBEDDED_DISCOVERY_HANDLER_ENDPOINT,
    },
    streaming_extension::StreamingExt,
};
use akri_discovery_utils::discovery::v0::{
    discovery_client::DiscoveryClient, Device, DiscoverRequest, DiscoverResponse,
};
use akri_shared::{akri::configuration::Configuration, k8s};
use blake2::{
    digest::{Update, VariableOutput},
    VarBlake2b,
};
use log::{error, info, trace};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{broadcast, mpsc};
use tonic::Status;

enum StreamType {
    Internal(mpsc::Receiver<std::result::Result<DiscoverResponse, Status>>),
    External(tonic::Streaming<DiscoverResponse>),
}

/// Information required for periodic discovery
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
        }
    }

    /// This is spawned as a task for each Configuration and continues to periodically run
    /// until the Config is deleted, at which point, this function is signaled to stop.
    /// Looks up which instances are currently visible to the node. Passes this list to a function that
    /// updates the ConnectivityStatus of the Configuration's Instances or deletes Instance CRDs if needed.
    /// If a new instance becomes visible that isn't in the Configuration's InstanceMap,
    /// a DevicePluginService and Instance CRD are created for it, and it is added to the InstanceMap.
    pub async fn start_discovery(
        self,
        new_discovery_handler_receiver: &mut broadcast::Receiver<String>,
        stop_all_discovery_receiver: &mut broadcast::Receiver<()>,
        finished_all_discovery_sender: &mut broadcast::Sender<()>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        info!("start_discovery - entered for protocol {}", self.protocol);
        let mut tasks = Vec::new();
        let discovery_operator = Arc::new(self.clone());
        let already_reg_discovery_operator = discovery_operator.clone();
        // Call discover on already registered Discovery Handlers for this protocol
        tasks.push(tokio::spawn(async move {
            already_reg_discovery_operator.do_discover().await.unwrap();
        }));
        loop {
            tokio::select! {
                _ = try_receive(stop_all_discovery_receiver) => {
                    info!("start_discovery - received message to stop discovery for protocol {}", discovery_operator.protocol);
                    self.stop_all_discovery().await;
                    break;
                },
                result = try_receive(new_discovery_handler_receiver) => {
                    // check if it is this protocol
                    if let Ok(protocol) = result {
                        if protocol == self.protocol {
                            info!("start_discovery - received new registered discovery handler for protocol {}", protocol);
                            let new_discovery_operator = discovery_operator.clone();
                            tasks.push(tokio::spawn(async move {
                                new_discovery_operator.do_discover().await.unwrap();
                            }));
                        }
                    }
                }
            }
        }
        futures::future::try_join_all(tasks).await?;
        finished_all_discovery_sender.send(()).unwrap();
        Ok(())
    }

    async fn stop_all_discovery(&self) {
        let mut discovery_handler_map = self.discovery_handler_map.lock().unwrap().clone();
        if let Some(protocol_dhs_map) = discovery_handler_map.get_mut(&self.protocol) {
            for (endpoint, dh_details) in protocol_dhs_map.clone() {
                match dh_details.stop_discovery.send(()) {
                    Ok(_) => info!("stop_all_discovery - discovery client for protocol {} at endpoint {} told to stop", self.protocol, endpoint),
                    Err(e) => error!("stop_all_discovery - discovery client for protocol {} at endpoint {} could not receive stop message with error {:?}", self.protocol, endpoint, e)
                }
            }
        }
    }

    async fn do_discover(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        info!("do_discover - entered for protocol {}", self.protocol);
        let mut tasks = Vec::new();
        let mut discovery_operator = Arc::new(self.clone());
        // get clone of map
        let mut discovery_handler_map = self.discovery_handler_map.lock().unwrap().clone();
        info!(
            "do_discover - discovery_handler_map is {:?}",
            discovery_handler_map
        );
        // discover on already registered DHs
        if let Some(protocol_dhs_map) = discovery_handler_map.get_mut(&self.protocol) {
            for (endpoint, dh_details) in protocol_dhs_map.clone() {
                info!(
                    "do_discover - for protocol {} and endpoint {}",
                    self.protocol, endpoint
                );
                discovery_operator = discovery_operator.clone();
                // Check if there is already a client
                if dh_details.connectivity_status != DiscoveryHandlerConnectivityStatus::HasClient {
                    info!(
                        "do_discover - endpoint {} for protocol {} doesnt have client",
                        endpoint, discovery_operator.protocol
                    );
                    let discovery_operator = discovery_operator.clone();
                    tasks.push(tokio::spawn(async move {
                        loop {
                            match discovery_operator.get_stream(&endpoint).await {
                                Some(stream_type) => {
                                    match stream_type {
                                        StreamType::External(mut stream) => {
                                            match discovery_operator.internal_do_discover(&endpoint, &dh_details, &mut stream).await {
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
                                            discovery_operator.internal_do_discover(&endpoint, &dh_details, &mut stream).await.unwrap();
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
        // get discovery client
        info!("get_stream - endpoint is {}", endpoint);
        if endpoint == EMBEDDED_DISCOVERY_HANDLER_ENDPOINT
            && get_discovery_handler(&self.discovery_details).is_ok()
        {
            info!(
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
        } else {
            match DiscoveryClient::connect(endpoint.to_string()).await {
                Ok(mut discovery_client) => {
                    info!(
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
                    info!("internal_do_discover - received message to stop discovery for endpoint {} serving protocol {}", endpoint, discovery_operator.protocol);
                    break;
                },
                result = stream.get_message() => {
                    let message = result?;
                    if let Some(response) = message {
                        info!("internal_do_discover - got discovery results {:?}", response.devices);
                        handle_discovery_results(
                            discovery_operator.clone(),
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
        info!("mark_offline_or_deregister - discovery handler at endpoint {} and protocol {} is offline", endpoint, self.protocol);
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
                    if instant.elapsed().as_secs() > DH_OFFLINE_GRACE_PERIOD {
                        info!("mark_offline_or_deregister - de-registering discovery handler for protocol {} at endpoint {} since been offline for longer than 5 minutes", self.protocol, endpoint);
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
}

async fn handle_discovery_results(
    dis: Arc<DiscoveryOperator>,
    discovery_results: Vec<Device>,
    is_local: bool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    info!(
        "handle_discovery_results - for config {} with discovery results {:?}",
        dis.config_name, discovery_results
    );
    let currently_visible_instances: HashMap<String, Device> = discovery_results
        .iter()
        .map(|discovery_result| {
            let id = generate_instance_digest(&discovery_result.id, !is_local);
            let instance_name = get_device_instance_name(&id, &dis.config_name);
            (instance_name, discovery_result.clone())
        })
        .collect();
    // Update the connectivity status of instances and return list of visible instances that don't have Instance CRs
    let new_discovery_results =
        update_connectivity_status(dis.clone(), currently_visible_instances, is_local).await?;

    // If there are newly visible instances associated with a Config, make a device plugin and Instance CR for them
    if !new_discovery_results.is_empty() {
        for discovery_result in new_discovery_results {
            let id = generate_instance_digest(&discovery_result.id, !is_local);
            let instance_name = get_device_instance_name(&id, &dis.config_name);
            info!(
                "handle_discovery_results - new instance {} came online",
                instance_name
            );
            let instance_properties = discovery_result.properties.clone();
            let config_spec = dis.config_spec.clone();
            let instance_map = dis.instance_map.clone();
            if let Err(e) = device_plugin_service::build_device_plugin(
                instance_name,
                dis.config_name.clone(),
                dis.config_uid.clone(),
                dis.config_namespace.clone(),
                config_spec,
                !is_local,
                instance_properties,
                instance_map,
                DEVICE_PLUGIN_PATH,
            )
            .await
            {
                error!("handle_discovery_results - error {} building device plugin ... trying again on next iteration", e);
            }
        }
    }
    Ok(())
}

async fn try_receive<T>(
    receiver: &mut broadcast::Receiver<T>,
) -> Result<T, tokio::sync::broadcast::RecvError>
where
    T: std::clone::Clone,
{
    receiver.recv().await
}

/// Takes in a list of currently visible instances and either updates an Instance's ConnectivityStatus or deletes an Instance.
/// If an instance is no longer visible then it's ConnectivityStatus is changed to Offline(time now).
/// The associated DevicePluginService checks its ConnectivityStatus before sending a response back to kubelet
/// and will send all unhealthy devices if its status is Offline, preventing kubelet from allocating any more pods to it.
/// An Instance CRD is deleted and it's DevicePluginService shutdown if its:
/// (A) shared instance is still not visible after 5 minutes or (B) unshared instance is still not visible on the next visibility check.
/// An unshared instance will be offline for between DISCOVERY_DELAY_SECS - 2 x DISCOVERY_DELAY_SECS
pub async fn update_connectivity_status(
    dis: Arc<DiscoveryOperator>,
    currently_visible_instances: HashMap<String, Device>,
    is_local: bool,
) -> Result<Vec<Device>, Box<dyn std::error::Error + Send + Sync + 'static>> {
    let instance_map = dis.instance_map.lock().await.clone();
    // Find all visible instances that do not have Instance CRDs yet
    let new_discovery_results: Vec<Device> = currently_visible_instances
        .iter()
        .filter(|(name, _)| !instance_map.contains_key(*name))
        .map(|(_, p)| p.clone())
        .collect();
    tokio::spawn(async move {
        let instance_map = dis.instance_map.lock().await.clone();
        loop {
            let mut keep_looping = false;
            for (instance, instance_info) in instance_map.clone() {
                info!("loop for instance {}", instance);
                let currently_visible_instances = currently_visible_instances.clone();
                let dis = dis.clone();
                if currently_visible_instances.contains_key(&instance) {
                    let connectivity_status = instance_info.connectivity_status;
                    // If instance is visible, make sure connectivity status is (updated to be) Online
                    if let ConnectivityStatus::Offline(_instant) = connectivity_status {
                        info!(
                            "update_connectivity_status - instance {} that was temporarily offline is back online",
                            instance
                        );
                        let list_and_watch_message_sender =
                            instance_info.list_and_watch_message_sender;
                        let updated_instance_info = InstanceInfo {
                            connectivity_status: ConnectivityStatus::Online,
                            list_and_watch_message_sender: list_and_watch_message_sender.clone(),
                        };
                        dis.instance_map
                            .lock()
                            .await
                            .insert(instance.clone(), updated_instance_info);
                        list_and_watch_message_sender
                            .send(device_plugin_service::ListAndWatchMessageKind::Continue)
                            .unwrap();
                    } else {
                        info!(
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
                        ConnectivityStatus::Online => {
                            if is_local {
                                remove_instance = true;
                            } else {
                                let sender = instance_info.list_and_watch_message_sender.clone();
                                let updated_instance_info = InstanceInfo {
                                    connectivity_status: ConnectivityStatus::Offline(Instant::now()),
                                    list_and_watch_message_sender: instance_info
                                        .list_and_watch_message_sender
                                        .clone(),
                                };
                                dis.instance_map
                                    .lock()
                                    .await
                                    .insert(instance.clone(), updated_instance_info);
                                info!(
                                    "update_connectivity_status - instance {} went offline ... starting timer and forcing list_and_watch to continue",
                                    instance
                                );
                                sender
                                    .send(device_plugin_service::ListAndWatchMessageKind::Continue)
                                    .unwrap();
                                keep_looping = true;
                            }
                        }
                        ConnectivityStatus::Offline(instant) => {
                            let time_offline = instant.elapsed().as_secs();
                            // If instance has been offline for longer than the grace period or it is unshared, terminate the associated device plugin
                            if is_local || time_offline >= SHARED_INSTANCE_OFFLINE_GRACE_PERIOD_SECS
                            {
                                remove_instance = true;
                            } else {
                                keep_looping = true;
                            }
                        }
                    }
                    if remove_instance {
                        info!("update_connectivity_status - instance {} has been offline too long ... terminating device plugin", instance);
                        device_plugin_service::terminate_device_plugin_service(
                            &instance,
                            dis.instance_map.clone(),
                        )
                        .await
                        .unwrap();
                        k8s::try_delete_instance(
                            &k8s::create_kube_interface(),
                            &instance,
                            &dis.config_namespace,
                        )
                        .await
                        .unwrap();
                    }
                }
            }
            if !keep_looping {
                info!("told to stop looping");
                break;
            }
            tokio::time::delay_for(Duration::from_secs(60)).await;
        }
    });
    Ok(new_discovery_results)
}

pub fn generate_instance_digest(id_to_digest: &str, shared: bool) -> String {
    let mut id_to_digest = id_to_digest.to_string();
    // For unshared devices, include node hostname in id_to_digest so instances have unique names
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
