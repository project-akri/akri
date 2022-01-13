use super::{
    constants::{
        DISCOVERY_OPERATOR_FINISHED_DISCOVERY_CHANNEL_CAPACITY,
        DISCOVERY_OPERATOR_STOP_DISCOVERY_CHANNEL_CAPACITY,
    },
    device_plugin_service,
    device_plugin_service::InstanceMap,
    discovery_operator::start_discovery::{start_discovery, DiscoveryOperator},
    registration::RegisteredDiscoveryHandlerMap,
};
use akri_shared::{
    akri::configuration::Configuration,
    k8s,
    k8s::{try_delete_instance, KubeInterface},
};
use futures::{StreamExt, TryStreamExt};
use kube::api::{Api, ListParams};
use kube_runtime::watcher::{watcher, Event};
use log::{info, trace};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{broadcast, mpsc, Mutex};

type ConfigMap = Arc<Mutex<HashMap<String, ConfigInfo>>>;

/// Information for managing a Configuration, such as all applied Instances of that Configuration
/// and senders for ceasing to discover instances upon Configuration deletion.
#[derive(Debug)]
pub struct ConfigInfo {
    /// Map of all of a Configuration's Instances
    instance_map: InstanceMap,
    /// Sends notification to a `DiscoveryOperator` that it should stop all discovery for its Configuration.
    /// This signals it to tell each of its subtasks to stop discovery.
    /// A broadcast channel is used so both the sending and receiving ends can be cloned.
    stop_discovery_sender: broadcast::Sender<()>,
    /// Receives notification that all `DiscoveryOperators` threads have completed and a Configuration's Instances
    /// can be safely deleted and the associated `DevicePluginServices` terminated.
    finished_discovery_receiver: mpsc::Receiver<()>,
    /// Tracks the last generation of the `Configuration` resource (i.e. `.metadata.generation`).
    /// This is used to determine if the `Configuration` actually changed, or if only the metadata changed.
    /// The `.metadata.generation` value is incremented for all changes, except for changes to `.metadata` or `.status`.
    last_generation: Option<i64>,
}

/// This handles pre-existing Configurations and invokes an internal method that watches for Configuration events.
pub async fn do_config_watch(
    discovery_handler_map: RegisteredDiscoveryHandlerMap,
    new_discovery_handler_sender: broadcast::Sender<String>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    info!("do_config_watch - enter");
    let config_map: ConfigMap = Arc::new(Mutex::new(HashMap::new()));
    let kube_interface = k8s::KubeImpl::new().await?;
    let mut tasks = Vec::new();

    // Handle pre-existing configs
    let pre_existing_configs = kube_interface.get_configurations().await?;
    for config in pre_existing_configs {
        let config_map = config_map.clone();
        let discovery_handler_map = discovery_handler_map.clone();
        let new_discovery_handler_sender = new_discovery_handler_sender.clone();
        tasks.push(tokio::spawn(async move {
            handle_config_add(
                Arc::new(k8s::KubeImpl::new().await.unwrap()),
                &config,
                config_map,
                discovery_handler_map,
                new_discovery_handler_sender,
            )
            .await
            .unwrap();
        }));
    }

    // Watch for new configs and changes
    tasks.push(tokio::spawn(async move {
        watch_for_config_changes(
            &kube_interface,
            config_map,
            discovery_handler_map,
            new_discovery_handler_sender,
        )
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
    discovery_handler_map: RegisteredDiscoveryHandlerMap,
    new_discovery_handler_sender: broadcast::Sender<String>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    trace!("watch_for_config_changes - start");
    let resource = Api::<Configuration>::all(kube_interface.get_kube_client());
    let watcher = watcher(resource, ListParams::default());
    let mut informer = watcher.boxed();
    let mut first_event = true;
    // Currently, this does not handle None except to break the
    // while.
    while let Some(event) = informer.try_next().await? {
        let new_discovery_handler_sender = new_discovery_handler_sender.clone();
        handle_config(
            kube_interface,
            event,
            config_map.clone(),
            discovery_handler_map.clone(),
            new_discovery_handler_sender,
            &mut first_event,
        )
        .await?
    }
    Ok(())
}

/// This takes an event off the Configuration stream and delegates it to the
/// correct function based on the event type.
async fn handle_config(
    kube_interface: &impl KubeInterface,
    event: Event<Configuration>,
    config_map: ConfigMap,
    discovery_handler_map: RegisteredDiscoveryHandlerMap,
    new_discovery_handler_sender: broadcast::Sender<String>,
    first_event: &mut bool,
) -> anyhow::Result<()> {
    trace!("handle_config - something happened to a configuration");
    match event {
        Event::Applied(config) => {
            info!(
                "handle_config - added or modified Configuration {:?}",
                config.metadata.name.as_ref().unwrap(),
            );
            // Applied events can either be newly added Configurations or modified Configurations.
            // If modified delete all associated instances and device plugins and then recreate them to reflect updated config
            // TODO: more gracefully handle modified Configurations by determining what changed rather than delete/re-add
            if config_map
                .lock()
                .await
                .contains_key(config.metadata.name.as_ref().unwrap())
            {
                let do_recreate = should_recreate_config(&config, config_map.clone()).await?;
                if !do_recreate {
                    trace!(
                        "handle_config - config {:?} has not changed. ignoring config modified event.",
                        config.metadata.name,
                    );
                    return Ok(());
                }
                info!(
                    "handle_config - modified Configuration {:?}",
                    config.metadata.name,
                );
                handle_config_delete(kube_interface, &config, config_map.clone()).await?;
            }

            tokio::spawn(async move {
                handle_config_add(
                    Arc::new(k8s::KubeImpl::new().await.unwrap()),
                    &config,
                    config_map,
                    discovery_handler_map,
                    new_discovery_handler_sender,
                )
                .await
                .unwrap();
            });
        }
        Event::Deleted(config) => {
            info!(
                "handle_config - deleted Configuration {:?}",
                config.metadata.name,
            );
            handle_config_delete(kube_interface, &config, config_map).await?;
        }
        Event::Restarted(_configs) => {
            if *first_event {
                info!("handle_config - watcher started");
            } else {
                return Err(anyhow::anyhow!(
                    "Configuration watcher restarted - throwing error to restart agent"
                ));
            }
        }
    }
    *first_event = false;
    Ok(())
}

/// This handles added Configuration by creating a new ConfigInfo for it and adding it to the ConfigMap.
/// Then calls a function to continually observe the availability of instances associated with the Configuration.
async fn handle_config_add(
    kube_interface: Arc<dyn k8s::KubeInterface>,
    config: &Configuration,
    config_map: ConfigMap,
    discovery_handler_map: RegisteredDiscoveryHandlerMap,
    new_discovery_handler_sender: broadcast::Sender<String>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    let config_name = config.metadata.name.clone();
    // Create a new instance map for this config and add it to the config map
    let instance_map: InstanceMap = Arc::new(Mutex::new(HashMap::new()));
    let (stop_discovery_sender, _): (broadcast::Sender<()>, broadcast::Receiver<()>) =
        broadcast::channel(DISCOVERY_OPERATOR_STOP_DISCOVERY_CHANNEL_CAPACITY);
    let (mut finished_discovery_sender, finished_discovery_receiver) =
        mpsc::channel(DISCOVERY_OPERATOR_FINISHED_DISCOVERY_CHANNEL_CAPACITY);
    let config_info = ConfigInfo {
        instance_map: instance_map.clone(),
        stop_discovery_sender: stop_discovery_sender.clone(),
        finished_discovery_receiver,
        last_generation: config.metadata.generation,
    };
    config_map
        .lock()
        .await
        .insert(config_name.clone().unwrap(), config_info);

    let config = config.clone();
    // Keep discovering instances until the config is deleted, signaled by a message from handle_config_delete
    tokio::spawn(async move {
        let discovery_operator =
            DiscoveryOperator::new(discovery_handler_map, config, instance_map);
        start_discovery(
            discovery_operator,
            new_discovery_handler_sender,
            stop_discovery_sender,
            &mut finished_discovery_sender,
            kube_interface,
        )
        .await
        .unwrap();
    })
    .await?;
    Ok(())
}

/// This handles a deleted Configuration. First, it ceases to discover instances associated with the Configuration.
/// Then, for each of the Configuration's Instances, it signals the DevicePluginService to shutdown,
/// and deletes the Instance CRD.
async fn handle_config_delete(
    kube_interface: &impl KubeInterface,
    config: &Configuration,
    config_map: ConfigMap,
) -> anyhow::Result<()> {
    let name = config.metadata.name.clone().unwrap();
    trace!(
        "handle_config_delete - for config {} telling do_periodic_discovery to end",
        name
    );
    // Send message to stop observing instances' availability and waits until response is received
    if config_map
        .lock()
        .await
        .get(&name)
        .unwrap()
        .stop_discovery_sender
        .clone()
        .send(())
        .is_ok()
    {
        config_map
            .lock()
            .await
            .get_mut(&name)
            .unwrap()
            .finished_discovery_receiver
            .recv()
            .await
            .unwrap();
        trace!(
            "handle_config_delete - for config {} received message that do_periodic_discovery ended",
            name
        );
    } else {
        trace!(
            "handle_config_delete - for config {} do_periodic_discovery receiver has been dropped",
            name
        );
    }

    // Get map of instances for the Configuration and then remove Configuration from ConfigMap
    let instance_map: InstanceMap;
    {
        let mut config_map_locked = config_map.lock().await;
        instance_map = config_map_locked.get(&name).unwrap().instance_map.clone();
        config_map_locked.remove(&name);
    }
    delete_all_instances_in_map(kube_interface, instance_map, config).await?;
    Ok(())
}

/// Checks to see if the configuration needs to be recreated.
/// At present, this just checks to see if the `.metadata.generation` has changed.
/// The `.metadata.generation` value is incremented for all changes, except for changes to `.metadata` or `.status`.
async fn should_recreate_config(
    config: &Configuration,
    config_map: ConfigMap,
) -> Result<bool, anyhow::Error> {
    let name = config.metadata.name.as_ref().unwrap();
    let last_generation = config_map
        .lock()
        .await
        .get(name)
        .ok_or_else(|| anyhow::anyhow!("Configuration {} not found in ConfigMap", &name))?
        .last_generation;

    if config.metadata.generation <= last_generation {
        return Ok(false);
    }

    Ok(true)
}

/// This shuts down all a Configuration's Instances and terminates the associated Device Plugins
pub async fn delete_all_instances_in_map(
    kube_interface: &impl k8s::KubeInterface,
    instance_map: InstanceMap,
    config: &Configuration,
) -> anyhow::Result<()> {
    let mut instance_map_locked = instance_map.lock().await;
    let instances_to_delete_map = instance_map_locked.clone();
    let namespace = config.metadata.namespace.as_ref().unwrap();
    for (instance_name, instance_info) in instances_to_delete_map {
        trace!(
            "handle_config_delete - found Instance {} associated with deleted config {:?} ... sending message to end list_and_watch",
            instance_name,
            config.metadata.name
        );
        instance_info
            .list_and_watch_message_sender
            .send(device_plugin_service::ListAndWatchMessageKind::End)
            .unwrap();
        instance_map_locked.remove(&instance_name);
        try_delete_instance(kube_interface, &instance_name, namespace).await?;
    }
    Ok(())
}

#[cfg(test)]
mod config_action_tests {
    use super::super::{
        device_plugin_service,
        device_plugin_service::{InstanceConnectivityStatus, InstanceMap},
        discovery_operator::tests::build_instance_map,
    };
    use super::*;
    use akri_shared::{akri::configuration::Configuration, k8s::MockKubeInterface};
    use std::{collections::HashMap, fs, sync::Arc};
    use tokio::sync::{broadcast, Mutex};

    // Test that watcher errors on restarts unless it is the first restart (aka initial startup)
    #[tokio::test]
    async fn test_handle_watcher_restart() {
        let _ = env_logger::builder().is_test(true).try_init();
        let config_map = Arc::new(Mutex::new(HashMap::new()));
        let dh_map = Arc::new(std::sync::Mutex::new(HashMap::new()));
        let (tx, mut _rx1) = broadcast::channel(1);
        let mut first_event = true;
        assert!(handle_config(
            &MockKubeInterface::new(),
            Event::Restarted(Vec::new()),
            config_map.clone(),
            dh_map.clone(),
            tx.clone(),
            &mut first_event
        )
        .await
        .is_ok());
        first_event = false;
        assert!(handle_config(
            &MockKubeInterface::new(),
            Event::Restarted(Vec::new()),
            config_map,
            dh_map,
            tx,
            &mut first_event
        )
        .await
        .is_err());
    }

    #[tokio::test]
    async fn test_handle_config_delete() {
        let _ = env_logger::builder().is_test(true).try_init();
        let path_to_config = "../test/yaml/config-a.yaml";
        let config_yaml = fs::read_to_string(path_to_config).expect("Unable to read file");
        let config: Configuration = serde_yaml::from_str(&config_yaml).unwrap();
        let config_name = config.metadata.name.clone().unwrap();
        let mut list_and_watch_message_receivers = Vec::new();
        let mut visible_discovery_results = Vec::new();
        let mut mock = MockKubeInterface::new();
        let instance_map: InstanceMap = build_instance_map(
            &config,
            &mut visible_discovery_results,
            &mut list_and_watch_message_receivers,
            InstanceConnectivityStatus::Online,
        )
        .await;
        let (stop_discovery_sender, mut stop_discovery_receiver) = broadcast::channel(2);
        let (finished_discovery_sender, finished_discovery_receiver) = mpsc::channel(2);
        let mut map: HashMap<String, ConfigInfo> = HashMap::new();
        map.insert(
            config_name.clone(),
            ConfigInfo {
                stop_discovery_sender,
                instance_map: instance_map.clone(),
                finished_discovery_receiver,
                last_generation: config.metadata.generation,
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

        // Assert that handle_config_delete tells start_discovery to end
        assert!(stop_discovery_receiver.recv().await.is_ok());
        // Mimic do_periodic_discovery's response
        finished_discovery_sender.send(()).await.unwrap();

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

    // Tests that when a Configuration is updated,
    // if generation has changed, should return true
    #[tokio::test]
    async fn test_should_recreate_config_new_generation() {
        let (mut config, config_map) = get_should_recreate_config_data().await;

        // using different generation as what is already in config_map
        config.metadata.generation = Some(2);
        let do_recreate = should_recreate_config(&config, config_map.clone())
            .await
            .unwrap();

        assert!(do_recreate)
    }

    // Tests that when a Configuration is updated,
    // if generation has NOT changed, should return false
    #[tokio::test]
    async fn test_should_recreate_config_same_generation() {
        let (mut config, config_map) = get_should_recreate_config_data().await;

        // using same generation as what is already in config_map
        config.metadata.generation = Some(1);
        let do_recreate = should_recreate_config(&config, config_map.clone())
            .await
            .unwrap();

        assert!(!do_recreate)
    }

    // Tests that when a Configuration is updated,
    // if generation is older, should return false
    #[tokio::test]
    async fn test_should_recreate_config_older_generation() {
        let (mut config, config_map) = get_should_recreate_config_data().await;

        // using older generation than what is already in config_map
        config.metadata.generation = Some(0);
        let do_recreate = should_recreate_config(&config, config_map.clone())
            .await
            .unwrap();

        assert!(!do_recreate)
    }

    async fn get_should_recreate_config_data() -> (Configuration, ConfigMap) {
        let path_to_config = "../test/yaml/config-a.yaml";
        let config_yaml = fs::read_to_string(path_to_config).expect("Unable to read file");
        let config: Configuration = serde_yaml::from_str(&config_yaml).unwrap();

        let (stop_discovery_sender, _) = broadcast::channel(2);
        let (_, finished_discovery_receiver) = mpsc::channel(2);

        let config_info = ConfigInfo {
            instance_map: Arc::new(Mutex::new(HashMap::new())),
            stop_discovery_sender: stop_discovery_sender.clone(),
            finished_discovery_receiver,
            last_generation: Some(1),
        };
        let config_name = config.metadata.name.clone().unwrap();
        let config_map: ConfigMap = Arc::new(Mutex::new(HashMap::new()));
        config_map.lock().await.insert(config_name, config_info);
        (config, config_map)
    }
}
