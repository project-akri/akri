use super::super::BROKER_POD_COUNT_METRIC;
use super::{pod_action::PodAction, pod_action::PodActionInfo};
use akri_shared::{
    akri::{configuration::BrokerSpec, instance::Instance, AKRI_PREFIX},
    k8s::{
        self, job, pod,
        pod::{AKRI_INSTANCE_LABEL_NAME, AKRI_TARGET_NODE_LABEL_NAME},
        KubeInterface, OwnershipInfo, OwnershipType,
    },
};
use async_std::sync::Mutex;
use futures::{StreamExt, TryStreamExt};
use k8s_openapi::api::batch::v1::JobSpec;
use k8s_openapi::api::core::v1::{Pod, PodSpec};
use kube::api::{Api, ListParams};
use kube_runtime::watcher::{default_backoff, watcher, Event};
use kube_runtime::WatchStreamExt;
use log::{error, info, trace};
use std::collections::HashMap;
use std::sync::Arc;
/// Length of time a Pod can be pending before we give up and retry
pub const PENDING_POD_GRACE_PERIOD_MINUTES: i64 = 5;
/// Length of time a Pod can be in an error state before we retry
pub const FAILED_POD_GRACE_PERIOD_MINUTES: i64 = 0;

/// Instance action types
///
/// Instance actions describe the types of actions the Controller can
/// react to for Instances.
///
/// This will determine what broker management actions to take (if any)
///
///   | --> InstanceAction::Add
///                 | --> No broker => Do nothing
///                 | --> <BrokerSpec::BrokerJobSpec> => Deploy a Job
///                 | --> <BrokerSpec::BrokerPodSpec> => Deploy Pod to each Node on Instance's `nodes` list (up to `capacity` total)
///   | --> InstanceAction::Remove
///                 | --> No broker => Do nothing
///                 | --> <BrokerSpec::BrokerJobSpec> => Delete all Jobs labeled with the Instance name
///                 | --> <BrokerSpec::BrokerPodSpec> => Delete all Pods labeled with the Instance name
///   | --> InstanceAction::Update
///                 | --> No broker => Do nothing
///                 | --> <BrokerSpec::BrokerJobSpec> => No nothing
///                 | --> <BrokerSpec::BrokerPodSpec> => Ensure that each Node on Instance's `nodes` list (up to `capacity` total) have a Pod
///
#[derive(Clone, Debug, PartialEq)]
pub enum InstanceAction {
    /// An Instance is added
    Add,
    /// An Instance is removed
    Remove,
    /// An Instance is updated
    Update,
}

/// This invokes an internal method that watches for Instance events
pub async fn handle_existing_instances(
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    internal_handle_existing_instances(&k8s::KubeImpl::new().await?).await
}

/// This invokes an internal method that watches for Instance events
pub async fn do_instance_watch(
    synchronization: Arc<Mutex<()>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    // Watch for instance changes
    internal_do_instance_watch(&synchronization, &k8s::KubeImpl::new().await?).await
}

/// This invokes an internal method that watches for Instance events
async fn internal_handle_existing_instances(
    kube_interface: &impl KubeInterface,
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    let mut tasks = Vec::new();

    // Handle existing instances
    let pre_existing_instances = kube_interface.get_instances().await?;
    for instance in pre_existing_instances {
        tasks.push(tokio::spawn(async move {
            let inner_kube_interface = k8s::KubeImpl::new().await.unwrap();
            handle_instance_change(&instance, &InstanceAction::Update, &inner_kube_interface)
                .await
                .unwrap();
        }));
    }
    futures::future::try_join_all(tasks).await?;
    Ok(())
}

async fn internal_do_instance_watch(
    synchronization: &Arc<Mutex<()>>,
    kube_interface: &impl KubeInterface,
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    trace!("internal_do_instance_watch - enter");
    let resource = Api::<Instance>::all(kube_interface.get_kube_client());
    let watcher = watcher(resource, ListParams::default()).backoff(default_backoff());
    let mut informer = watcher.boxed();
    let mut first_event = true;
    // Currently, this does not handle None except to break the loop.
    loop {
        let event = match informer.try_next().await {
            Err(e) => {
                error!("Error during watch: {}", e);
                continue;
            }
            Ok(None) => break,
            Ok(Some(event)) => event,
        };
        // Aquire lock to ensure cleanup_instance_and_configuration_svcs and the
        // inner loop handle_instance call in internal_do_instance_watch
        // cannot execute at the same time.
        let _lock = synchronization.lock().await;
        trace!("internal_do_instance_watch - aquired sync lock");
        handle_instance(event, kube_interface, &mut first_event).await?;
    }
    Ok(())
}

/// This takes an event off the Instance stream and delegates it to the
/// correct function based on the event type.
async fn handle_instance(
    event: Event<Instance>,
    kube_interface: &impl KubeInterface,
    first_event: &mut bool,
) -> anyhow::Result<()> {
    trace!("handle_instance - enter");
    match event {
        Event::Applied(instance) => {
            info!(
                "handle_instance - added or modified Akri Instance {:?}: {:?}",
                instance.metadata.name, instance.spec
            );
            // TODO: consider renaming `InstanceAction::Add` to `InstanceAction::AddOrUpdate`
            // to reflect that this could also be an Update event. Or as we do more specific
            // inspection in future, delineation may be useful.
            handle_instance_change(&instance, &InstanceAction::Add, kube_interface).await?;
        }
        Event::Deleted(instance) => {
            info!(
                "handle_instance - deleted Akri Instance {:?}: {:?}",
                instance.metadata.name, instance.spec
            );
            handle_instance_change(&instance, &InstanceAction::Remove, kube_interface).await?;
        }
        Event::Restarted(_instances) => {
            if *first_event {
                info!("handle_instance - watcher started");
            } else {
                return Err(anyhow::anyhow!(
                    "Instance watcher restarted - throwing error to restart controller"
                ));
            }
        }
    }
    *first_event = false;
    Ok(())
}

/// PodContext stores a set of details required to track/create/delete broker
/// Pods.
///
/// The PodContext stores what is required to determine how to handle a
/// specific Node's protocol broker Pod.
///
/// * the node is described by node_name
/// * the protocol (or capability) is described by instance_name and namespace
/// * what to do with the broker Pod is described by action
// TODO: add Pod name so does not need to be
// generated on deletes and remove Option wrappers.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PodContext {
    pub(crate) node_name: Option<String>,
    namespace: Option<String>,
    action: PodAction,
}

pub(crate) fn create_pod_context(k8s_pod: &Pod, action: PodAction) -> anyhow::Result<PodContext> {
    let pod_name = k8s_pod.metadata.name.as_ref().unwrap();
    let labels = &k8s_pod
        .metadata
        .labels
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("no labels found for Pod {:?}", pod_name))?;
    // Early exits above ensure unwrap will not panic
    let node_to_run_pod_on = labels.get(AKRI_TARGET_NODE_LABEL_NAME).ok_or_else(|| {
        anyhow::anyhow!(
            "no {} label found for {:?}",
            AKRI_TARGET_NODE_LABEL_NAME,
            pod_name
        )
    })?;

    Ok(PodContext {
        node_name: Some(node_to_run_pod_on.to_string()),
        namespace: k8s_pod.metadata.namespace.clone(),
        action,
    })
}

/// This finds what to do with a given broker Pod based on its current state and
/// the Instance event action.  If this method has enough information,
/// it will update the nodes_to_act_on map with the required action.
fn determine_action_for_pod(
    k8s_pod: &Pod,
    action: &InstanceAction,
    nodes_to_act_on: &mut HashMap<String, PodContext>,
) -> anyhow::Result<()> {
    let pod_name = k8s_pod.metadata.name.as_ref().unwrap();
    let pod_phase = k8s_pod
        .status
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("No pod status found for Pod {:?}", pod_name))?
        .phase
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("No pod phase found for Pod {:?}", pod_name))?;

    let mut update_pod_context = create_pod_context(k8s_pod, PodAction::NoAction)?;
    let node_to_run_pod_on = update_pod_context.node_name.as_ref().unwrap();

    // Early exits above ensure unwrap will not panic
    let pod_start_time = k8s_pod.status.as_ref().unwrap().start_time.clone();

    let pod_action_info = PodActionInfo {
        pending_grace_time_in_minutes: PENDING_POD_GRACE_PERIOD_MINUTES,
        ended_grace_time_in_minutes: FAILED_POD_GRACE_PERIOD_MINUTES,
        phase: pod_phase.to_string(),
        instance_action: action.clone(),
        status_start_time: pod_start_time,
        unknown_node: !nodes_to_act_on.contains_key(node_to_run_pod_on),
        trace_node_name: k8s_pod.metadata.name.clone().unwrap(),
    };
    update_pod_context.action = pod_action_info.select_pod_action()?;
    nodes_to_act_on.insert(node_to_run_pod_on.to_string(), update_pod_context);
    Ok(())
}

/// This handles Instance deletion event by deleting the
/// broker Pod, the broker Service (if there are no remaining broker Pods),
/// and the capability Service (if there are no remaining capability Pods).
async fn handle_deletion_work(
    instance_name: &str,
    configuration_name: &str,
    instance_shared: bool,
    node_to_delete_pod: &str,
    context: &PodContext,
    kube_interface: &impl KubeInterface,
) -> anyhow::Result<()> {
    let context_node_name = context.node_name.as_ref().ok_or_else(|| {
        anyhow::anyhow!(
            "handle_deletion_work - Context node_name is missing for {}: {:?}",
            node_to_delete_pod,
            context
        )
    })?;
    let context_namespace = context.namespace.as_ref().ok_or_else(|| {
        anyhow::anyhow!(
            "handle_deletion_work - Context namespace is missing for {}: {:?}",
            node_to_delete_pod,
            context
        )
    })?;

    trace!(
        "handle_deletion_work - pod::create_broker_app_name({:?}, {:?}, {:?}, {:?})",
        &instance_name,
        context_node_name,
        instance_shared,
        "pod"
    );
    let pod_app_name = pod::create_broker_app_name(
        instance_name,
        Some(context_node_name),
        instance_shared,
        "pod",
    );
    trace!(
        "handle_deletion_work - pod::remove_pod name={:?}, namespace={:?}",
        &pod_app_name,
        &context_namespace
    );
    kube_interface
        .remove_pod(&pod_app_name, context_namespace)
        .await?;
    trace!("handle_deletion_work - pod::remove_pod succeeded",);
    BROKER_POD_COUNT_METRIC
        .with_label_values(&[configuration_name, context_node_name])
        .dec();
    Ok(())
}

#[cfg(test)]
mod handle_deletion_work_tests {
    use super::*;
    use akri_shared::k8s::MockKubeInterface;

    #[tokio::test]
    async fn test_handle_deletion_work_with_no_node_name() {
        let _ = env_logger::builder().is_test(true).try_init();

        let context = PodContext {
            node_name: None,
            namespace: Some("namespace".into()),
            action: PodAction::NoAction,
        };

        assert!(handle_deletion_work(
            "instance_name",
            "configuration_name",
            true,
            "node_to_delete_pod",
            &context,
            &MockKubeInterface::new(),
        )
        .await
        .is_err());
    }

    #[tokio::test]
    async fn test_handle_deletion_work_with_no_namespace() {
        let _ = env_logger::builder().is_test(true).try_init();

        let context = PodContext {
            node_name: Some("node-a".into()),
            namespace: None,
            action: PodAction::NoAction,
        };

        assert!(handle_deletion_work(
            "instance_name",
            "configuration_name",
            true,
            "node_to_delete_pod",
            &context,
            &MockKubeInterface::new(),
        )
        .await
        .is_err());
    }
}

/// This handles Instance addition event by creating the
/// broker Pod, the broker Service, and the capability Service.
/// TODO: reduce parameters by passing Instance object instead of
/// individual fields
#[allow(clippy::too_many_arguments)]
async fn handle_addition_work(
    instance_name: &str,
    instance_uid: &str,
    instance_namespace: &str,
    instance_class_name: &str,
    instance_shared: bool,
    new_node: &str,
    podspec: &PodSpec,
    kube_interface: &impl KubeInterface,
) -> anyhow::Result<()> {
    trace!(
        "handle_addition_work - Create new Pod for Node={:?}",
        new_node
    );
    let capability_id = format!("{}/{}", AKRI_PREFIX, instance_name);
    let new_pod = pod::create_new_pod_from_spec(
        instance_namespace,
        instance_name,
        instance_class_name,
        OwnershipInfo::new(
            OwnershipType::Instance,
            instance_name.to_string(),
            instance_uid.to_string(),
        ),
        &capability_id,
        new_node,
        instance_shared,
        podspec,
    )?;

    trace!("handle_addition_work - New pod spec={:?}", new_pod);

    kube_interface
        .create_pod(&new_pod, instance_namespace)
        .await?;
    trace!("handle_addition_work - pod::create_pod succeeded",);
    BROKER_POD_COUNT_METRIC
        .with_label_values(&[instance_class_name, new_node])
        .inc();

    Ok(())
}

/// Handle Instance change by
/// 1) checking to make sure the Instance's Configuration exists
/// 2) calling the appropriate handler depending on the broker type (Pod or Job) if any
pub async fn handle_instance_change(
    instance: &Instance,
    action: &InstanceAction,
    kube_interface: &impl KubeInterface,
) -> anyhow::Result<()> {
    trace!("handle_instance_change - enter {:?}", action);
    let instance_name = instance.metadata.name.clone().unwrap();
    let instance_namespace =
        instance.metadata.namespace.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Namespace not found for instance: {}", &instance_name)
        })?;
    let configuration = match kube_interface
        .find_configuration(&instance.spec.configuration_name, instance_namespace)
        .await
    {
        Ok(config) => config,
        _ => {
            if action != &InstanceAction::Remove {
                // In this scenario, a configuration has been deleted without the Akri Agent deleting the associated Instances.
                // Furthermore, Akri Agent is still modifying the Instances. This should not happen beacuse Agent
                // is designed to shutdown when it's Configuration watcher fails.
                error!(
                        "handle_instance_change - no configuration found for {:?} yet instance {:?} exists - check that device plugin is running properly",
                        &instance.spec.configuration_name, &instance.metadata.name
                    );
            }
            return Ok(());
        }
    };
    if let Some(broker_spec) = &configuration.spec.broker_spec {
        let instance_change_result = match broker_spec {
            BrokerSpec::BrokerPodSpec(p) => {
                handle_instance_change_pod(instance, p, action, kube_interface).await
            }
            BrokerSpec::BrokerJobSpec(j) => {
                handle_instance_change_job(
                    instance,
                    *configuration.metadata.generation.as_ref().unwrap(),
                    j,
                    action,
                    kube_interface,
                )
                .await
            }
        };
        if let Err(e) = instance_change_result {
            error!("Unable to handle Broker action: {:?}", e);
        }
    }
    Ok(())
}

/// Called when an Instance has changed that requires a Job broker. Action determined by InstanceAction.
/// InstanceAction::Add =>  Deploy a Job with JobSpec from Configuration. Label with Instance name.
/// InstanceAction::Remove => Delete all Jobs labeled with the Instance name
/// InstanceAction::Update => No nothing
pub async fn handle_instance_change_job(
    instance: &Instance,
    config_generation: i64,
    job_spec: &JobSpec,
    action: &InstanceAction,
    kube_interface: &impl KubeInterface,
) -> anyhow::Result<()> {
    trace!("handle_instance_change_job - enter {:?}", action);
    // Create name for Job. Includes Configuration generation in the suffix
    // to track what version of the Configuration the Job is associated with.
    let job_name = pod::create_broker_app_name(
        instance.metadata.name.as_ref().unwrap(),
        None,
        instance.spec.shared,
        &format!("{}-job", config_generation),
    );

    let instance_name = instance.metadata.name.as_ref().unwrap();
    let instance_namespace = instance.metadata.namespace.as_ref().unwrap();
    let instance_uid = instance
        .metadata
        .uid
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("UID not found for instance: {}", &instance_name))?;
    match action {
        InstanceAction::Add => {
            trace!("handle_instance_change_job - instance added");
            let capability_id = format!("{}/{}", AKRI_PREFIX, instance_name);
            let new_job = job::create_new_job_from_spec(
                instance,
                OwnershipInfo::new(
                    OwnershipType::Instance,
                    instance_name.to_string(),
                    instance_uid.to_string(),
                ),
                &capability_id,
                job_spec,
                &job_name,
            )?;
            kube_interface
                .create_job(&new_job, instance_namespace)
                .await?;
        }
        InstanceAction::Remove => {
            trace!("handle_instance_change_job - instance removed");
            // Find all jobs with the label
            let instance_jobs = kube_interface
                .find_jobs_with_label(&format!("{}={}", AKRI_INSTANCE_LABEL_NAME, instance_name))
                .await?;
            let delete_tasks = instance_jobs.into_iter().map(|j| async move {
                kube_interface
                    .remove_job(
                        j.metadata.name.as_ref().unwrap(),
                        j.metadata.namespace.as_ref().unwrap(),
                    )
                    .await
            });

            futures::future::try_join_all(delete_tasks).await?;
        }
        InstanceAction::Update => {
            trace!("handle_instance_change_job - instance updated");
            // TODO: Broker could have encountered unexpected admission error and need to be removed and added
        }
    }
    Ok(())
}

/// Called when an Instance has changed that requires a Pod broker.
/// Action determined by InstanceAction and changes to the Instance's `nodes` list.
/// Starts broker Pods that are missing and stops Pods that are no longer needed.
/// InstanceAction::Add =>  Deploy Pod to each Node on Instance's `nodes` list (up to `capacity` total)
/// InstanceAction::Remove => Delete all Pods labeled with the Instance name
/// InstanceAction::Update => Ensure that each Node on Instance's `nodes` list (up to `capacity` total) have a Pod
pub async fn handle_instance_change_pod(
    instance: &Instance,
    podspec: &PodSpec,
    action: &InstanceAction,
    kube_interface: &impl KubeInterface,
) -> anyhow::Result<()> {
    trace!("handle_instance_change_pod - enter {:?}", action);

    let instance_name = instance.metadata.name.clone().unwrap();

    // If InstanceAction::Remove, assume all nodes require PodAction::NoAction (reflect that there is no running Pod unless we find one)
    // Otherwise, assume all nodes require PodAction::Add (reflect that there is no running Pod, unless we find one)
    let default_action = match action {
        InstanceAction::Remove => PodAction::NoAction,
        _ => PodAction::Add,
    };
    let mut nodes_to_act_on: HashMap<String, PodContext> = instance
        .spec
        .nodes
        .iter()
        .map(|node| {
            (
                node.to_string(),
                PodContext {
                    node_name: None,
                    namespace: None,
                    action: default_action,
                },
            )
        })
        .collect();
    trace!(
        "handle_instance_change - nodes tracked from instance={:?}",
        nodes_to_act_on
    );

    trace!(
        "handle_instance_change - find all pods that have {}={}",
        AKRI_INSTANCE_LABEL_NAME,
        instance_name
    );
    let instance_pods = kube_interface
        .find_pods_with_label(&format!("{}={}", AKRI_INSTANCE_LABEL_NAME, instance_name))
        .await?;
    trace!(
        "handle_instance_change - found {} pods",
        instance_pods.items.len()
    );

    trace!("handle_instance_change - update actions based on the existing pods");
    // By default, assume any pod tracked by the instance need to be added.
    // Query the existing pods to see if some of these are already added, or
    // need to be removed
    instance_pods
        .items
        .iter()
        .try_for_each(|x| determine_action_for_pod(x, action, &mut nodes_to_act_on))?;

    trace!(
        "handle_instance_change - nodes tracked after querying existing pods={:?}",
        nodes_to_act_on
    );
    do_pod_action_for_nodes(nodes_to_act_on, instance, podspec, kube_interface).await?;
    trace!("handle_instance_change - exit");

    Ok(())
}

pub(crate) async fn do_pod_action_for_nodes(
    nodes_to_act_on: HashMap<String, PodContext>,
    instance: &Instance,
    podspec: &PodSpec,
    kube_interface: &impl KubeInterface,
) -> anyhow::Result<()> {
    trace!("do_pod_action_for_nodes - enter");
    // Iterate over nodes_to_act_on where value == (PodAction::Remove | PodAction::RemoveAndAdd)
    for (node_to_delete_pod, context) in nodes_to_act_on.iter().filter(|&(_, v)| {
        ((v.action) == PodAction::Remove) | ((v.action) == PodAction::RemoveAndAdd)
    }) {
        handle_deletion_work(
            instance.metadata.name.as_ref().unwrap(),
            &instance.spec.configuration_name,
            instance.spec.shared,
            node_to_delete_pod,
            context,
            kube_interface,
        )
        .await?
    }

    let nodes_to_add = nodes_to_act_on
        .iter()
        .filter_map(|(node, context)| {
            if ((context.action) == PodAction::Add) | ((context.action) == PodAction::RemoveAndAdd)
            {
                Some(node.to_string())
            } else {
                None
            }
        })
        .collect::<Vec<String>>();

    // Iterate over nodes_to_act_on where value == (PodAction::Add | PodAction::RemoveAndAdd)
    for new_node in nodes_to_add {
        handle_addition_work(
            instance.metadata.name.as_ref().unwrap(),
            instance.metadata.uid.as_ref().unwrap(),
            instance.metadata.namespace.as_ref().unwrap(),
            &instance.spec.configuration_name,
            instance.spec.shared,
            &new_node,
            podspec,
            kube_interface,
        )
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod handle_instance_tests {
    use super::super::shared_test_utils::config_for_tests;
    use super::super::shared_test_utils::config_for_tests::PodList;
    use super::*;
    use akri_shared::{
        akri::instance::Instance,
        k8s::{pod::AKRI_INSTANCE_LABEL_NAME, MockKubeInterface},
        os::file,
    };
    use chrono::prelude::*;
    use chrono::Utc;
    use mockall::predicate::*;

    fn configure_find_pods_with_phase(
        mock: &mut MockKubeInterface,
        pod_selector: &'static str,
        result_file: &'static str,
        specified_phase: &'static str,
    ) {
        trace!(
            "mock.expect_find_pods_with_label pod_selector:{}",
            pod_selector
        );
        mock.expect_find_pods_with_label()
            .times(1)
            .withf(move |selector| selector == pod_selector)
            .returning(move |_| {
                let pods_json = file::read_file_to_string(result_file);
                let phase_adjusted_json = pods_json.replace(
                    "\"phase\": \"Running\"",
                    &format!("\"phase\": \"{}\"", specified_phase),
                );
                let pods: PodList = serde_json::from_str(&phase_adjusted_json).unwrap();
                Ok(pods)
            });
    }

    fn configure_find_pods_with_phase_and_start_time(
        mock: &mut MockKubeInterface,
        pod_selector: &'static str,
        result_file: &'static str,
        specified_phase: &'static str,
        start_time: DateTime<Utc>,
    ) {
        trace!(
            "mock.expect_find_pods_with_label pod_selector:{}",
            pod_selector
        );
        mock.expect_find_pods_with_label()
            .times(1)
            .withf(move |selector| selector == pod_selector)
            .returning(move |_| {
                let pods_json = file::read_file_to_string(result_file);
                let phase_adjusted_json = pods_json.replace(
                    "\"phase\": \"Running\"",
                    &format!("\"phase\": \"{}\"", specified_phase),
                );
                let start_time_adjusted_json = phase_adjusted_json.replace(
                    "\"startTime\": \"2020-02-25T20:48:03Z\"",
                    &format!(
                        "\"startTime\": \"{}\"",
                        start_time.format("%Y-%m-%dT%H:%M:%SZ")
                    ),
                );
                let pods: PodList = serde_json::from_str(&start_time_adjusted_json).unwrap();
                Ok(pods)
            });
    }

    fn configure_find_pods_with_phase_and_no_start_time(
        mock: &mut MockKubeInterface,
        pod_selector: &'static str,
        result_file: &'static str,
        specified_phase: &'static str,
    ) {
        trace!(
            "mock.expect_find_pods_with_label pod_selector:{}",
            pod_selector
        );
        mock.expect_find_pods_with_label()
            .times(1)
            .withf(move |selector| selector == pod_selector)
            .returning(move |_| {
                let pods_json = file::read_file_to_string(result_file);
                let phase_adjusted_json = pods_json.replace(
                    "\"phase\": \"Running\"",
                    &format!("\"phase\": \"{}\"", specified_phase),
                );
                let start_time_adjusted_json =
                    phase_adjusted_json.replace("\"startTime\": \"2020-02-25T20:48:03Z\",", "");
                let pods: PodList = serde_json::from_str(&start_time_adjusted_json).unwrap();
                Ok(pods)
            });
    }

    #[derive(Clone)]
    struct HandleInstanceWork {
        find_pods_selector: &'static str,
        find_pods_result: &'static str,
        find_pods_phase: Option<&'static str>,
        find_pods_start_time: Option<DateTime<Utc>>,
        find_pods_delete_start_time: bool,
        config_work: HandleConfigWork,
        deletion_work: Option<HandleDeletionWork>,
        addition_work: Option<HandleAdditionWork>,
    }

    #[derive(Clone)]
    struct HandleConfigWork {
        find_config_name: &'static str,
        find_config_namespace: &'static str,
        find_config_result: &'static str,
    }

    fn configure_for_handle_instance_change(
        mock: &mut MockKubeInterface,
        work: &HandleInstanceWork,
    ) {
        config_for_tests::configure_find_config(
            mock,
            work.config_work.find_config_name,
            work.config_work.find_config_namespace,
            work.config_work.find_config_result,
            false,
        );
        if let Some(phase) = work.find_pods_phase {
            if let Some(start_time) = work.find_pods_start_time {
                configure_find_pods_with_phase_and_start_time(
                    mock,
                    work.find_pods_selector,
                    work.find_pods_result,
                    phase,
                    start_time,
                );
            } else if work.find_pods_delete_start_time {
                configure_find_pods_with_phase_and_no_start_time(
                    mock,
                    work.find_pods_selector,
                    work.find_pods_result,
                    phase,
                );
            } else {
                configure_find_pods_with_phase(
                    mock,
                    work.find_pods_selector,
                    work.find_pods_result,
                    phase,
                );
            }
        } else {
            config_for_tests::configure_find_pods(
                mock,
                work.find_pods_selector,
                work.find_pods_result,
                false,
            );
        }

        if let Some(deletion_work) = &work.deletion_work {
            configure_for_handle_deletion_work(mock, deletion_work);
        }

        if let Some(addition_work) = &work.addition_work {
            configure_for_handle_addition_work(mock, addition_work);
        }
    }

    #[derive(Clone)]
    struct HandleDeletionWork {
        broker_pod_names: Vec<&'static str>,
        // instance_svc_names: Vec<&'static str>,
        cleanup_namespaces: Vec<&'static str>,
    }

    fn configure_deletion_work_for_config_a_359973() -> HandleDeletionWork {
        HandleDeletionWork {
            broker_pod_names: vec!["node-a-config-a-359973-pod"],
            // instance_svc_names: vec!["config-a-359973-svc"],
            cleanup_namespaces: vec!["config-a-namespace"],
        }
    }

    fn configure_deletion_work_for_config_a_b494b6() -> HandleDeletionWork {
        HandleDeletionWork {
            broker_pod_names: vec!["config-a-b494b6-pod"],
            // instance_svc_names: vec!["config-a-b494b6-svc"],
            cleanup_namespaces: vec!["config-a-namespace"],
        }
    }

    fn configure_for_handle_deletion_work(mock: &mut MockKubeInterface, work: &HandleDeletionWork) {
        for i in 0..work.broker_pod_names.len() {
            let broker_pod_name = work.broker_pod_names[i];
            let cleanup_namespace = work.cleanup_namespaces[i];

            config_for_tests::configure_remove_pod(mock, broker_pod_name, cleanup_namespace);
        }
    }

    #[derive(Clone)]
    struct HandleAdditionWork {
        new_pod_names: Vec<&'static str>,
        new_pod_instance_names: Vec<&'static str>,
        new_pod_namespaces: Vec<&'static str>,
        new_pod_error: Vec<bool>,
    }

    fn configure_add_shared_config_a_359973(pod_name: &'static str) -> HandleAdditionWork {
        HandleAdditionWork {
            new_pod_names: vec![pod_name],
            new_pod_instance_names: vec!["config-a-359973"],
            new_pod_namespaces: vec!["config-a-namespace"],
            new_pod_error: vec![false],
        }
    }
    fn get_config_work() -> HandleConfigWork {
        HandleConfigWork {
            find_config_name: "config-a",
            find_config_namespace: "config-a-namespace",
            find_config_result: "../test/json/config-a.json",
        }
    }
    fn configure_add_local_config_a_b494b6(error: bool) -> HandleAdditionWork {
        HandleAdditionWork {
            new_pod_names: vec!["config-a-b494b6-pod"],
            new_pod_instance_names: vec!["config-a-b494b6"],
            new_pod_namespaces: vec!["config-a-namespace"],
            new_pod_error: vec![error],
        }
    }

    fn configure_for_handle_addition_work(mock: &mut MockKubeInterface, work: &HandleAdditionWork) {
        for i in 0..work.new_pod_names.len() {
            config_for_tests::configure_add_pod(
                mock,
                work.new_pod_names[i],
                work.new_pod_namespaces[i],
                AKRI_INSTANCE_LABEL_NAME,
                work.new_pod_instance_names[i],
                work.new_pod_error[i],
            );
        }
    }

    async fn run_handle_instance_change_test(
        mock: &mut MockKubeInterface,
        instance_file: &'static str,
        action: &'static InstanceAction,
    ) {
        trace!("run_handle_instance_change_test enter");
        let instance_json = file::read_file_to_string(instance_file);
        let instance: Instance = serde_json::from_str(&instance_json).unwrap();
        handle_instance(
            match action {
                InstanceAction::Add | InstanceAction::Update => Event::Applied(instance),
                InstanceAction::Remove => Event::Deleted(instance),
            },
            mock,
            &mut false,
        )
        .await
        .unwrap();
        trace!("run_handle_instance_change_test exit");
    }

    // Test that watcher errors on restarts unless it is the first restart (aka initial startup)
    #[tokio::test]
    async fn test_handle_watcher_restart() {
        let _ = env_logger::builder().is_test(true).try_init();
        let mut first_event = true;
        assert!(handle_instance(
            Event::Restarted(Vec::new()),
            &MockKubeInterface::new(),
            &mut first_event
        )
        .await
        .is_ok());
        first_event = false;
        assert!(handle_instance(
            Event::Restarted(Vec::new()),
            &MockKubeInterface::new(),
            &mut first_event
        )
        .await
        .is_err());
    }

    #[tokio::test]
    async fn test_internal_handle_existing_instances_no_instances() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mut mock = MockKubeInterface::new();
        config_for_tests::configure_get_instances(&mut mock, "../test/json/empty-list.json", false);
        internal_handle_existing_instances(&mock).await.unwrap();
    }

    #[tokio::test]
    async fn test_handle_instance_change_for_add_new_local_instance() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mut mock = MockKubeInterface::new();
        configure_for_handle_instance_change(
            &mut mock,
            &HandleInstanceWork {
                find_pods_selector: "akri.sh/instance=config-a-b494b6",
                find_pods_result: "../test/json/empty-list.json",
                find_pods_phase: None,
                find_pods_start_time: None,
                find_pods_delete_start_time: false,
                config_work: get_config_work(),
                deletion_work: None,
                addition_work: Some(configure_add_local_config_a_b494b6(false)),
            },
        );
        run_handle_instance_change_test(
            &mut mock,
            "../test/json/local-instance.json",
            &InstanceAction::Add,
        )
        .await;
    }

    #[tokio::test]
    async fn test_handle_instance_change_for_add_new_local_instance_error() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mut mock = MockKubeInterface::new();
        configure_for_handle_instance_change(
            &mut mock,
            &HandleInstanceWork {
                find_pods_selector: "akri.sh/instance=config-a-b494b6",
                find_pods_result: "../test/json/empty-list.json",
                find_pods_phase: None,
                find_pods_start_time: None,
                find_pods_delete_start_time: false,
                config_work: get_config_work(),
                deletion_work: None,
                addition_work: Some(configure_add_local_config_a_b494b6(true)),
            },
        );
        run_handle_instance_change_test(
            &mut mock,
            "../test/json/local-instance.json",
            &InstanceAction::Add,
        )
        .await;
    }

    #[tokio::test]
    async fn test_handle_instance_change_for_remove_running_local_instance() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mut mock = MockKubeInterface::new();
        configure_for_handle_instance_change(
            &mut mock,
            &HandleInstanceWork {
                find_pods_selector: "akri.sh/instance=config-a-b494b6",
                find_pods_result: "../test/json/running-pod-list-for-config-a-local.json",
                find_pods_phase: None,
                find_pods_start_time: None,
                find_pods_delete_start_time: false,
                config_work: get_config_work(),
                deletion_work: Some(configure_deletion_work_for_config_a_b494b6()),
                addition_work: None,
            },
        );
        run_handle_instance_change_test(
            &mut mock,
            "../test/json/local-instance.json",
            &InstanceAction::Remove,
        )
        .await;
    }

    #[tokio::test]
    async fn test_handle_instance_change_for_add_new_shared_instance() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mut mock = MockKubeInterface::new();
        configure_for_handle_instance_change(
            &mut mock,
            &HandleInstanceWork {
                find_pods_selector: "akri.sh/instance=config-a-359973",
                find_pods_result: "../test/json/empty-list.json",
                find_pods_phase: None,
                find_pods_start_time: None,
                find_pods_delete_start_time: false,
                config_work: get_config_work(),
                deletion_work: None,
                addition_work: Some(configure_add_shared_config_a_359973(
                    "node-a-config-a-359973-pod",
                )),
            },
        );
        run_handle_instance_change_test(
            &mut mock,
            "../test/json/shared-instance.json",
            &InstanceAction::Add,
        )
        .await;
    }

    #[tokio::test]
    async fn test_handle_instance_change_for_remove_running_shared_instance() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mut mock = MockKubeInterface::new();
        configure_for_handle_instance_change(
            &mut mock,
            &HandleInstanceWork {
                find_pods_selector: "akri.sh/instance=config-a-359973",
                find_pods_result: "../test/json/running-pod-list-for-config-a-shared.json",
                find_pods_phase: None,
                find_pods_start_time: None,
                find_pods_delete_start_time: false,
                config_work: get_config_work(),
                deletion_work: Some(configure_deletion_work_for_config_a_359973()),
                addition_work: None,
            },
        );
        run_handle_instance_change_test(
            &mut mock,
            "../test/json/shared-instance.json",
            &InstanceAction::Remove,
        )
        .await;
    }

    #[tokio::test]
    async fn test_handle_instance_change_for_update_active_shared_instance() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mut mock = MockKubeInterface::new();
        configure_for_handle_instance_change(
            &mut mock,
            &HandleInstanceWork {
                find_pods_selector: "akri.sh/instance=config-a-359973",
                find_pods_result: "../test/json/running-pod-list-for-config-a-shared.json",
                find_pods_phase: None,
                find_pods_start_time: None,
                find_pods_delete_start_time: false,
                config_work: get_config_work(),
                deletion_work: Some(configure_deletion_work_for_config_a_359973()),
                addition_work: Some(configure_add_shared_config_a_359973(
                    "node-b-config-a-359973-pod",
                )),
            },
        );
        run_handle_instance_change_test(
            &mut mock,
            "../test/json/shared-instance-update.json",
            &InstanceAction::Update,
        )
        .await;
    }

    #[tokio::test]
    async fn test_handle_instance_change_when_node_disappears_shared() {
        let _ = env_logger::builder().is_test(true).try_init();

        let deleted_node = "node-b";
        let instance_file = "../test/json/shared-instance-update.json";
        let instance_json = file::read_file_to_string(instance_file);
        let kube_object_instance: Instance = serde_json::from_str(&instance_json).unwrap();
        let mut instance = kube_object_instance.spec;
        instance.nodes = instance
            .nodes
            .iter()
            .filter_map(|n| {
                if n != deleted_node {
                    Some(n.to_string())
                } else {
                    None
                }
            })
            .collect();
        instance.device_usage = instance
            .device_usage
            .iter()
            .map(|(k, v)| {
                if v != deleted_node {
                    (k.to_string(), v.to_string())
                } else {
                    (k.to_string(), "".to_string())
                }
            })
            .collect::<HashMap<String, String>>();

        let mut mock = MockKubeInterface::new();
        configure_for_handle_instance_change(
            &mut mock,
            &HandleInstanceWork {
                find_pods_selector: "akri.sh/instance=config-a-359973",
                find_pods_result: "../test/json/running-pod-list-for-config-a-shared.json",
                find_pods_phase: None,
                find_pods_start_time: None,
                find_pods_delete_start_time: false,
                config_work: get_config_work(),
                deletion_work: Some(configure_deletion_work_for_config_a_359973()),
                addition_work: Some(configure_add_shared_config_a_359973(
                    "node-b-config-a-359973-pod",
                )),
            },
        );
        run_handle_instance_change_test(&mut mock, instance_file, &InstanceAction::Update).await;
    }

    /// Checks that the BROKER_POD_COUNT_METRIC is appropriately incremented
    /// and decremented when an instance is added and deleted (and pods are
    /// created and deleted). Cannot be run in parallel with other tests
    /// due to the metric being a global variable and modified unpredictably by
    /// other tests.
    /// Run with: cargo test -- test_broker_pod_count_metric --ignored
    #[tokio::test]
    #[ignore]
    async fn test_broker_pod_count_metric() {
        let _ = env_logger::builder().is_test(true).try_init();
        BROKER_POD_COUNT_METRIC
            .with_label_values(&["config-a", "node-a"])
            .set(0);

        let mut mock = MockKubeInterface::new();
        configure_for_handle_instance_change(
            &mut mock,
            &HandleInstanceWork {
                find_pods_selector: "akri.sh/instance=config-a-b494b6",
                find_pods_result: "../test/json/empty-list.json",
                find_pods_phase: None,
                find_pods_start_time: None,
                find_pods_delete_start_time: false,
                config_work: get_config_work(),
                deletion_work: None,
                addition_work: Some(configure_add_local_config_a_b494b6(false)),
            },
        );
        run_handle_instance_change_test(
            &mut mock,
            "../test/json/local-instance.json",
            &InstanceAction::Add,
        )
        .await;

        // Check that broker pod count metric has been incremented to include new pod for this instance
        assert_eq!(
            BROKER_POD_COUNT_METRIC
                .with_label_values(&["config-a", "node-a"])
                .get(),
            1
        );

        configure_for_handle_instance_change(
            &mut mock,
            &HandleInstanceWork {
                find_pods_selector: "akri.sh/instance=config-a-b494b6",
                find_pods_result: "../test/json/running-pod-list-for-config-a-local.json",
                find_pods_phase: None,
                find_pods_start_time: None,
                find_pods_delete_start_time: false,
                config_work: get_config_work(),
                deletion_work: Some(configure_deletion_work_for_config_a_b494b6()),
                addition_work: None,
            },
        );
        run_handle_instance_change_test(
            &mut mock,
            "../test/json/local-instance.json",
            &InstanceAction::Remove,
        )
        .await;

        // Check that broker pod count metric has been decremented to reflect deleted instance and pod
        assert_eq!(
            BROKER_POD_COUNT_METRIC
                .with_label_values(&["config-a", "node-a"])
                .get(),
            0
        );
    }
}
