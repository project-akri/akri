use super::super::BROKER_POD_COUNT_METRIC;
use super::{pod_action::PodAction, pod_action::PodActionInfo};
use crate::util::context::{ControllerKubeClient, InstanceControllerContext};
use crate::util::{ControllerError, Result};
use akri_shared::akri::configuration::Configuration;
use akri_shared::k8s::api::Api;
use akri_shared::{
    akri::{configuration::BrokerSpec, instance::Instance, AKRI_PREFIX},
    k8s::{
        job, pod, OwnershipInfo, OwnershipType, AKRI_INSTANCE_LABEL_NAME,
        AKRI_TARGET_NODE_LABEL_NAME,
    },
};
use anyhow::Context;
use futures::StreamExt;
use k8s_openapi::api::batch::v1::{Job, JobSpec};
use k8s_openapi::api::core::v1::{Pod, PodSpec};

use kube::{
    api::{ListParams, ResourceExt},
    runtime::{
        controller::{Action, Controller},
        finalizer::{finalizer, Event},
        watcher::Config,
    },
};
use log::{error, trace};
use std::collections::HashMap;
use std::sync::Arc;

/// Length of time a Pod can be pending before we give up and retry
pub const PENDING_POD_GRACE_PERIOD_MINUTES: i64 = 5;
/// Length of time a Pod can be in an error state before we retry
pub const FAILED_POD_GRACE_PERIOD_MINUTES: i64 = 0;

pub static INSTANCE_FINALIZER: &str = "instances.kube.rs";

/// Initialize the instance controller
/// TODO: consider passing state that is shared among controllers such as a metrics exporter
pub async fn run(ctx: Arc<InstanceControllerContext>) {
    let api = ctx.client().all().as_inner();
    if let Err(e) = api.list(&ListParams::default().limit(1)).await {
        error!("Instance CRD is not queryable; {e:?}. Is the CRD installed?");
        std::process::exit(1);
    }
    Controller::new(api, Config::default().any_semantic())
        .shutdown_on_signal()
        .run(reconcile, error_policy, ctx.clone())
        .filter_map(|x| async move { std::result::Result::ok(x) })
        .for_each(|_| futures::future::ready(()))
        .await;
}

fn error_policy(
    _instance: Arc<Instance>,
    error: &ControllerError,
    _ctx: Arc<InstanceControllerContext>,
) -> Action {
    log::warn!("reconcile failed: {:?}", error);
    Action::requeue(std::time::Duration::from_secs(5 * 60))
}

/// Instance event types
///
/// Instance actions describe the types of actions the Controller can
/// react to for Instances.
///
/// This will determine what broker management actions to take (if any)
///
///   | --> Instance Applied
///                 | --> No broker => Do nothing
///                 | --> <BrokerSpec::BrokerJobSpec> => Deploy a Job if one does not exist
///                 | --> <BrokerSpec::BrokerPodSpec> => Ensure that each Node on Instance's `nodes` list (up to `capacity` total) have a Pod.
///                                                      Deploy Pods as necessary

/// This function is the main Reconcile function for Instance resources
/// This will get called every time an Instance gets added or is changed, it will also be called for every existing instance on startup.
pub async fn reconcile(
    instance: Arc<Instance>,
    ctx: Arc<InstanceControllerContext>,
) -> Result<Action> {
    let ns = instance.namespace().unwrap(); // instance has namespace scope
    trace!("Reconciling {} in {}", instance.name_any(), ns);
    finalizer(
        &ctx.client().all().as_inner(),
        INSTANCE_FINALIZER,
        instance,
        |event| reconcile_inner(event, ctx.client()),
    )
    .await
    .map_err(|e| ControllerError::FinalizerError(Box::new(e)))
}

async fn reconcile_inner(
    event: Event<Instance>,
    client: Arc<dyn ControllerKubeClient>,
) -> Result<Action> {
    match event {
        Event::Apply(instance) => handle_instance_change(&instance, client).await,
        Event::Cleanup(_) => {
            // Do nothing. OwnerReferences are attached to Jobs and Pods to automate cleanup
            Ok(default_requeue_action())
        }
    }
}

/// PodContext stores a set of details required to track/create/delete broker
/// Pods.
///
/// The PodContext stores what is required to determine how to handle a
/// specific Node's protocol broker Pod.
///
/// * the node is described by node_name
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
    // Early exits above ensure unwrap will not panic
    let node_to_run_pod_on = &k8s_pod
        .labels()
        .get(AKRI_TARGET_NODE_LABEL_NAME)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "no {} label found for {:?}",
                AKRI_TARGET_NODE_LABEL_NAME,
                k8s_pod.name_unchecked()
            )
        })?;

    Ok(PodContext {
        node_name: Some(node_to_run_pod_on.to_string()),
        namespace: k8s_pod.namespace(),
        action,
    })
}

/// This finds what to do with a given broker Pod based on its current state and
/// the Instance event action.  If this method has enough information,
/// it will update the nodes_to_act_on map with the required action.
fn determine_action_for_pod(
    k8s_pod: &Pod,
    nodes_to_act_on: &mut HashMap<String, PodContext>,
) -> anyhow::Result<()> {
    let pod_name = k8s_pod.name_unchecked();
    let pod_phase = k8s_pod
        .status
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("No pod status found for Pod {:?}", pod_name))?
        .phase
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("No pod phase found for Pod {:?}", pod_name))?;

    let mut ctx = create_pod_context(k8s_pod, PodAction::NoAction)?;

    // Early exits above ensure unwrap will not panic
    let pod_start_time = k8s_pod.status.as_ref().unwrap().start_time.clone();
    let node_to_run_pod_on = ctx.node_name.as_ref().unwrap();

    let pod_action_info = PodActionInfo {
        pending_grace_time_in_minutes: PENDING_POD_GRACE_PERIOD_MINUTES,
        ended_grace_time_in_minutes: FAILED_POD_GRACE_PERIOD_MINUTES,
        phase: pod_phase.to_string(),
        status_start_time: pod_start_time,
        unknown_node: !nodes_to_act_on.contains_key(node_to_run_pod_on),
        trace_pod_name: k8s_pod.name_unchecked(),
    };
    ctx.action = pod_action_info.select_pod_action()?;
    nodes_to_act_on.insert(node_to_run_pod_on.to_string(), ctx);
    Ok(())
}

/// This deliberately deletes the broker Pod, the broker Service (if there are no remaining broker Pods), and the configuration service (if there are no remaining capability Pods).
/// This is done before recreating the broker Pod and svcs
async fn handle_deletion_work(
    instance_name: &str,
    configuration_name: &str,
    instance_shared: bool,
    node_to_delete_pod: &str,
    context: &PodContext,
    api: &dyn Api<Pod>,
) -> anyhow::Result<()> {
    let context_node_name = context.node_name.as_ref().ok_or_else(|| {
        anyhow::anyhow!(
            "handle_deletion_work - Context node_name is missing for {}: {:?}",
            node_to_delete_pod,
            context
        )
    })?;

    trace!(
        "handle_deletion_work - pod::create_broker_app_name({:?}, {:?}, {:?}, {:?})",
        &instance_name,
        context.node_name,
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
        &context.namespace
    );
    api.delete(&pod_app_name).await?;
    trace!("handle_deletion_work - pod::remove_pod succeeded");
    BROKER_POD_COUNT_METRIC
        .with_label_values(&[configuration_name, context_node_name])
        .dec();
    Ok(())
}

/// This handles Instance addition event by creating the
/// broker Pod.
async fn handle_addition_work(
    api: &dyn Api<Pod>,
    pod: Pod,
    configuration_name: &str,
    new_node: &str,
) -> anyhow::Result<()> {
    trace!(
        "handle_addition_work - Create new Pod for Node={:?}",
        new_node
    );

    trace!("handle_addition_work - New pod spec={:?}", pod);
    api.apply(pod, INSTANCE_FINALIZER).await?;
    trace!("handle_addition_work - pod::create_pod succeeded",);
    BROKER_POD_COUNT_METRIC
        .with_label_values(&[configuration_name, new_node])
        .inc();

    Ok(())
}

/// Handle Instance change by
/// 1) checking to make sure the Instance's Configuration exists
/// 2) calling the appropriate handler depending on the broker type (Pod or Job) if any
pub async fn handle_instance_change(
    instance: &Instance,
    client: Arc<dyn ControllerKubeClient>,
) -> Result<Action> {
    trace!("handle_instance_change - enter");
    let instance_namespace = instance.namespace().unwrap();
    let api: Box<dyn Api<Configuration>> = client.namespaced(&instance_namespace);
    let Ok(Some(configuration)) = api.get(&instance.spec.configuration_name).await else {
        // In this scenario, a configuration has been deleted without the Akri Agent deleting the associated Instances.
        // Furthermore, Akri Agent is still modifying the Instances. This should not happen beacuse Agent
        // is designed to shutdown when it's Configuration watcher fails.
        error!("handle_instance_change - no configuration found for {:?} yet instance {:?} exists - check that device plugin is running properly",
                        &instance.spec.configuration_name, &instance.name_unchecked()
                    );

        return Ok(default_requeue_action());
    };
    let Some(broker_spec) = &configuration.spec.broker_spec else {
        return Ok(default_requeue_action());
    };
    let res = match broker_spec {
        BrokerSpec::BrokerPodSpec(p) => handle_instance_change_pod(instance, p, client).await,
        BrokerSpec::BrokerJobSpec(j) => {
            handle_instance_change_job(
                instance,
                *configuration.metadata.generation.as_ref().unwrap(),
                j,
                client.clone(),
            )
            .await
        }
    };
    if let Err(e) = res {
        error!("Unable to handle Broker action: {:?}", e);
    }
    Ok(default_requeue_action())
}

/// Called when an Instance has changed that requires a Job broker. Action determined by InstanceAction.
/// First check if a job with the instance name exists. If it does, do nothing. Otherwise, deploy a Job
///  with JobSpec from Configuration and label with Instance name.
pub async fn handle_instance_change_job(
    instance: &Instance,
    config_generation: i64,
    job_spec: &JobSpec,
    client: Arc<dyn ControllerKubeClient>,
) -> anyhow::Result<()> {
    trace!("handle_instance_change_job - enter");
    let api: Box<dyn Api<Job>> = client.namespaced(&instance.namespace().unwrap());
    if api.get(&instance.name_unchecked()).await?.is_some() {
        // Job already exists, do nothing
        return Ok(());
    }
    let instance_name = instance.name_unchecked();
    // Create name for Job. Includes Configuration generation in the suffix
    // to track what version of the Configuration the Job is associated with.
    let job_name = pod::create_broker_app_name(
        &instance_name,
        None,
        instance.spec.shared,
        &format!("{}-job", config_generation),
    );

    trace!("handle_instance_change_job - instance added");
    let capability_id = format!("{}/{}", AKRI_PREFIX, instance_name);
    let new_job = job::create_new_job_from_spec(
        instance,
        OwnershipInfo::new(
            OwnershipType::Instance,
            instance_name,
            instance.uid().unwrap(),
        ),
        &capability_id,
        job_spec,
        &job_name,
    )?;
    let api: Box<dyn Api<Job>> = client.namespaced(&instance.namespace().unwrap());
    // TODO: Consider using server side apply instead of create
    api.create(&new_job).await?;
    Ok(())
}

/// Called when an Instance has changed that requires a Pod broker.
/// Ensures that each Node on Instance's `nodes` list (up to `capacity` total) has a running Pod
pub async fn handle_instance_change_pod(
    instance: &Instance,
    podspec: &PodSpec,
    client: Arc<dyn ControllerKubeClient>,
) -> anyhow::Result<()> {
    trace!("handle_instance_change_pod - enter");
    // Assume all nodes require PodAction::Add (reflect that there is no running Pod, unless we find one)
    let default_action = PodAction::Add;
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

    let lp = ListParams::default().labels(&format!(
        "{}={}",
        AKRI_INSTANCE_LABEL_NAME,
        instance.name_unchecked()
    ));
    let api = client.namespaced(&instance.namespace().context("no namespace")?);
    let instance_pods = api.list(&lp).await?;
    trace!(
        "handle_instance_change - found {} pods",
        instance_pods.items.len()
    );
    // By default, assume any pod tracked by the instance need to be added.
    // Query the existing pods to see if some of these are already added, or
    // need to be removed
    instance_pods
        .items
        .iter()
        .try_for_each(|x| determine_action_for_pod(x, &mut nodes_to_act_on))?;

    trace!(
        "handle_instance_change - nodes tracked after querying existing pods={:?}",
        nodes_to_act_on
    );
    do_pod_action_for_nodes(nodes_to_act_on, instance, podspec, api).await?;
    trace!("handle_instance_change - exit");

    Ok(())
}

pub(crate) async fn do_pod_action_for_nodes(
    nodes_to_act_on: HashMap<String, PodContext>,
    instance: &Instance,
    podspec: &PodSpec,
    api: Box<dyn Api<Pod>>,
) -> anyhow::Result<()> {
    trace!("do_pod_action_for_nodes - enter");
    // Iterate over nodes_to_act_on where value == (PodAction::Remove | PodAction::RemoveAndAdd)
    for (node_to_delete_pod, context) in nodes_to_act_on.iter().filter(|&(_, v)| {
        ((v.action) == PodAction::Remove) | ((v.action) == PodAction::RemoveAndAdd)
    }) {
        handle_deletion_work(
            &instance.name_unchecked(),
            &instance.spec.configuration_name,
            instance.spec.shared,
            node_to_delete_pod,
            context,
            api.as_ref(),
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
    let instance_name = instance.name_unchecked();
    let capability_id = format!("{}/{}", AKRI_PREFIX, instance_name);
    for new_node in nodes_to_add {
        let new_pod = pod::create_new_pod_from_spec(
            &instance.namespace().unwrap(),
            &instance_name,
            &instance.spec.configuration_name,
            OwnershipInfo::new(
                OwnershipType::Instance,
                instance_name.clone(),
                instance.uid().unwrap(),
            ),
            &capability_id,
            &new_node,
            instance.spec.shared,
            podspec,
        )?;
        handle_addition_work(
            api.as_ref(),
            new_pod,
            &instance.spec.configuration_name,
            &new_node,
        )
        .await?;
    }
    Ok(())
}

// Default action for finalizers for the instance controller
fn default_requeue_action() -> Action {
    Action::await_change()
}

#[cfg(test)]
mod handle_instance_tests {
    use crate::util::shared_test_utils::mock_client::MockControllerKubeClient;

    use super::super::shared_test_utils::config_for_tests::*;
    use super::*;
    use akri_shared::{
        akri::instance::Instance,
        k8s::{api::MockApi, pod::AKRI_INSTANCE_LABEL_NAME},
        os::file,
    };
    use chrono::prelude::*;
    use chrono::Utc;
    use mockall::predicate::*;

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
        mock: &mut MockControllerKubeClient,
        work: &HandleInstanceWork,
    ) {
        let mut mock_pod_api: MockApi<Pod> = MockApi::new();
        configure_find_config(
            mock,
            work.config_work.find_config_name,
            work.config_work.find_config_namespace,
            work.config_work.find_config_result,
            false,
        );
        if let Some(phase) = work.find_pods_phase {
            if let Some(start_time) = work.find_pods_start_time {
                configure_find_pods_with_phase_and_start_time(
                    &mut mock_pod_api,
                    work.find_pods_selector,
                    work.find_pods_result,
                    phase,
                    start_time,
                );
            } else if work.find_pods_delete_start_time {
                configure_find_pods_with_phase_and_no_start_time(
                    &mut mock_pod_api,
                    work.find_pods_selector,
                    work.find_pods_result,
                    phase,
                );
            } else {
                configure_find_pods_with_phase(
                    &mut mock_pod_api,
                    work.find_pods_selector,
                    work.find_pods_result,
                    phase,
                );
            }
        } else {
            configure_find_pods(
                &mut mock_pod_api,
                work.find_pods_selector,
                work.config_work.find_config_namespace,
                work.find_pods_result,
                false,
            );
        }

        if let Some(deletion_work) = &work.deletion_work {
            configure_for_handle_deletion_work(&mut mock_pod_api, deletion_work);
        }

        if let Some(addition_work) = &work.addition_work {
            configure_for_handle_addition_work(&mut mock_pod_api, addition_work);
        }
        mock.pod
            .expect_namespaced()
            .return_once(move |_| Box::new(mock_pod_api));
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

    fn configure_for_handle_deletion_work(mock: &mut MockApi<Pod>, work: &HandleDeletionWork) {
        for i in 0..work.broker_pod_names.len() {
            let broker_pod_name = work.broker_pod_names[i];
            let cleanup_namespace = work.cleanup_namespaces[i];

            configure_remove_pod(mock, broker_pod_name, cleanup_namespace);
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

    fn configure_for_handle_addition_work(mock_api: &mut MockApi<Pod>, work: &HandleAdditionWork) {
        for i in 0..work.new_pod_names.len() {
            configure_add_pod(
                mock_api,
                work.new_pod_names[i],
                work.new_pod_namespaces[i],
                AKRI_INSTANCE_LABEL_NAME,
                work.new_pod_instance_names[i],
                work.new_pod_error[i],
            );
        }
    }

    async fn run_handle_instance_change_test(
        client: Arc<dyn ControllerKubeClient>,
        instance_file: &'static str,
    ) {
        trace!("run_handle_instance_change_test enter");
        let instance_json = file::read_file_to_string(instance_file);
        let instance: Instance = serde_json::from_str(&instance_json).unwrap();
        reconcile_inner(Event::Apply(Arc::new(instance)), client)
            .await
            .unwrap();
        trace!("run_handle_instance_change_test exit");
    }

    #[tokio::test]
    async fn test_handle_instance_change_for_add_new_local_instance() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mut mock = MockControllerKubeClient::default();
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
        run_handle_instance_change_test(Arc::new(mock), "../test/json/local-instance.json").await;
    }

    #[tokio::test]
    async fn test_handle_instance_change_for_add_new_local_instance_error() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mut mock = MockControllerKubeClient::default();
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
        run_handle_instance_change_test(Arc::new(mock), "../test/json/local-instance.json").await;
    }

    #[tokio::test]
    async fn test_handle_instance_change_for_add_new_shared_instance() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mut mock = MockControllerKubeClient::default();
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
        run_handle_instance_change_test(Arc::new(mock), "../test/json/shared-instance.json").await;
    }

    #[tokio::test]
    async fn test_handle_instance_change_for_update_active_shared_instance() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mut mock = MockControllerKubeClient::default();
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
        run_handle_instance_change_test(Arc::new(mock), "../test/json/shared-instance-update.json")
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

        let mut mock = MockControllerKubeClient::default();
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
        run_handle_instance_change_test(Arc::new(mock), instance_file).await;
    }

    /// Checks that the BROKER_POD_COUNT_METRIC is appropriately incremented
    /// instance is added and pods are created. Cannot be run in parallel with other tests
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

        let mut mock = MockControllerKubeClient::default();
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
        run_handle_instance_change_test(Arc::new(mock), "../test/json/local-instance.json").await;

        // Check that broker pod count metric has been incremented to include new pod for this instance
        assert_eq!(
            BROKER_POD_COUNT_METRIC
                .with_label_values(&["config-a", "node-a"])
                .get(),
            1
        );
    }
}
