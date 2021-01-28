use super::super::{
    discover::{
        discovery::{discovery_client::DiscoveryClient, Device, DiscoverRequest},
        register::RegisteredDiscoveryHandlerMap,
    },
    protocols, DISCOVERY_RESPONSE_TIME_METRIC, INSTANCE_COUNT_METRIC,
};
use super::{
    constants::{DISCOVERY_DELAY_SECS, SHARED_INSTANCE_OFFLINE_GRACE_PERIOD_SECS},
    device_plugin_service,
    device_plugin_service::{
        get_device_instance_name, ConnectivityStatus, InstanceInfo, InstanceMap,
    },
};
use akri_shared::{
    akri::{
        configuration::{Configuration, KubeAkriConfig, ProtocolHandler, ProtocolHandler2},
        API_CONFIGURATIONS, API_NAMESPACE, API_VERSION,
    },
    k8s,
    k8s::{try_delete_instance, KubeInterface},
};
use blake2::{
    digest::{Update, VariableOutput},
    VarBlake2b,
};
use futures::StreamExt;
use kube::api::{Informer, RawApi, WatchEvent};
use log::{info, trace};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    sync::{broadcast, mpsc, Mutex},
    time::timeout,
};
use tonic::transport::Channel;

// Checks if there is a registered DH for this protocol and returns it's endpoint.
fn get_discovery_handler_endpoint(protocol: &str) -> Option<String> {
    None
}

/// Information required for periodic discovery
#[derive(Clone)]
pub struct PeriodicDiscovery {
    config_name: String,
    config_uid: String,
    config_namespace: String,
    config_spec: Configuration,
    config_protocol: ProtocolHandler2,
    instance_map: InstanceMap,
    discovery_handler_map: RegisteredDiscoveryHandlerMap,
}

impl PeriodicDiscovery {
    pub fn new(
        config_name: &str,
        config_uid: &str,
        config_namespace: &str,
        config_spec: Configuration,
        config_protocol: ProtocolHandler2,
        instance_map: InstanceMap,
        discovery_handler_map: RegisteredDiscoveryHandlerMap,
    ) -> Self {
        PeriodicDiscovery {
            config_name: config_name.to_string(),
            config_uid: config_uid.to_string(),
            config_namespace: config_namespace.to_string(),
            config_spec,
            config_protocol,
            instance_map,
            discovery_handler_map,
        }
    }

    /// This is spawned as a task for each Configuration and continues to periodically run
    /// until the Config is deleted, at which point, this function is signaled to stop.
    /// Looks up which instances are currently visible to the node. Passes this list to a function that
    /// updates the ConnectivityStatus of the Configuration's Instances or deletes Instance CRDs if needed.
    /// If a new instance becomes visible that isn't in the Configuration's InstanceMap,
    /// a DevicePluginService and Instance CRD are created for it, and it is added to the InstanceMap.
    async fn do_discover(
        self,
        device_plugin_path: String,
        stop_discovery_sender: broadcast::Sender<()>,
        finished_discovery_sender: broadcast::Sender<()>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        let mut tasks = Vec::new();
        let discovery = Arc::new(self.clone());
        if let Some(discovery_handlers_map) = self
            .discovery_handler_map
            .lock()
            .unwrap()
            .get_mut(&self.config_protocol.name)
        {
            for (endpoint, is_local) in discovery_handlers_map.clone() {
                if let Ok(mut discovery_client) = DiscoveryClient::connect(endpoint.clone()).await {
                    // clone objects for thread
                    let mut stop_discovery_receiver = stop_discovery_sender.subscribe();
                    let discovery_details = self.config_protocol.discovery_details.clone();
                    let discovery = discovery.clone();
                    let device_plugin_path = device_plugin_path.clone();
                    tasks.push(tokio::spawn(async move {
                        let discover_request =
                            tonic::Request::new(DiscoverRequest { discovery_details });
                        let mut stream = discovery_client
                            .discover(discover_request)
                            .await
                            .unwrap()
                            .into_inner();
                        while let Some(result) = stream.message().await.unwrap() {
                            if stop_discovery_receiver.try_recv().is_ok() {
                                break;
                            }
                            handle_discovery_results(
                                discovery.clone(),
                                &device_plugin_path,
                                result.devices,
                                is_local,
                            )
                            .await
                            .unwrap();
                        }
                    }));
                }
            }
        }
        futures::future::try_join_all(tasks).await?;
        finished_discovery_sender.send(()).unwrap();
        Ok(())
    }
}

async fn handle_discovery_results(
    dis: Arc<PeriodicDiscovery>,
    device_plugin_path: &str,
    discovery_results: Vec<Device>,
    is_local: bool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    trace!(
        "handle_discovery_results - for config {} with discovery results {:?}",
        dis.config_name,
        discovery_results
    );
    let currently_visible_instances: HashMap<String, Device> = discovery_results
        .iter()
        .map(|discovery_result| {
            let id = generate_instance_digest(&discovery_result.id, !is_local);
            let instance_name = get_device_instance_name(&id, &dis.config_name);
            (instance_name, discovery_result.clone())
        })
        .collect();
    INSTANCE_COUNT_METRIC
        .with_label_values(&[&dis.config_name, &is_local.to_string()])
        .set(currently_visible_instances.len() as i64);
    // Update the connectivity status of instances and return list of visible instances that don't have Instance CRs
    let new_discovery_results =
        update_connectivity_status(dis.clone(), &currently_visible_instances, !is_local).await?;

    // If there are newly visible instances associated with a Config, make a device plugin and Instance CR for them
    if !new_discovery_results.is_empty() {
        for discovery_result in new_discovery_results {
            let id = generate_instance_digest(&discovery_result.id, !is_local);
            let instance_name = get_device_instance_name(&id, &dis.config_name);
            trace!(
                "do_periodic_discovery - new instance {} came online",
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
                device_plugin_path,
            )
            .await
            {
                // TODO: handle this with retry?
                error!(
                    "handle_discovery_results - error {} building device plugin",
                    e
                );
            }
        }
    }
    Ok(())
}

/// Takes in a list of currently visible instances and either updates an Instance's ConnectivityStatus or deletes an Instance.
/// If an instance is no longer visible then it's ConnectivityStatus is changed to Offline(time now).
/// The associated DevicePluginService checks its ConnectivityStatus before sending a response back to kubelet
/// and will send all unhealthy devices if its status is Offline, preventing kubelet from allocating any more pods to it.
/// An Instance CRD is deleted and it's DevicePluginService shutdown if its:
/// (A) shared instance is still not visible after 5 minutes or (B) unshared instance is still not visible on the next visibility check.
/// An unshared instance will be offline for between DISCOVERY_DELAY_SECS - 2 x DISCOVERY_DELAY_SECS
pub async fn update_connectivity_status(
    dis: Arc<PeriodicDiscovery>,
    currently_visible_instances: &HashMap<String, Device>,
    shared: bool,
) -> Result<Vec<Device>, Box<dyn std::error::Error + Send + Sync + 'static>> {
    let instance_map_clone = dis.instance_map.lock().await.clone();
    // Find all visible instances that do not have Instance CRDs yet
    let new_discovery_results: Vec<Device> = currently_visible_instances
        .iter()
        .filter(|(name, _)| !instance_map_clone.contains_key(*name))
        .map(|(_, p)| p.clone())
        .collect();

    for (instance, instance_info) in instance_map_clone {
        if currently_visible_instances.contains_key(&instance) {
            let connectivity_status = instance_info.connectivity_status;
            // If instance is visible, make sure connectivity status is (updated to be) Online
            if let ConnectivityStatus::Offline(_instant) = connectivity_status {
                trace!(
                    "update_connectivity_status - instance {} that was temporarily offline is back online",
                    instance
                );
                let list_and_watch_message_sender = instance_info.list_and_watch_message_sender;
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
            }
            trace!(
                "update_connectivity_status - instance {} still online",
                instance
            );
        } else {
            // If the instance is not visible:
            // // If the instance has not already been labeled offline, label it
            // // If the instance has already been labeled offline
            // // // shared - remove instance from map if grace period has elaspsed without the instance coming back online
            // // // unshared - remove instance from map
            match instance_info.connectivity_status {
                ConnectivityStatus::Online => {
                    let sender = instance_info.list_and_watch_message_sender.clone();
                    let updated_instance_info = InstanceInfo {
                        connectivity_status: ConnectivityStatus::Offline(Instant::now()),
                        list_and_watch_message_sender: instance_info.list_and_watch_message_sender,
                    };
                    dis.instance_map
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
                ConnectivityStatus::Offline(instant) => {
                    let time_offline = instant.elapsed().as_secs();
                    // If instance has been offline for longer than the grace period or it is unshared, terminate the associated device plugin
                    if !shared || time_offline >= SHARED_INSTANCE_OFFLINE_GRACE_PERIOD_SECS {
                        trace!("update_connectivity_status - instance {} has been offline too long ... terminating DevicePluginService", instance);
                        device_plugin_service::terminate_device_plugin_service(
                            &instance,
                            dis.instance_map.clone(),
                        )
                        .await?;
                        try_delete_instance(
                            &k8s::create_kube_interface(),
                            &instance,
                            &dis.config_namespace,
                        )
                        .await?;
                    }
                }
            }
        }
    }
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
    let digest = String::new();
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
