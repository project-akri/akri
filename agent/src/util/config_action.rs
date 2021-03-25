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
    akri::{configuration::KubeAkriConfig, API_CONFIGURATIONS, API_NAMESPACE, API_VERSION},
    k8s,
    k8s::{try_delete_instance, KubeInterface},
};
use futures::StreamExt;
use kube::api::{Informer, RawApi, WatchEvent};
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
}

/// This handles pre-existing Configurations and invokes an internal method that watches for Configuration events.
pub async fn do_config_watch(
    discovery_handler_map: RegisteredDiscoveryHandlerMap,
    new_discovery_handler_sender: broadcast::Sender<String>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    info!("do_config_watch - enter");
    let config_map: ConfigMap = Arc::new(Mutex::new(HashMap::new()));
    let kube_interface = k8s::create_kube_interface();
    let mut tasks = Vec::new();

    // Handle pre-existing configs
    let pre_existing_configs = kube_interface.get_configurations().await?;
    for config in pre_existing_configs {
        let config_map = config_map.clone();
        let discovery_handler_map = discovery_handler_map.clone();
        let new_discovery_handler_sender = new_discovery_handler_sender.clone();
        tasks.push(tokio::spawn(async move {
            handle_config_add(
                Arc::new(Box::new(k8s::create_kube_interface())),
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
            let new_discovery_handler_sender = new_discovery_handler_sender.clone();
            handle_config(
                kube_interface,
                event?,
                config_map.clone(),
                discovery_handler_map.clone(),
                new_discovery_handler_sender,
            )
            .await?
        }
    }
}

/// This takes an event off the Configuration stream and delegates it to the
/// correct function based on the event type.
async fn handle_config(
    kube_interface: &impl KubeInterface,
    event: WatchEvent<KubeAkriConfig>,
    config_map: ConfigMap,
    discovery_handler_map: RegisteredDiscoveryHandlerMap,
    new_discovery_handler_sender: broadcast::Sender<String>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    trace!("handle_config - something happened to a configuration");
    match event {
        WatchEvent::Added(config) => {
            info!(
                "handle_config - added Configuration {}",
                config.metadata.name
            );
            tokio::spawn(async move {
                handle_config_add(
                    Arc::new(Box::new(k8s::create_kube_interface())),
                    &config,
                    config_map,
                    discovery_handler_map,
                    new_discovery_handler_sender,
                )
                .await
                .unwrap();
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
                handle_config_add(
                    Arc::new(Box::new(k8s::create_kube_interface())),
                    &config,
                    config_map,
                    discovery_handler_map,
                    new_discovery_handler_sender,
                )
                .await
                .unwrap();
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
    kube_interface: Arc<Box<dyn k8s::KubeInterface>>,
    config: &KubeAkriConfig,
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
    };
    config_map
        .lock()
        .await
        .insert(config_name.clone(), config_info);

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
        .is_ok()
    {
        config_map
            .lock()
            .await
            .get_mut(&config.metadata.name)
            .unwrap()
            .finished_discovery_receiver
            .recv()
            .await
            .unwrap();
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
    delete_all_instances_in_map(kube_interface, instance_map, config).await?;
    Ok(())
}

/// This shuts down all a Configuration's Instances and terminates the associated Device Plugins
pub async fn delete_all_instances_in_map(
    kube_interface: &impl k8s::KubeInterface,
    instance_map: InstanceMap,
    config: &KubeAkriConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
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

#[cfg(test)]
mod config_action_tests {
    use super::super::{
        device_plugin_service,
        device_plugin_service::{InstanceConnectivityStatus, InstanceMap},
        discovery_operator::tests::{add_discovery_handler_to_map, build_instance_map},
        registration::{DiscoveryHandlerEndpoint, DiscoveryHandlerStatus},
    };
    use super::*;
    use akri_discovery_utils::discovery::{mock_discovery_handler, v0::Device};
    use akri_shared::{akri::configuration::KubeAkriConfig, k8s::MockKubeInterface};
    use std::{collections::HashMap, fs, sync::Arc};
    use tokio::sync::{broadcast, Mutex};

    #[tokio::test]
    async fn test_handle_config_delete() {
        let _ = env_logger::builder().is_test(true).try_init();
        let path_to_config = "../test/yaml/config-a.yaml";
        let config_yaml = fs::read_to_string(path_to_config).expect("Unable to read file");
        let config: KubeAkriConfig = serde_yaml::from_str(&config_yaml).unwrap();
        let config_name = config.metadata.name.clone();
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
        let (mut finished_discovery_sender, finished_discovery_receiver) = mpsc::channel(2);
        let mut map: HashMap<String, ConfigInfo> = HashMap::new();
        map.insert(
            config_name.clone(),
            ConfigInfo {
                stop_discovery_sender,
                instance_map: instance_map.clone(),
                finished_discovery_receiver,
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

    async fn run_and_test_handle_config_add(
        discovery_handler_map: RegisteredDiscoveryHandlerMap,
        config_map: ConfigMap,
        config: KubeAkriConfig,
        dh_endpoint: &DiscoveryHandlerEndpoint,
        dh_name: &str,
    ) -> tokio::task::JoinHandle<()> {
        let (new_discovery_handler_sender, _) = broadcast::channel(1);
        let mut mock_kube_interface = MockKubeInterface::new();
        mock_kube_interface
            .expect_create_instance()
            .times(1)
            .returning(move |_, _, _, _, _| Ok(()));
        let arc_mock_kube_interface: Arc<Box<dyn k8s::KubeInterface>> =
            Arc::new(Box::new(mock_kube_interface));
        let config_add_config = config.clone();
        let config_add_config_map = config_map.clone();
        let config_add_discovery_handler_map = discovery_handler_map.clone();
        let handle = tokio::spawn(async move {
            handle_config_add(
                arc_mock_kube_interface,
                &config_add_config,
                config_add_config_map,
                config_add_discovery_handler_map,
                new_discovery_handler_sender,
            )
            .await
            .unwrap();
        });

        // Loop until the Configuration and single discovered Instance are added to the ConfigMap
        let mut x: i8 = 0;
        while x < 5 {
            tokio::time::delay_for(std::time::Duration::from_millis(200)).await;
            if let Some(config_info) = config_map.lock().await.get(&config.metadata.name) {
                if config_info.instance_map.lock().await.len() == 1 {
                    break;
                }
            }
            x += 1;
        }
        assert_ne!(x, 4);
        // Assert that Discovery Handler is marked as Active
        check_discovery_handler_status(
            discovery_handler_map,
            dh_name,
            dh_endpoint,
            DiscoveryHandlerStatus::Active,
        )
        .await;
        handle
    }

    async fn check_discovery_handler_status(
        discovery_handler_map: RegisteredDiscoveryHandlerMap,
        dh_name: &str,
        dh_endpoint: &DiscoveryHandlerEndpoint,
        dh_status: DiscoveryHandlerStatus,
    ) {
        let mut x: i8 = 0;
        while x < 5 {
            tokio::time::delay_for(std::time::Duration::from_millis(200)).await;
            let dh_map = discovery_handler_map.lock().unwrap();
            if let Some(dh_details_map) = dh_map.get(dh_name) {
                if dh_details_map.get(dh_endpoint).unwrap().connectivity_status == dh_status {
                    break;
                }
            }
            x += 1;
        }
        assert_ne!(x, 4);
    }

    // Tests that when a Configuration is added, deleted, and added again,
    // instances are created, deleted and recreated,
    // and the Discovery Handler is marked as Active, Waiting, Active, and Waiting.
    // Also asserts that all threads are successfully terminated.
    #[tokio::test]
    async fn test_handle_config_add_delete_add() {
        let _ = env_logger::builder().is_test(true).try_init();

        // Set up Discovery Handler
        // Start a mock DH, specifying that it should NOT return an error
        let return_error = false;
        let (endpoint_dir, endpoint) =
            mock_discovery_handler::get_mock_discovery_handler_dir_and_endpoint("mock.sock");
        let dh_endpoint = DiscoveryHandlerEndpoint::Uds(endpoint.to_string());
        let device_id = "device_id";
        let _dh_server_thread_handle = mock_discovery_handler::run_mock_discovery_handler(
            &endpoint_dir,
            &endpoint,
            return_error,
            vec![Device {
                id: device_id.to_string(),
                properties: HashMap::new(),
                mounts: Vec::default(),
                device_specs: Vec::default(),
            }],
        )
        .await;
        // Make sure registration server has started
        akri_shared::uds::unix_stream::try_connect(&endpoint)
            .await
            .unwrap();

        // Add Discovery Handler to map
        let dh_name = "debugEcho";
        let discovery_handler_map = Arc::new(std::sync::Mutex::new(HashMap::new()));
        add_discovery_handler_to_map(dh_name, &dh_endpoint, false, discovery_handler_map.clone());

        // Set up, run, and test handle_config_add
        // Discovery Handler should create an instance and be marked as Active
        let path_to_config = "../test/yaml/config-a.yaml";
        let config_yaml = fs::read_to_string(path_to_config).expect("Unable to read file");
        let config: KubeAkriConfig = serde_yaml::from_str(&config_yaml).unwrap();
        let config_name = config.metadata.name.clone();
        let config_map: ConfigMap = Arc::new(Mutex::new(HashMap::new()));
        let first_add_handle = run_and_test_handle_config_add(
            discovery_handler_map.clone(),
            config_map.clone(),
            config.clone(),
            &dh_endpoint,
            dh_name,
        )
        .await;

        let config_delete_config = config.clone();
        let config_delete_config_map = config_map.clone();
        handle_config_delete(
            &MockKubeInterface::new(),
            &config_delete_config,
            config_delete_config_map.clone(),
        )
        .await
        .unwrap();

        // Assert that config is removed from map after it has been deleted
        assert!(!config_delete_config_map
            .lock()
            .await
            .contains_key(&config_name));

        // Assert that Discovery Handler is marked as Waiting
        check_discovery_handler_status(
            discovery_handler_map.clone(),
            dh_name,
            &dh_endpoint,
            DiscoveryHandlerStatus::Waiting,
        )
        .await;

        let second_add_handle = run_and_test_handle_config_add(
            discovery_handler_map.clone(),
            config_map.clone(),
            config.clone(),
            &dh_endpoint,
            dh_name,
        )
        .await;

        // Assert that Discovery Handler is marked as Waiting
        check_discovery_handler_status(
            discovery_handler_map.clone(),
            dh_name,
            &dh_endpoint,
            DiscoveryHandlerStatus::Waiting,
        )
        .await;

        futures::future::join_all(vec![first_add_handle, second_add_handle]).await;
    }
}
