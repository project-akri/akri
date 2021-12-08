use super::instance_action::{
    create_pod_context, do_pod_action_for_nodes, handle_instance_change_job, InstanceAction,
    PodContext,
};
use super::pod_action::PodAction;
use akri_shared::{
    akri::configuration::{BrokerSpec, Configuration},
    akri::configuration_state::{should_recreate_config, ConfigState},
    akri::instance::Instance,
    k8s,
    k8s::{pod, KubeInterface},
};
use futures::{StreamExt, TryStreamExt};
use k8s_openapi::api::batch::v1::JobSpec;
use k8s_openapi::api::core::v1::PodSpec;
use kube::api::{Api, ListParams};
use kube_runtime::watcher::{watcher, Event};
use log::{info, trace};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;

// Map of Configuration name and Current Configuration state
type ConfigMap = Arc<RwLock<HashMap<String, ConfigState>>>;

/// This handles pre-existing Configurations and invokes an internal method that watches for Configuration events.
pub async fn do_config_watch() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    info!("do_config_watch - enter");
    let config_map: ConfigMap = Arc::new(RwLock::new(HashMap::new()));
    let kube_interface = k8s::KubeImpl::new().await?;
    let mut tasks = Vec::new();

    // Handle pre-existing configs
    let pre_existing_configs = kube_interface.get_configurations().await?;
    for config in pre_existing_configs {
        let config_map = config_map.clone();
        tasks.push(tokio::spawn(async move {
            handle_config(
                &k8s::KubeImpl::new().await.unwrap(),
                Event::Applied(config),
                config_map,
                &mut false,
            )
            .await
            .unwrap();
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
    let resource = Api::<Configuration>::all(kube_interface.get_kube_client());
    let watcher = watcher(resource, ListParams::default());
    let mut informer = watcher.boxed();
    let mut first_event = true;
    // Currently, this does not handle None except to break the
    // while.
    while let Some(event) = informer.try_next().await? {
        handle_config(kube_interface, event, config_map.clone(), &mut first_event).await?
    }
    Ok(())
}
async fn get_entry(config_name: &str, config_map: ConfigMap) -> Option<ConfigState> {
    config_map.read().await.get(config_name).cloned()
}

/// This takes an event off the Configuration stream and delegates it to the
/// correct function based on the event type.
async fn handle_config(
    kube_interface: &impl KubeInterface,
    event: Event<Configuration>,
    config_map: ConfigMap,
    first_event: &mut bool,
) -> anyhow::Result<()> {
    trace!("handle_config - something happened to a configuration");
    match event {
        Event::Applied(config) => {
            // Applied events can either be newly added Configurations or modified Configurations.
            info!(
                "handle_config - added or modified Configuration {:?}",
                config.metadata.name.as_ref().unwrap(),
            );
            let config_state = ConfigState {
                last_generation: config.metadata.generation,
                last_configuration_spec: config.spec.clone(),
            };
            // Check the Configuration map to see if the Configuration has been updated and
            // brokers and services need to be redeployed.
            match get_entry(config.metadata.name.as_ref().unwrap(), config_map.clone()).await {
                Some(config_state) => match &config.spec.broker_spec {
                    None => {
                        config_map.write().await.insert(
                            config.metadata.name.as_ref().unwrap().to_string(),
                            config_state,
                        );
                    }
                    Some(broker) => {
                        if !should_recreate_config(&config, &config_state) {
                            config_map.write().await.insert(
                                config.metadata.name.as_ref().unwrap().to_string(),
                                config_state,
                            );
                            match broker {
                                BrokerSpec::BrokerPodSpec(p) => {
                                    handle_broker_updates_for_configuration_pod(
                                        kube_interface,
                                        &config,
                                        p,
                                    )
                                    .await?;
                                }
                                BrokerSpec::BrokerJobSpec(j) => {
                                    handle_broker_updates_for_configuration_job(
                                        kube_interface,
                                        *config.metadata.generation.as_ref().unwrap(),
                                        &config,
                                        j,
                                    )
                                    .await?;
                                }
                            }
                        }
                    }
                },
                None => {
                    config_map.write().await.insert(
                        config.metadata.name.as_ref().unwrap().to_string(),
                        config_state,
                    );
                }
            }
        }
        Event::Deleted(config) => {
            info!(
                "handle_config - deleted Configuration {:?}",
                config.metadata.name,
            );
            // Remove Configuration from map
            config_map
                .write()
                .await
                .remove(config.metadata.name.as_ref().unwrap());
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

async fn handle_broker_updates_for_configuration_pod(
    kube_interface: &impl k8s::KubeInterface,
    config: &Configuration,
    podspec: &PodSpec,
) -> anyhow::Result<()> {
    trace!("handle_broker_updates_for_configuration_pod - entered");
    // Get all pods of this Configuration and store them in a mapping of Instance names to Node to PodContext map
    let mut configuration_pods: HashMap<String, HashMap<String, PodContext>> = HashMap::new();
    kube_interface
        .find_pods_with_label(&format!(
            "{}={}",
            pod::AKRI_CONFIGURATION_LABEL_NAME,
            config.metadata.name.as_ref().unwrap()
        ))
        .await?
        .into_iter()
        .try_for_each(|p| -> anyhow::Result<()> {
            let instance_name = p
                .metadata
                .labels
                .as_ref()
                .unwrap()
                .get(pod::AKRI_INSTANCE_LABEL_NAME)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "No instance label found for Pod {}",
                        p.metadata.name.as_ref().unwrap()
                    )
                })?
                .clone();
            let c = create_pod_context(&p, PodAction::RemoveAndAdd)?;
            configuration_pods
                .entry(instance_name)
                .and_modify(|m| {
                    m.insert(c.node_name.as_ref().unwrap().to_string(), c.clone());
                })
                .or_insert_with(|| {
                    let mut m = HashMap::new();
                    m.insert(c.node_name.as_ref().unwrap().to_string(), c);
                    m
                });
            Ok(())
        })?;

    // Find all Instances of the Configuration
    let instances: HashMap<String, Instance> = kube_interface
        .get_instances()
        .await?
        .into_iter()
        .filter(|i| &i.spec.configuration_name == config.metadata.name.as_ref().unwrap())
        .map(|i| (i.metadata.name.as_ref().unwrap().to_string(), i))
        .collect();

    // For each Instance, delete and add broker Pods
    let futures: Vec<_> = configuration_pods
        .into_iter()
        .filter_map(|(i_name, m)| {
            instances
                .get(&i_name)
                .map(|instance| do_pod_action_for_nodes(m, instance, podspec, kube_interface))
        })
        .collect();

    futures::future::try_join_all(futures).await?;
    Ok(())
}

async fn handle_broker_updates_for_configuration_job(
    kube_interface: &impl k8s::KubeInterface,
    config_generation: i64,
    config: &Configuration,
    job_spec: &JobSpec,
) -> anyhow::Result<()> {
    trace!("handle_broker_updates_for_configuration_job - entered");
    // Find all Instances of the Configuration
    let namespace = config.metadata.namespace.as_ref().unwrap();
    let instances = kube_interface
        .get_instances()
        .await?
        .into_iter()
        .filter(|i| &i.spec.configuration_name == config.metadata.name.as_ref().unwrap())
        .collect::<Vec<Instance>>();

    // Delete all Jobs labeled with this Configuration
    kube_interface
        .delete_jobs_with_label(
            Some(format!(
                "{}={}",
                pod::AKRI_CONFIGURATION_LABEL_NAME,
                config.metadata.name.as_ref().unwrap()
            )),
            namespace,
        )
        .await?;

    trace!("handle_broker_updates_for_configuration_job - deleted jobs and now recreating them");
    trace!("instances are {:?}", instances);
    // Recreate a Job for each Instance
    let futures: Vec<_> = instances
        .into_iter()
        .map(|i| {
            handle_instance_change_job(
                i,
                config_generation,
                job_spec,
                &InstanceAction::Add,
                kube_interface,
            )
        })
        .collect();
    futures::future::try_join_all(futures).await?;
    Ok(())
}

#[cfg(test)]
mod config_action_tests {
    use super::*;
    use akri_shared::{akri::configuration::Configuration, k8s::MockKubeInterface, os::file};
    use kube::api::ObjectList;

    // Test that when a Configuration's BrokerSpec is updated, the
    #[tokio::test]
    async fn test_handle_config_updated() {
        let _ = env_logger::builder().is_test(true).try_init();
        let path_to_config = "../test/yaml/config-b.yaml";
        let config_yaml = std::fs::read_to_string(path_to_config).expect("Unable to read file");
        let config: Configuration = serde_yaml::from_str(&config_yaml).unwrap();
        let mut config_map = HashMap::new();
        let mut last_configuration_spec = config.spec.clone();
        last_configuration_spec.broker_spec.as_mut().map(|b| {
            if let BrokerSpec::BrokerJobSpec(j) = b {
                j.parallelism = Some(5);
            } else {
                panic!("Expected Configuration to contain JobSpec");
            }
        });
        config_map.insert(
            "config-b".to_string(),
            ConfigState {
                last_generation: Some(0),
                last_configuration_spec,
            },
        );
        let mut mock = MockKubeInterface::new();
        // Return one instance for the Configuration
        mock.expect_get_instances().times(1).returning(move || {
            let instances_json = file::read_file_to_string("../test/json/local-instance-list.json");
            let is: ObjectList<Instance> = serde_json::from_str(&instances_json).unwrap();
            Ok(is)
        });
        mock.expect_delete_jobs_with_label()
            .times(1)
            .returning(|_, _| Ok(()));
        // create job
        mock.expect_create_job().times(1).returning(|_, _| Ok(()));
        handle_config(
            &mock,
            Event::Applied(config),
            Arc::new(RwLock::new(config_map)),
            &mut false,
        )
        .await
        .unwrap();
    }

    // Test that when a Configuration is deleted, it is removed from the ConfigMap
    #[tokio::test]
    async fn test_handle_config_deleted() {
        let _ = env_logger::builder().is_test(true).try_init();
        let path_to_config = "../test/yaml/config-b.yaml";
        let config_yaml = std::fs::read_to_string(path_to_config).expect("Unable to read file");
        let config: Configuration = serde_yaml::from_str(&config_yaml).unwrap();
        let mut config_map = HashMap::new();
        config_map.insert(
            "config-b".to_string(),
            ConfigState {
                last_generation: Some(0),
                last_configuration_spec: config.spec.clone(),
            },
        );
        let shared_map = Arc::new(RwLock::new(config_map));

        handle_config(
            &MockKubeInterface::new(),
            Event::Deleted(config),
            shared_map.clone(),
            &mut false,
        )
        .await
        .unwrap();

        assert!(shared_map.read().await.is_empty())
    }
}
