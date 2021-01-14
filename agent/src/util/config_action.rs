use super::super::{protocols, DISCOVERY_RESPONSE_TIME_METRIC, INSTANCE_COUNT_METRIC};
use super::{
    constants::{
        DEVICE_PLUGIN_PATH, DISCOVERY_DELAY_SECS, SHARED_INSTANCE_OFFLINE_GRACE_PERIOD_SECS,
    },
    device_plugin_service,
    device_plugin_service::{
        get_device_instance_name, ConnectivityStatus, InstanceInfo, InstanceMap,
    },
};
use akri_shared::{
    akri::{
        configuration::{Configuration, KubeAkriConfig, ProtocolHandler},
        API_CONFIGURATIONS, API_NAMESPACE, API_VERSION,
    },
    k8s,
    k8s::KubeInterface,
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

type ConfigMap = Arc<Mutex<HashMap<String, ConfigInfo>>>;

/// Information for managing a Configuration, such as all applied Instances of that Configuration
/// and senders for ceasing to discover instances upon Configuration deletion.
#[derive(Debug)]
pub struct ConfigInfo {
    instance_map: InstanceMap,
    stop_discovery_sender: mpsc::Sender<()>,
    finished_discovery_sender: broadcast::Sender<()>,
}

/// This handles pre-existing Configurations and invokes an internal method that watches for Configuration events.
pub async fn do_config_watch() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    info!("do_config_watch - enter");
    let config_map: ConfigMap = Arc::new(Mutex::new(HashMap::new()));
    let kube_interface = k8s::create_kube_interface();
    let mut tasks = Vec::new();

    // Handle pre-existing configs
    let pre_existing_configs = kube_interface.get_configurations().await?;
    for config in pre_existing_configs {
        let config_map = config_map.clone();
        tasks.push(tokio::spawn(async move {
            handle_config_add(&config, config_map).await.unwrap();
        }));
    }

    // Watch for new configs and changes
    tasks.push(tokio::spawn(async move {
        watch_for_config_changes(&kube_interface, config_map)
            .await
            .unwrap();
    }));

    futures::future::try_join_all(tasks).await?;
    info!("do_config_watch - end");
    Ok(())
}

/// This watches for Configuration events
async fn watch_for_config_changes(
    kube_interface: &impl KubeInterface,
    config_map: ConfigMap,
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    trace!("watch_for_config_changes - start");
    let akri_config_type = RawApi::customResource(API_CONFIGURATIONS)
        .group(API_NAMESPACE)
        .version(API_VERSION);
    let informer = Informer::raw(kube_interface.get_kube_client(), akri_config_type)
        .init()
        .await?;
    loop {
        let mut configs = informer.poll().await?.boxed();

        // Currently, this does not handle None except to break the
        // while.
        while let Some(event) = configs.next().await {
            handle_config(kube_interface, event?, config_map.clone()).await?
        }
    }
}

/// This takes an event off the Configuration stream and delegates it to the
/// correct function based on the event type.
async fn handle_config(
    kube_interface: &impl KubeInterface,
    event: WatchEvent<KubeAkriConfig>,
    config_map: ConfigMap,
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    trace!("handle_config - something happened to a configuration");
    match event {
        WatchEvent::Added(config) => {
            info!(
                "handle_config - added Configuration {}",
                config.metadata.name
            );
            tokio::spawn(async move {
                handle_config_add(&config, config_map).await.unwrap();
            });
            Ok(())
        }
        WatchEvent::Deleted(config) => {
            info!(
                "handle_config - deleted Configuration {}",
                config.metadata.name,
            );
            handle_config_delete(kube_interface, &config, config_map).await?;
            Ok(())
        }
        // If a config is updated, delete all associated instances and device plugins and then recreate them to reflect updated config
        WatchEvent::Modified(config) => {
            info!(
                "handle_config - modified Configuration {}",
                config.metadata.name,
            );
            handle_config_delete(kube_interface, &config, config_map.clone()).await?;
            tokio::spawn(async move {
                handle_config_add(&config, config_map).await.unwrap();
            });
            Ok(())
        }
        WatchEvent::Error(ref e) => {
            error!("handle_config - error for Configuration: {}", e);
            Ok(())
        }
    }
}

/// This handles added Configuration by creating a new ConfigInfo for it and adding it to the ConfigMap.
/// Then calls a function to continually observe the availability of instances associated with the Configuration.
async fn handle_config_add(
    config: &KubeAkriConfig,
    config_map: ConfigMap,
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    let config_protocol = config.spec.protocol.clone();
    let discovery_handler = protocols::get_discovery_handler(&config_protocol)?;
    let discovery_results = discovery_handler.discover().await?;
    let config_name = config.metadata.name.clone();
    let config_uid = config.metadata.uid.as_ref().unwrap().clone();
    let config_namespace = config.metadata.namespace.as_ref().unwrap().clone();
    info!(
        "handle_config_add - entered for Configuration {} with visible_instances={:?}",
        config.metadata.name, &discovery_results
    );
    // Create a new instance map for this config and add it to the config map
    let instance_map: InstanceMap = Arc::new(Mutex::new(HashMap::new()));
    // Channel capacity: should only ever be sent once upon config deletion
    let (stop_discovery_sender, stop_discovery_receiver) = mpsc::channel(1);
    // Channel capacity: should only ever be sent once upon receiving stop watching message
    let (finished_discovery_sender, _) = broadcast::channel(1);
    let config_info = ConfigInfo {
        instance_map: instance_map.clone(),
        stop_discovery_sender,
        finished_discovery_sender: finished_discovery_sender.clone(),
    };
    config_map
        .lock()
        .await
        .insert(config_name.clone(), config_info);

    let kube_interface = k8s::create_kube_interface();
    let config_spec = config.spec.clone();
    // Keep discovering instances until the config is deleted, signaled by a message from handle_config_delete
    tokio::spawn(async move {
        let periodic_discovery = PeriodicDiscovery {
            config_name,
            config_uid,
            config_namespace,
            config_spec,
            config_protocol,
            instance_map,
        };
        periodic_discovery
            .do_periodic_discovery(
                &kube_interface,
                stop_discovery_receiver,
                finished_discovery_sender,
                DEVICE_PLUGIN_PATH,
            )
            .await
            .unwrap();
    })
    .await?;
    Ok(())
}

/// This handles a deleted Congfiguration. First, it ceases to discover instances associated with the Configuration.
/// Then, for each of the Configuration's Instances, it signals the DevicePluginService to shutdown,
/// and deletes the Instance CRD.
pub async fn handle_config_delete(
    kube_interface: &impl KubeInterface,
    config: &KubeAkriConfig,
    config_map: ConfigMap,
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    trace!(
        "handle_config_delete - for config {} telling do_periodic_discovery to end",
        config.metadata.name
    );
    // Send message to stop observing instances' availability and waits until response is received
    if config_map
        .lock()
        .await
        .get(&config.metadata.name)
        .unwrap()
        .stop_discovery_sender
        .clone()
        .send(())
        .await
        .is_ok()
    {
        let mut finished_discovery_receiver = config_map
            .lock()
            .await
            .get(&config.metadata.name)
            .unwrap()
            .finished_discovery_sender
            .subscribe();
        finished_discovery_receiver.recv().await.unwrap();
        trace!(
            "handle_config_delete - for config {} received message that do_periodic_discovery ended",
            config.metadata.name
        );
    } else {
        trace!(
            "handle_config_delete - for config {} do_periodic_discovery receiver has been dropped",
            config.metadata.name
        );
    }

    // Get map of instances for the Configuration and then remove Configuration from ConfigMap
    let instance_map: InstanceMap;
    {
        let mut config_map_locked = config_map.lock().await;
        instance_map = config_map_locked
            .get(&config.metadata.name)
            .unwrap()
            .instance_map
            .clone();
        config_map_locked.remove(&config.metadata.name);
    }

    // Shutdown Instances' DevicePluginServices and delete the Instances
    let mut instance_map_locked = instance_map.lock().await;
    let instances_to_delete_map = instance_map_locked.clone();
    let namespace = config.metadata.namespace.as_ref().unwrap();
    for (instance_name, instance_info) in instances_to_delete_map {
        trace!(
            "handle_config_delete - found Instance {} associated with deleted config {} ... sending message to end list_and_watch",
            instance_name,
            config.metadata.name
        );
        instance_info
            .list_and_watch_message_sender
            .send(device_plugin_service::ListAndWatchMessageKind::End)
            .unwrap();
        instance_map_locked.remove(&instance_name);
        try_delete_instance(kube_interface, &instance_name, &namespace).await?;
    }

    Ok(())
}

/// This deletes an Instance unless it has already been deleted by another node
async fn try_delete_instance(
    kube_interface: &impl KubeInterface,
    instance_name: &str,
    instance_namespace: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    match kube_interface
        .delete_instance(instance_name, &instance_namespace)
        .await
    {
        Ok(()) => {
            trace!("delete_instance - deleted Instance {}", instance_name);
            Ok(())
        }
        Err(e) => {
            // Check if already was deleted else return error
            if let Err(_e) = kube_interface
                .find_instance(&instance_name, &instance_namespace)
                .await
            {
                trace!(
                    "delete_instance - discovered Instance {} already deleted",
                    instance_name
                );
                Ok(())
            } else {
                Err(e)
            }
        }
    }
}

/// Information required for periodic discovery
struct PeriodicDiscovery {
    config_name: String,
    config_uid: String,
    config_namespace: String,
    config_spec: Configuration,
    config_protocol: ProtocolHandler,
    instance_map: InstanceMap,
}

impl PeriodicDiscovery {
    /// This is spawned as a task for each Configuration and continues to periodically run
    /// until the Config is deleted, at which point, this function is signaled to stop.
    /// Looks up which instances are currently visible to the node. Passes this list to a function that
    /// updates the ConnectivityStatus of the Configuration's Instances or deletes Instance CRDs if needed.
    /// If a new instance becomes visible that isn't in the Configuration's InstanceMap,
    /// a DevicePluginService and Instance CRD are created for it, and it is added to the InstanceMap.
    async fn do_periodic_discovery(
        &self,
        kube_interface: &impl KubeInterface,
        mut stop_discovery_receiver: mpsc::Receiver<()>,
        finished_discovery_sender: broadcast::Sender<()>,
        device_plugin_path: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        trace!(
            "do_periodic_discovery - start for config {}",
            self.config_name
        );
        let protocol = protocols::get_discovery_handler(&self.config_protocol)?;
        let shared = protocol.are_shared()?;
        loop {
            trace!(
                "do_periodic_discovery - loop iteration for config {}",
                &self.config_name
            );
            let config_name = self.config_name.clone();
            let timer = DISCOVERY_RESPONSE_TIME_METRIC
                .with_label_values(&[&config_name])
                .start_timer();
            let discovery_results = protocol.discover().await?;
            timer.observe_duration();
            let currently_visible_instances: HashMap<String, protocols::DiscoveryResult> =
                discovery_results
                    .iter()
                    .map(|discovery_result| {
                        let instance_name =
                            get_device_instance_name(&discovery_result.digest, &config_name);
                        (instance_name, discovery_result.clone())
                    })
                    .collect();
            INSTANCE_COUNT_METRIC
                .with_label_values(&[&config_name, &shared.to_string()])
                .set(currently_visible_instances.len() as i64);
            // Update the connectivity status of instances and return list of visible instances that don't have Instance CRs
            let new_discovery_results = self
                .update_connectivity_status(kube_interface, &currently_visible_instances, shared)
                .await?;

            // If there are newly visible instances associated with a Config, make a device plugin and Instance CR for them
            if !new_discovery_results.is_empty() {
                for discovery_result in new_discovery_results {
                    let config_name = config_name.clone();
                    let instance_name =
                        get_device_instance_name(&discovery_result.digest, &config_name);
                    trace!(
                        "do_periodic_discovery - new instance {} came online",
                        instance_name
                    );
                    let instance_properties = discovery_result.properties.clone();
                    let config_spec = self.config_spec.clone();
                    let instance_map = self.instance_map.clone();
                    if let Err(e) = device_plugin_service::build_device_plugin(
                        instance_name,
                        config_name,
                        self.config_uid.clone(),
                        self.config_namespace.clone(),
                        config_spec,
                        shared,
                        instance_properties,
                        instance_map,
                        device_plugin_path,
                    )
                    .await
                    {
                        error!("do_periodic_discovery - error {} building device plugin ... trying again on next iteration", e);
                    }
                }
            }
            if timeout(
                Duration::from_secs(DISCOVERY_DELAY_SECS),
                stop_discovery_receiver.recv(),
            )
            .await
            .is_ok()
            {
                trace!("do_periodic_discovery - for config {} received message to end ... sending message that finished and returning Ok", config_name);
                finished_discovery_sender.send(()).unwrap();
                return Ok(());
            };
        }
    }

    /// Takes in a list of currently visible instances and either updates an Instance's ConnectivityStatus or deletes an Instance.
    /// If an instance is no longer visible then it's ConnectivityStatus is changed to Offline(time now).
    /// The associated DevicePluginService checks its ConnectivityStatus before sending a response back to kubelet
    /// and will send all unhealthy devices if its status is Offline, preventing kubelet from allocating any more pods to it.
    /// An Instance CRD is deleted and it's DevicePluginService shutdown if its:
    /// (A) shared instance is still not visible after 5 minutes or (B) unshared instance is still not visible on the next visibility check.
    /// An unshared instance will be offline for between DISCOVERY_DELAY_SECS - 2 x DISCOVERY_DELAY_SECS
    async fn update_connectivity_status(
        &self,
        kube_interface: &impl KubeInterface,
        currently_visible_instances: &HashMap<String, protocols::DiscoveryResult>,
        shared: bool,
    ) -> Result<Vec<protocols::DiscoveryResult>, Box<dyn std::error::Error + Send + Sync + 'static>>
    {
        let instance_map_clone = self.instance_map.lock().await.clone();
        // Find all visible instances that do not have Instance CRDs yet
        let new_discovery_results: Vec<protocols::DiscoveryResult> = currently_visible_instances
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
                    self.instance_map
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
                            list_and_watch_message_sender: instance_info
                                .list_and_watch_message_sender,
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
                    ConnectivityStatus::Offline(instant) => {
                        let time_offline = instant.elapsed().as_secs();
                        // If instance has been offline for longer than the grace period or it is unshared, terminate the associated device plugin
                        if !shared || time_offline >= SHARED_INSTANCE_OFFLINE_GRACE_PERIOD_SECS {
                            trace!("update_connectivity_status - instance {} has been offline too long ... terminating DevicePluginService", instance);
                            device_plugin_service::terminate_device_plugin_service(
                                &instance,
                                self.instance_map.clone(),
                            )
                            .await?;
                            try_delete_instance(kube_interface, &instance, &self.config_namespace)
                                .await?;
                        }
                    }
                }
            }
        }
        Ok(new_discovery_results)
    }
}

#[cfg(test)]
mod config_action_tests {
    use super::*;
    use akri_shared::k8s::test_kube::MockKubeImpl;
    use protocols::debug_echo::{DEBUG_ECHO_AVAILABILITY_CHECK_PATH, OFFLINE};
    use std::{env, fs};
    use tempfile::Builder;
    use tokio::sync::broadcast;

    async fn build_instance_map(
        config: &KubeAkriConfig,
        visibile_discovery_results: &mut Vec<protocols::DiscoveryResult>,
        list_and_watch_message_receivers: &mut Vec<
            broadcast::Receiver<device_plugin_service::ListAndWatchMessageKind>,
        >,
        connectivity_status: ConnectivityStatus,
    ) -> InstanceMap {
        // Set env vars for getting instances
        env::set_var("AGENT_NODE_NAME", "node-a");
        env::set_var("ENABLE_DEBUG_ECHO", "yes");
        let protocol = config.spec.protocol.clone();
        let discovery_handler = protocols::get_discovery_handler(&protocol).unwrap();
        let discovery_results = discovery_handler.discover().await.unwrap();
        *visibile_discovery_results = discovery_results.clone();
        let instance_map: InstanceMap = Arc::new(Mutex::new(
            discovery_results
                .iter()
                .map(|instance_info| {
                    let (list_and_watch_message_sender, list_and_watch_message_receiver) =
                        broadcast::channel(2);
                    list_and_watch_message_receivers.push(list_and_watch_message_receiver);
                    let instance_name =
                        get_device_instance_name(&instance_info.digest, &config.metadata.name);
                    (
                        instance_name,
                        InstanceInfo {
                            list_and_watch_message_sender,
                            connectivity_status: connectivity_status.clone(),
                        },
                    )
                })
                .collect(),
        ));
        instance_map
    }

    #[tokio::test]
    async fn test_handle_config_delete() {
        let _ = env_logger::builder().is_test(true).try_init();
        let path_to_config = "../test/json/config-a.json";
        let dcc_json = fs::read_to_string(path_to_config).expect("Unable to read file");
        let config: KubeAkriConfig = serde_json::from_str(&dcc_json).unwrap();
        let config_name = config.metadata.name.clone();
        let mut list_and_watch_message_receivers = Vec::new();
        let mut visible_discovery_results = Vec::new();
        let mut mock = MockKubeImpl::new();
        let instance_map: InstanceMap = build_instance_map(
            &config,
            &mut visible_discovery_results,
            &mut list_and_watch_message_receivers,
            ConnectivityStatus::Online,
        )
        .await;
        let (stop_discovery_sender, mut stop_discovery_receiver) = mpsc::channel(2);
        let (finished_discovery_sender, _) = broadcast::channel(2);
        let mut map: HashMap<String, ConfigInfo> = HashMap::new();
        map.insert(
            config_name.clone(),
            ConfigInfo {
                stop_discovery_sender,
                instance_map: instance_map.clone(),
                finished_discovery_sender: finished_discovery_sender.clone(),
            },
        );
        let config_map: ConfigMap = Arc::new(Mutex::new(map));

        mock.expect_delete_instance()
            .times(2)
            .returning(move |_, _| Ok(()));
        tokio::spawn(async move {
            handle_config_delete(&mock, &config, config_map.clone())
                .await
                .unwrap();
            // Assert that config is removed from map after it has been deleted
            assert!(!config_map.lock().await.contains_key(&config_name));
        });

        // Assert that handle_config_delete tells do_periodic_discovery to end
        assert!(stop_discovery_receiver.recv().await.is_some());
        // Mimic do_periodic_discovery's response
        finished_discovery_sender.send(()).unwrap();

        // Assert list_and_watch is signaled to end for every instance associated with a config
        let mut tasks = Vec::new();
        for mut receiver in list_and_watch_message_receivers {
            tasks.push(tokio::spawn(async move {
                assert_eq!(
                    receiver.recv().await.unwrap(),
                    device_plugin_service::ListAndWatchMessageKind::End
                );
            }));
        }
        futures::future::join_all(tasks).await;

        // Assert that all instances have been removed from the instance map
        assert_eq!(instance_map.lock().await.len(), 0);
    }

    // 1: ConnectivityStatus of all instances that go offline is changed from Online to Offline
    // 2: ConnectivityStatus of shared instances that come back online in under 5 minutes is changed from Offline to Online
    // 3: ConnectivityStatus of unshared instances that come back online before next periodic discovery is changed from Offline to Online
    #[tokio::test]
    async fn test_update_connectivity_status() {
        let _ = env_logger::builder().is_test(true).try_init();
        let path_to_config = "../test/json/config-a.json";
        let dcc_json = fs::read_to_string(path_to_config).expect("Unable to read file");
        let config: KubeAkriConfig = serde_json::from_str(&dcc_json).unwrap();
        let config_name = config.metadata.name.clone();
        let mut list_and_watch_message_receivers = Vec::new();
        let mut visible_discovery_results = Vec::new();
        let mock = MockKubeImpl::new();

        //
        // 1: Assert that ConnectivityStatus of instance that are no longer visible is changed to Offline
        //
        let instance_map: InstanceMap = build_instance_map(
            &config,
            &mut visible_discovery_results,
            &mut list_and_watch_message_receivers,
            ConnectivityStatus::Online,
        )
        .await;
        let shared = true;
        // discover returns an empty vector when instances are offline
        let no_visible_instances: HashMap<String, protocols::DiscoveryResult> = HashMap::new();
        let periodic_dicovery = PeriodicDiscovery {
            config_name: config_name.clone(),
            config_uid: config.metadata.uid.as_ref().unwrap().clone(),
            config_namespace: config.metadata.namespace.as_ref().unwrap().clone(),
            config_spec: config.spec.clone(),
            config_protocol: config.spec.protocol.clone(),
            instance_map: instance_map.clone(),
        };
        periodic_dicovery
            .update_connectivity_status(&mock, &no_visible_instances, shared)
            .await
            .unwrap();
        let unwrapped_instance_map = instance_map.lock().await.clone();
        for (_, instance_info) in unwrapped_instance_map {
            assert_ne!(
                instance_info.connectivity_status,
                ConnectivityStatus::Online
            );
        }

        //
        // 2: Assert that ConnectivityStatus of shared instances that come back online in <5 mins is changed to Online
        //
        let instance_map: InstanceMap = build_instance_map(
            &config,
            &mut visible_discovery_results,
            &mut list_and_watch_message_receivers,
            ConnectivityStatus::Offline(Instant::now()),
        )
        .await;
        let shared = true;
        let currently_visible_instances: HashMap<String, protocols::DiscoveryResult> =
            visible_discovery_results
                .iter()
                .map(|instance_info| {
                    let instance_name =
                        get_device_instance_name(&instance_info.digest, &config_name);
                    (instance_name, instance_info.clone())
                })
                .collect();
        let periodic_dicovery = PeriodicDiscovery {
            config_name: config_name.clone(),
            config_uid: config.metadata.uid.as_ref().unwrap().clone(),
            config_namespace: config.metadata.namespace.as_ref().unwrap().clone(),
            config_spec: config.spec.clone(),
            config_protocol: config.spec.protocol.clone(),
            instance_map: instance_map.clone(),
        };
        periodic_dicovery
            .update_connectivity_status(&mock, &currently_visible_instances, shared)
            .await
            .unwrap();
        let unwrapped_instance_map = instance_map.lock().await.clone();
        for (_, instance_info) in unwrapped_instance_map {
            assert_eq!(
                instance_info.connectivity_status,
                ConnectivityStatus::Online
            );
        }

        //
        // 3: Assert that ConnectivityStatus of unshared instances that come back online before next visibility check is changed to Online
        //
        let instance_map: InstanceMap = build_instance_map(
            &config,
            &mut visible_discovery_results,
            &mut list_and_watch_message_receivers,
            ConnectivityStatus::Offline(Instant::now()),
        )
        .await;
        let shared = false;
        let periodic_dicovery = PeriodicDiscovery {
            config_name: config_name.clone(),
            config_uid: config.metadata.uid.as_ref().unwrap().clone(),
            config_namespace: config.metadata.namespace.as_ref().unwrap().clone(),
            config_spec: config.spec.clone(),
            config_protocol: config.spec.protocol.clone(),
            instance_map: instance_map.clone(),
        };
        periodic_dicovery
            .update_connectivity_status(&mock, &currently_visible_instances, shared)
            .await
            .unwrap();
        let unwrapped_instance_map = instance_map.lock().await.clone();
        for (_, instance_info) in unwrapped_instance_map {
            assert_eq!(
                instance_info.connectivity_status,
                ConnectivityStatus::Online
            );
        }
    }

    /// Checks the termination case for when an unshared instance is still offline upon the second periodic discovery
    /// Must be run independently since writing "OFFLINE" to DEBUG_ECHO_AVAILABILITY_CHECK_PATH in order to emulate
    /// offline devices can clobber other tests run in parallel that are looking for online devices.
    /// Run with: cargo test -- test_do_periodic_discovery --ignored
    #[tokio::test]
    #[ignore]
    async fn test_do_periodic_discovery() {
        let _ = env_logger::builder().is_test(true).try_init();
        // Set env vars
        env::set_var("AGENT_NODE_NAME", "node-a");
        env::set_var("ENABLE_DEBUG_ECHO", "yes");
        // Make each get_instances check return an empty list of instances
        let path_to_config = "../test/json/config-a.json";
        let dcc_json = fs::read_to_string(path_to_config).expect("Unable to read file");
        let config: KubeAkriConfig = serde_json::from_str(&dcc_json).unwrap();
        let config_name = config.metadata.name.clone();
        let mut visible_discovery_results = Vec::new();
        let mut list_and_watch_message_receivers = Vec::new();
        let (mut watch_periph_tx, watch_periph_rx) = mpsc::channel(2);
        let (finished_watching_tx, mut finished_watching_rx) = broadcast::channel(2);
        let mut mock = MockKubeImpl::new();

        // Set instance count metric to ensure it is cleared
        INSTANCE_COUNT_METRIC
            .with_label_values(&[&config_name, "false"])
            .set(2);

        // Set ConnectivityStatus of all instances in InstanceMap initially to Offline
        let instance_map: InstanceMap = build_instance_map(
            &config,
            &mut visible_discovery_results,
            &mut list_and_watch_message_receivers,
            ConnectivityStatus::Offline(Instant::now()),
        )
        .await;

        // Assert that when an unshared instance is already offline it is terminated
        mock.expect_delete_instance()
            .times(2)
            .returning(move |_, _| Ok(()));
        let instance_map_clone = instance_map.clone();
        // Change instances to be offline
        fs::write(DEBUG_ECHO_AVAILABILITY_CHECK_PATH, OFFLINE).unwrap();
        tokio::spawn(async move {
            let periodic_dicovery = PeriodicDiscovery {
                config_name: config.metadata.name,
                config_uid: config.metadata.uid.as_ref().unwrap().to_string(),
                config_namespace: config.metadata.namespace.as_ref().unwrap().to_string(),
                config_protocol: config.spec.protocol.clone(),
                config_spec: config.spec,
                instance_map: instance_map_clone,
            };
            let device_plugin_temp_dir =
                Builder::new().prefix("device-plugins-").tempdir().unwrap();
            let device_plugin_temp_dir_path = device_plugin_temp_dir.path().to_str().unwrap();
            periodic_dicovery
                .do_periodic_discovery(
                    &mock,
                    watch_periph_rx,
                    finished_watching_tx,
                    device_plugin_temp_dir_path,
                )
                .await
                .unwrap();
        });
        let mut tasks = Vec::new();
        for mut receiver in list_and_watch_message_receivers {
            tasks.push(tokio::spawn(async move {
                assert_eq!(
                    receiver.recv().await.unwrap(),
                    device_plugin_service::ListAndWatchMessageKind::End
                );
            }));
        }
        futures::future::join_all(tasks).await;

        // Assert that all instances have been removed from the instance map
        assert_eq!(instance_map.lock().await.len(), 0);

        // Assert that instance count metric is reporting no instances
        assert_eq!(
            INSTANCE_COUNT_METRIC
                .with_label_values(&[&config_name, "false"])
                .get(),
            0
        );

        watch_periph_tx.send(()).await.unwrap();
        // Assert that replies saying finished watching
        assert!(finished_watching_rx.recv().await.is_ok());

        // Reset file to be online
        fs::write(DEBUG_ECHO_AVAILABILITY_CHECK_PATH, "ONLINE").unwrap();
    }
}
