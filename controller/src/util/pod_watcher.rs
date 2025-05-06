use crate::util::context::{PodState, PodWatcherContext};
use crate::util::{ControllerError, Result};
use crate::BROKER_POD_COUNT_METRIC;
use akri_shared::k8s::AKRI_TARGET_NODE_LABEL_NAME;
use akri_shared::{
    akri::{configuration::Configuration, instance::Instance, API_NAMESPACE},
    k8s::{
        api::Api, OwnershipInfo, OwnershipType, AKRI_CONFIGURATION_LABEL_NAME,
        AKRI_INSTANCE_LABEL_NAME, APP_LABEL_ID, CONTROLLER_LABEL_ID,
    },
};

use k8s_openapi::api::core::v1::Pod;
use k8s_openapi::{
    api::core::v1::{Service, ServiceSpec},
    apimachinery::pkg::apis::meta::v1::OwnerReference,
};
use kube::api::ObjectList;

use kube::{
    api::{ListParams, ObjectMeta, ResourceExt},
    runtime::{
        controller::Action,
        finalizer::{finalizer, Event},
    },
};
use log::{info, trace};
use std::future::Future;
use std::{collections::BTreeMap, sync::Arc};

pub static POD_FINALIZER: &str = "akri-pod-watcher.kube.rs";

/// The `kind` of a broker Pod's controlling OwnerReference
///
/// Determines what controls the deployment of the broker Pod.
#[derive(Debug, PartialEq)]
enum BrokerPodOwnerKind {
    /// An Instance "owns" this broker Pod, since the broker pod
    /// has an OwnerReference where `kind == "Instance"` and `controller=true`.
    Instance,
    /// A Job "owns" this broker Pod, since the broker pod
    /// has an OwnerReference where `kind == "Job"` and `controller=true`.
    Job,
    /// The broker Pod does not have a Job nor Instance OwnerReference
    Other,
}

/// Determines whether a Pod is owned by an Instance (has an ownerReference of Kind = "Instance")
/// Pods deployed directly by the Controller will have this ownership, while Pods
/// created by Jobs will not.
fn get_broker_pod_owner_kind(pod: Arc<Pod>) -> BrokerPodOwnerKind {
    let instance_kind = "Instance".to_string();
    let job_kind = "Job".to_string();
    let or = &pod.owner_references();
    if or
        .iter()
        .any(|r| r.kind == instance_kind && r.controller.unwrap_or(false))
    {
        BrokerPodOwnerKind::Instance
    } else if or
        .iter()
        .any(|r| r.kind == job_kind && r.controller.unwrap_or(false))
    {
        BrokerPodOwnerKind::Job
    } else {
        BrokerPodOwnerKind::Other
    }
}

pub async fn check(client: Arc<dyn super::context::ControllerKubeClient>) -> anyhow::Result<()> {
    let api: Box<dyn Api<Pod>> = client.all();
    if let Err(e) = api.list(&ListParams::default().limit(1)).await {
        anyhow::bail!("Pods are not queryable; {e:?}")
    }
    Ok(())
}

pub fn error_policy(
    _pod: Arc<Pod>,
    error: &ControllerError,
    _ctx: Arc<PodWatcherContext>,
) -> Action {
    log::warn!("reconcile failed: {:?}", error);
    Action::requeue(std::time::Duration::from_secs(5 * 60))
}

/// This is used to handle broker Pods entering and leaving
/// the Running state.
///
/// When a broker Pod enters the Running state, make sure
/// that the required instance and configuration services
/// are running.
///
/// When a broker Pod leaves the Running state, make sure
/// that any existing instance and configuration services
/// still have other broker Pods supporting them.  If there
/// are no other supporting broker Pods, delete one or both
/// of the services.
pub async fn reconcile(pod: Arc<Pod>, ctx: Arc<PodWatcherContext>) -> Result<Action> {
    trace!("Reconciling broker pod {}", pod.name_any());
    finalizer(
        &ctx.client.clone().all().as_inner(),
        POD_FINALIZER,
        pod,
        |event| reconcile_inner(event, ctx.clone()),
    )
    .await
    .map_err(|e| ControllerError::FinalizerError(Box::new(e)))
}

async fn reconcile_inner(event: Event<Pod>, ctx: Arc<PodWatcherContext>) -> Result<Action> {
    match event {
        Event::Apply(pod) => {
            let phase = get_pod_phase(&pod);
            info!(
                "reconcile - pod {:?} applied with phase {phase}",
                &pod.metadata.name
            );
            match phase.as_str() {
                "Unknown" | "Pending" => {
                    ctx.known_pods
                        .write()
                        .await
                        .insert(pod.name_unchecked(), PodState::Pending);
                }
                "Running" => {
                    handle_pod(pod, ctx, PodState::Running, handle_running_pod).await?;
                }
                "Succeeded" | "Failed" => {
                    handle_pod(pod, ctx, PodState::Ended, handle_non_running_pod).await?;
                }
                _ => {
                    trace!("handle_pod - Unknown phase: {:?}", &phase);
                }
            }
            Ok(Action::await_change())
        }
        Event::Cleanup(pod) => {
            info!("handle_pod - Deleted: {:?}", &pod.metadata.name);
            handle_pod(pod, ctx, PodState::Deleted, handle_non_running_pod).await?;
            Ok(Action::await_change())
        }
    }
}

/// Gets Pods phase and returns "Unknown" if no phase exists
fn get_pod_phase(pod: &Pod) -> String {
    if let Some(status) = &pod.status {
        status
            .phase
            .as_ref()
            .unwrap_or(&"Unknown".to_string())
            .to_string()
    } else {
        "Unknown".to_string()
    }
}

async fn handle_pod<F, Fut>(
    pod: Arc<Pod>,
    ctx: Arc<PodWatcherContext>,
    desired_state: PodState,
    handler: F,
) -> anyhow::Result<()>
where
    F: FnOnce(Arc<Pod>, Arc<PodWatcherContext>) -> Fut,
    Fut: Future<Output = anyhow::Result<()>>,
{
    trace!("handle_pod_if_needed - enter");
    let pod_name = pod.name_unchecked();
    let last_known_state = ctx
        .known_pods
        .read()
        .await
        .get(&pod_name)
        .unwrap_or(&PodState::Pending)
        .clone();
    trace!(
        "handle_pod_if_needed - last_known_state: {:?}",
        &last_known_state
    );
    // Ensure that, for each pod, handle_running_pod is called once
    // per transition into the Running state
    if last_known_state != desired_state {
        handler(pod, ctx.clone()).await?;
        ctx.known_pods
            .write()
            .await
            .insert(pod_name.to_string(), desired_state);
    }
    Ok(())
}

/// Get instance id and configuration name from Pod annotations, return
/// error if the annotations are not found.
fn get_instance_and_configuration_from_pod(pod: Arc<Pod>) -> anyhow::Result<(String, String)> {
    let labels = pod.labels();
    let instance_id = labels
        .get(AKRI_INSTANCE_LABEL_NAME)
        .ok_or_else(|| anyhow::anyhow!("No configuration name found."))?;
    let config_name = labels
        .get(AKRI_CONFIGURATION_LABEL_NAME)
        .ok_or_else(|| anyhow::anyhow!("No instance id found."))?;
    Ok((instance_id.to_string(), config_name.to_string()))
}

/// This is called when a broker Pod exits the Running phase and ensures
/// that instance and configuration services are only running when
/// supported by Running broker Pods.
async fn handle_non_running_pod(pod: Arc<Pod>, ctx: Arc<PodWatcherContext>) -> anyhow::Result<()> {
    trace!("handle_non_running_pod - enter");
    let namespace = pod.namespace().unwrap();
    let (instance_id, config_name) = get_instance_and_configuration_from_pod(pod.clone())?;
    let selector = format!("{}={}", AKRI_CONFIGURATION_LABEL_NAME, config_name);
    let broker_pods: ObjectList<Pod> = ctx
        .client
        .all()
        .list(&ListParams {
            label_selector: Some(selector),
            ..Default::default()
        })
        .await?;
    // Clean up instance services so long as all pods are terminated or terminating
    let svc_api = ctx.client.namespaced(&namespace);
    cleanup_svc_if_unsupported(
        &broker_pods.items,
        &create_service_app_name(&config_name),
        &namespace,
        svc_api.as_ref(),
    )
    .await?;
    let instance_pods: Vec<Pod> = broker_pods
        .items
        .into_iter()
        .filter(|x| match x.labels().get(AKRI_INSTANCE_LABEL_NAME) {
            Some(name) => name == &instance_id,
            None => false,
        })
        .collect();
    cleanup_svc_if_unsupported(
        &instance_pods,
        &create_service_app_name(&instance_id),
        &namespace,
        svc_api.as_ref(),
    )
    .await?;
    let fallback_node = "unknown".to_string();
    let node = pod
        .labels()
        .get(AKRI_TARGET_NODE_LABEL_NAME)
        .unwrap_or(&fallback_node);

    BROKER_POD_COUNT_METRIC
        .with_label_values(&[&config_name, node])
        .dec();

    // Only redeploy Pods that are managed by the Akri Controller (controlled by an Instance OwnerReference)
    if get_broker_pod_owner_kind(pod) == BrokerPodOwnerKind::Instance {
        let client: Box<dyn Api<Instance>> = ctx.client.namespaced(&namespace);
        if let Ok(Some(instance)) = client.get(&instance_id).await {
            super::instance_action::handle_instance_change(&instance, ctx.client.clone()).await?;
        }
    }
    Ok(())
}

/// This determines if there are Services that need to be removed because
/// they lack supporting Pods.  If any are found, the Service is removed.
async fn cleanup_svc_if_unsupported(
    pods: &[Pod],
    svc_name: &str,
    namespace: &str,
    svc_api: &dyn Api<Service>,
) -> anyhow::Result<()> {
    // Find the number of non-Terminating pods, if there aren't any (the only pods that exist are Terminating), we should remove the associated services
    let num_non_terminating_pods = pods.iter().filter(|&x|
        match &x.status {
            Some(status) => {
                match &status.phase {
                    Some(phase) => {
                        trace!("cleanup_svc_if_unsupported - finding num_non_terminating_pods: pod:{:?} phase:{:?}", &x.metadata.name, &phase);
                        phase != "Terminating" && phase != "Failed" && phase != "Succeeded"
                    },
                    _ => true,
                }
            },
            _ => true,
        }).count();
    if num_non_terminating_pods == 0 {
        trace!(
            "cleanup_svc_if_unsupported - deleting service name={:?}, namespace={:?}",
            &svc_name,
            &namespace
        );
        svc_api.delete(svc_name).await?;
    }
    Ok(())
}

/// This is called when a Pod enters the Running phase and ensures
/// that instance and configuration services are running as specified
/// by the configuration.
async fn handle_running_pod(pod: Arc<Pod>, ctx: Arc<PodWatcherContext>) -> anyhow::Result<()> {
    trace!("handle_running_pod - enter");
    let namespace = pod.namespace().unwrap();
    let (instance_name, configuration_name) = get_instance_and_configuration_from_pod(pod)?;
    let Some(configuration) = ctx
        .client
        .namespaced(&namespace)
        .get(&configuration_name)
        .await?
    else {
        // In this scenario, a configuration has likely been deleted in the middle of handle_running_pod.
        // There is no need to propogate the error and bring down the Controller.
        trace!(
            "handle_running_pod - no configuration found for {}",
            &configuration_name
        );
        return Ok(());
    };
    let Some(instance): Option<Instance> = ctx
        .client
        .namespaced(&namespace)
        .get(&instance_name)
        .await?
    else {
        // In this scenario, a instance has likely been deleted in the middle of handle_running_pod.
        trace!(
            "handle_running_pod - no instance found for {}",
            &instance_name
        );
        return Ok(());
    };
    let instance_uid = instance.uid().unwrap();
    add_instance_and_configuration_services(
        &instance_name,
        &instance_uid,
        &namespace,
        &configuration_name,
        &configuration,
        ctx,
    )
    .await?;
    Ok(())
}

/// This creates the broker Service and the capability Service.
async fn add_instance_and_configuration_services(
    instance_name: &str,
    instance_uid: &str,
    namespace: &str,
    configuration_name: &str,
    configuration: &Configuration,
    ctx: Arc<PodWatcherContext>,
) -> anyhow::Result<()> {
    trace!(
        "add_instance_and_configuration_services - instance={:?}",
        instance_name
    );
    let api = ctx.client.namespaced(namespace);
    if let Some(instance_service_spec) = &configuration.spec.instance_service_spec {
        let ownership = OwnershipInfo::new(
            OwnershipType::Instance,
            instance_name.to_string(),
            instance_uid.to_string(),
        );
        let mut labels: BTreeMap<String, String> = BTreeMap::new();
        labels.insert(
            AKRI_INSTANCE_LABEL_NAME.to_string(),
            instance_name.to_string(),
        );
        let app_name = create_service_app_name(instance_name);
        let instance_svc = create_new_service_from_spec(
            &app_name,
            namespace,
            ownership.clone(),
            instance_service_spec,
            labels,
        )?;
        api.apply(instance_svc, POD_FINALIZER).await?;
    }
    if let Some(configuration_service_spec) = &configuration.spec.configuration_service_spec {
        let configuration_uid = configuration.uid().unwrap();
        let ownership = OwnershipInfo::new(
            OwnershipType::Configuration,
            configuration_name.to_string(),
            configuration_uid.clone(),
        );
        let mut labels: BTreeMap<String, String> = BTreeMap::new();
        labels.insert(
            AKRI_CONFIGURATION_LABEL_NAME.to_string(),
            configuration_name.to_string(),
        );
        let app_name = create_service_app_name(configuration_name);
        let config_svc = create_new_service_from_spec(
            &app_name,
            namespace,
            ownership.clone(),
            configuration_service_spec,
            labels,
        )?;
        // TODO: use patch instead of apply
        api.apply(config_svc, POD_FINALIZER).await?;
    }
    Ok(())
}

pub fn create_new_service_from_spec(
    app_name: &str,
    svc_namespace: &str,
    ownership: OwnershipInfo,
    svc_spec: &ServiceSpec,
    mut labels: BTreeMap<String, String>,
) -> anyhow::Result<Service> {
    labels.insert(APP_LABEL_ID.to_string(), app_name.to_owned());
    labels.insert(CONTROLLER_LABEL_ID.to_string(), API_NAMESPACE.to_string());
    let owner_references: Vec<OwnerReference> = vec![OwnerReference {
        api_version: ownership.get_api_version(),
        kind: ownership.get_kind(),
        controller: ownership.get_controller(),
        block_owner_deletion: ownership.get_block_owner_deletion(),
        name: ownership.get_name(),
        uid: ownership.get_uid(),
    }];

    let mut spec = svc_spec.clone();
    let mut modified_selector: BTreeMap<String, String> = spec.selector.unwrap_or_default();
    modified_selector.insert(CONTROLLER_LABEL_ID.to_string(), API_NAMESPACE.to_string());
    spec.selector = Some(modified_selector);

    let new_svc = Service {
        spec: Some(spec),
        metadata: ObjectMeta {
            name: Some(app_name.to_owned()),
            namespace: Some(svc_namespace.to_string()),
            labels: Some(labels),
            owner_references: Some(owner_references),
            ..Default::default()
        },
        ..Default::default()
    };

    Ok(new_svc)
}

pub fn create_service_app_name(resource_name: &str) -> String {
    let normalized = resource_name.replace('.', "-");
    format!("{}-{}", normalized, "svc")
}

#[cfg(test)]
mod tests {
    use crate::util::shared_test_utils::mock_client::MockControllerKubeClient;

    use akri_shared::k8s::api::MockApi;
    use kube::api::{ObjectList, TypeMeta};
    use mockall::Sequence;

    //     use super::super::shared_test_utils::config_for_tests;
    //     use super::super::shared_test_utils::config_for_tests::PodList;
    use super::*;
    use k8s_openapi::api::core::v1::{Pod, PodSpec, PodStatus};
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::{ObjectMeta, OwnerReference};

    fn make_obj_list<T: Clone>(items: Vec<T>) -> ObjectList<T> {
        ObjectList {
            types: TypeMeta {
                api_version: "v1".to_string(),
                kind: "List".to_string(),
            },
            metadata: Default::default(),
            items,
        }
    }

    fn make_configuration(
        name: &str,
        namespace: &str,
        create_instance_svc: bool,
        create_config_svc: bool,
    ) -> Configuration {
        let config = serde_json::json!({
            "apiVersion": "akri.sh/v0",
            "kind": "Configuration",
            "metadata": {
                "name": name,
                "namespace": namespace,
                "uid": "e9fbe880-99da-47c1-bea3-5398f21ee747"
            },
            "spec": {
                "brokerSpec": {
                    "brokerPodSpec": {
                        "containers": [
                            {
                                "image": "nginx:latest",
                                "name": "broker"
                            }
                        ]
                    }
                },
                "discoveryHandler": {
                    "name": "debugEcho",
                    "discoveryDetails": "{\"debugEcho\": {\"descriptions\":[\"filter1\", \"filter2\"]}}"
                },
                "capacity": 5,
                "brokerProperties": {}
            }
        }
        );
        let mut config: Configuration = serde_json::from_value(config).unwrap();
        if create_config_svc {
            config.spec.configuration_service_spec = serde_json::from_value(serde_json::json!({
                "ports": [
                    {
                        "name": "http",
                        "port": 6052,
                        "protocol": "TCP",
                        "targetPort": 6052
                    }
                ],
                "type": "ClusterIP"
            }))
            .ok();
        }
        if create_instance_svc {
            config.spec.instance_service_spec = serde_json::from_value(serde_json::json!({
                "ports": [
                    {
                        "name": "http",
                        "port": 6052,
                        "protocol": "TCP",
                        "targetPort": 6052
                    }
                ],
                "type": "ClusterIP"
            }))
            .ok();
        }
        config
    }

    fn make_instance(name: &str, namespace: &str, config: &str) -> Instance {
        let instance = serde_json::json!({
            "apiVersion": "akri.sh/v0",
            "kind": "Instance",
            "metadata": {
                "name": name,
                "namespace": namespace,
                "uid": "e9fbe880-99da-47c1-bea3-5398f21ee747"
            },
            "spec": {
                "configurationName": config,
                "capacity": 5,
                "cdiName": "akri.sh/config-a=12345",
                "nodes": [ "node-a" ],
                "shared": true
            }
        });
        serde_json::from_value(instance).unwrap()
    }

    fn make_pod_with_owners_and_phase(
        instance: &str,
        config: &str,
        phase: &str,
        kind: &str,
    ) -> Pod {
        let owner_references: Vec<OwnerReference> = vec![OwnerReference {
            kind: kind.to_string(),
            controller: Some(true),
            name: instance.to_string(),
            ..Default::default()
        }];
        make_pod_with_owner_references_and_phase(owner_references, instance, config, phase)
    }

    fn make_pod_with_owner_references(owner_references: Vec<OwnerReference>) -> Arc<Pod> {
        Arc::new(Pod {
            spec: Some(PodSpec::default()),
            metadata: ObjectMeta {
                owner_references: Some(owner_references),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    fn make_pod_with_owner_references_and_phase(
        owner_references: Vec<OwnerReference>,
        instance: &str,
        config: &str,
        phase: &str,
    ) -> Pod {
        let pod_status = PodStatus {
            phase: Some(phase.to_string()),
            ..Default::default()
        };
        let mut labels = BTreeMap::new();
        labels.insert(
            AKRI_CONFIGURATION_LABEL_NAME.to_string(),
            config.to_string(),
        );
        labels.insert(AKRI_INSTANCE_LABEL_NAME.to_string(), instance.to_string());
        Pod {
            spec: Some(PodSpec::default()),
            metadata: ObjectMeta {
                owner_references: Some(owner_references),
                name: Some("test-pod".to_string()),
                namespace: Some("test-ns".to_string()),
                labels: Some(labels),
                ..Default::default()
            },
            status: Some(pod_status),
        }
    }

    #[test]
    fn test_get_broker_pod_owner_kind_instance() {
        let owner_references: Vec<OwnerReference> = vec![OwnerReference {
            kind: "Instance".to_string(),
            controller: Some(true),
            ..Default::default()
        }];
        assert_eq!(
            get_broker_pod_owner_kind(make_pod_with_owner_references(owner_references)),
            BrokerPodOwnerKind::Instance
        );
    }

    #[test]
    fn test_get_broker_pod_owner_kind_job() {
        let owner_references: Vec<OwnerReference> = vec![OwnerReference {
            kind: "Job".to_string(),
            controller: Some(true),
            ..Default::default()
        }];
        assert_eq!(
            get_broker_pod_owner_kind(make_pod_with_owner_references(owner_references)),
            BrokerPodOwnerKind::Job
        );
    }

    #[test]
    fn test_get_broker_pod_owner_kind_other() {
        let owner_references: Vec<OwnerReference> = vec![OwnerReference {
            kind: "OtherOwner".to_string(),
            controller: Some(true),
            ..Default::default()
        }];
        assert_eq!(
            get_broker_pod_owner_kind(make_pod_with_owner_references(owner_references)),
            BrokerPodOwnerKind::Other
        );
    }

    // Test that is only labeled as Instance owned if it is the controller OwnerReference
    #[test]
    fn test_get_broker_pod_owner_kind_non_controlling() {
        let owner_references: Vec<OwnerReference> = vec![OwnerReference {
            kind: "Instance".to_string(),
            controller: Some(false),
            ..Default::default()
        }];
        assert_eq!(
            get_broker_pod_owner_kind(make_pod_with_owner_references(owner_references)),
            BrokerPodOwnerKind::Other
        );
    }

    // Test that if multiple OwnerReferences exist, the controlling one is returned.
    #[test]
    fn test_get_broker_pod_owner_kind_both() {
        let owner_references: Vec<OwnerReference> = vec![
            OwnerReference {
                kind: "Instance".to_string(),
                controller: Some(false),
                ..Default::default()
            },
            OwnerReference {
                kind: "Job".to_string(),
                controller: Some(true),
                ..Default::default()
            },
        ];
        assert_eq!(
            get_broker_pod_owner_kind(make_pod_with_owner_references(owner_references)),
            BrokerPodOwnerKind::Job
        );
    }

    fn valid_instance_svc(instance_svc: &Service, instance_name: &str, namespace: &str) -> bool {
        instance_svc.name_unchecked() == format!("{}-svc", instance_name)
            && instance_svc.namespace().unwrap() == namespace
            && instance_svc.owner_references().len() == 1
            && instance_svc.owner_references()[0].kind == "Instance"
            && instance_svc.owner_references()[0].name == instance_name
            && instance_svc.labels().get(AKRI_INSTANCE_LABEL_NAME).unwrap() == instance_name
    }

    fn valid_config_svc(config_svc: &Service, config_name: &str, namespace: &str) -> bool {
        config_svc.name_unchecked() == format!("{}-svc", config_name)
            && config_svc.namespace().unwrap() == namespace
            && config_svc.owner_references().len() == 1
            && config_svc.owner_references()[0].kind == "Configuration"
            && config_svc.owner_references()[0].name == config_name
            && config_svc
                .labels()
                .get(AKRI_CONFIGURATION_LABEL_NAME)
                .unwrap()
                == config_name
    }

    #[tokio::test]
    async fn test_reconcile_applied_unknown_phase() {
        let _ = env_logger::builder().is_test(true).try_init();
        let pod =
            make_pod_with_owners_and_phase("instance_name", "copnfig_name", "Unknown", "Instance");
        let pod_name = pod.name_unchecked();
        let ctx = Arc::new(PodWatcherContext::new(Arc::new(
            MockControllerKubeClient::default(),
        )));
        reconcile_inner(Event::Apply(Arc::new(pod)), ctx.clone())
            .await
            .unwrap();
        assert_eq!(
            ctx.known_pods.read().await.get(&pod_name).unwrap(),
            &PodState::Pending
        );
    }

    #[tokio::test]
    async fn test_reconcile_applied_pending_phase() {
        let _ = env_logger::builder().is_test(true).try_init();
        let pod =
            make_pod_with_owners_and_phase("instance_name", "config_name", "Pending", "Instance");
        let pod_name = pod.name_unchecked();
        let ctx = Arc::new(PodWatcherContext::new(Arc::new(
            MockControllerKubeClient::default(),
        )));
        reconcile_inner(Event::Apply(Arc::new(pod)), ctx.clone())
            .await
            .unwrap();
        assert_eq!(
            ctx.known_pods.read().await.get(&pod_name).unwrap(),
            &PodState::Pending
        );
    }

    // If the pod is in a running state and was previously running, do nothing
    #[tokio::test]
    async fn test_reconcile_applied_running_phase_previously_known() {
        let _ = env_logger::builder().is_test(true).try_init();
        let pod =
            make_pod_with_owners_and_phase("instance_name", "config_name", "Running", "Instance");
        let pod_name = pod.name_unchecked();
        let ctx = Arc::new(PodWatcherContext::new(Arc::new(
            MockControllerKubeClient::default(),
        )));
        ctx.known_pods
            .write()
            .await
            .insert(pod_name.clone(), PodState::Running);
        reconcile_inner(Event::Apply(Arc::new(pod)), ctx.clone())
            .await
            .unwrap();
        assert_eq!(
            ctx.known_pods.read().await.get(&pod_name).unwrap(),
            &PodState::Running
        );
    }

    // If the pod is in a running state and is not in known nodes, ensure services
    // are created
    #[tokio::test]
    async fn test_reconcile_applied_running_phase_unknown() {
        let _ = env_logger::builder().is_test(true).try_init();
        let pod =
            make_pod_with_owners_and_phase("instance_name", "config_name", "Running", "Instance");
        let pod_name = pod.name_unchecked();
        let mut mock = MockControllerKubeClient::default();
        let mut mock_config_api: MockApi<Configuration> = MockApi::new();
        mock_config_api.expect_get().return_once(|_| {
            Ok(Some(make_configuration(
                "config_name",
                "test-ns",
                true,
                true,
            )))
        });
        mock.config
            .expect_namespaced()
            .return_once(|_| Box::new(mock_config_api))
            .withf(|x| x == "test-ns");
        let mut mock_instance_api: MockApi<Instance> = MockApi::new();
        mock_instance_api.expect_get().return_once(|_| {
            Ok(Some(make_instance(
                "instance_name",
                "test-ns",
                "config_name",
            )))
        });
        mock.instance
            .expect_namespaced()
            .return_once(|_| Box::new(mock_instance_api))
            .withf(|x| x == "test-ns");

        let mut mock_svc_api: MockApi<Service> = MockApi::new();
        let mut seq = Sequence::new();
        mock_svc_api
            .expect_apply()
            .times(1)
            .in_sequence(&mut seq)
            .return_once(|_, _| Ok(Service::default()))
            .withf_st(move |x, _| valid_instance_svc(x, "instance_name", "test-ns"));
        mock_svc_api
            .expect_apply()
            .times(1)
            .in_sequence(&mut seq)
            .return_once(|_, _| Ok(Service::default()))
            .withf_st(move |x, _| valid_config_svc(x, "config_name", "test-ns"));
        mock.service
            .expect_namespaced()
            .return_once(|_| Box::new(mock_svc_api))
            .with(mockall::predicate::eq("test-ns"));

        let ctx = Arc::new(PodWatcherContext::new(Arc::new(mock)));

        reconcile_inner(Event::Apply(Arc::new(pod)), ctx.clone())
            .await
            .unwrap();
        assert_eq!(
            ctx.known_pods.read().await.get(&pod_name).unwrap(),
            &PodState::Running
        );
    }

    fn controller_ctx_for_handle_ended_pod_if_needed(
        pod_list: ObjectList<Pod>,
        delete_config_svc: bool,
    ) -> PodWatcherContext {
        let mut mock = MockControllerKubeClient::default();
        let mut mock_config_api: MockApi<Configuration> = MockApi::new();
        mock_config_api.expect_get().return_once(|_| {
            Ok(Some(make_configuration(
                "config_name",
                "test-ns",
                true,
                true,
            )))
        });
        mock.config
            .expect_namespaced()
            .return_once(|_| Box::new(mock_config_api))
            .with(mockall::predicate::eq("test-ns"));
        let mut mock_instance_api: MockApi<Instance> = MockApi::new();
        mock_instance_api.expect_get().return_once(|_| {
            Ok(Some(make_instance(
                "instance_name",
                "test-ns",
                "config_name",
            )))
        });
        mock.instance
            .expect_namespaced()
            .return_once(|_| Box::new(mock_instance_api))
            .with(mockall::predicate::eq("test-ns"));
        let mut mock_pod_api: MockApi<Pod> = MockApi::new();
        mock_pod_api.expect_list().return_once(|_| Ok(pod_list));
        mock.pod.expect_all().return_once(|| Box::new(mock_pod_api));

        let mut mock_svc_api: MockApi<Service> = MockApi::new();
        let mut seq = Sequence::new();
        if delete_config_svc {
            mock_svc_api
                .expect_delete()
                .times(1)
                .in_sequence(&mut seq)
                .return_once(|_| Ok(either::Left(Service::default())))
                .withf_st(move |x| x == create_service_app_name("config_name"));
        }
        mock_svc_api
            .expect_delete()
            .times(1)
            .in_sequence(&mut seq)
            .return_once(|_| Ok(either::Left(Service::default())))
            .withf_st(move |x| x == create_service_app_name("instance_name"));
        mock.service
            .expect_namespaced()
            .return_once(|_| Box::new(mock_svc_api))
            .with(mockall::predicate::eq("test-ns"));
        PodWatcherContext::new(Arc::new(mock))
    }

    async fn test_reconcile_applied_terminated_phases(phase: &str) {
        let _ = env_logger::builder().is_test(true).try_init();
        // NOTE: setting Job kind for the Pod owner to ensure `handle_instance_change` is not called
        let pod1 = make_pod_with_owners_and_phase("instance_name", "config_name", phase, "Job");
        let pod_name = pod1.name_unchecked();
        // Unrelated pod that should be filtered out
        let pod2 = make_pod_with_owners_and_phase("foo", "config_name", phase, "Job");
        let pod_list = make_obj_list(vec![pod1.clone(), pod2]);
        let ctx = Arc::new(controller_ctx_for_handle_ended_pod_if_needed(
            pod_list, true,
        ));
        // Configure the pod as previously running
        ctx.known_pods
            .write()
            .await
            .insert(pod_name.clone(), PodState::Running);
        reconcile_inner(Event::Apply(Arc::new(pod1)), ctx.clone())
            .await
            .unwrap();
        assert_eq!(
            ctx.known_pods.read().await.get(&pod_name).unwrap(),
            &PodState::Ended
        );
    }

    // If the pod is in a succeeded state and was previously known, ensure services
    // are deleted
    #[tokio::test]
    async fn test_reconcile_applied_succeeded_phase() {
        test_reconcile_applied_terminated_phases("Succeeded").await;
    }

    // If the pod is in a failed state and was previously known, ensure services
    // are deleted
    #[tokio::test]
    async fn test_reconcile_applied_failed_phase() {
        test_reconcile_applied_terminated_phases("Failed").await;
    }

    #[tokio::test]
    async fn test_reconcile_applied_failed_phase_pods_with_pods_not_terminating() {
        let _ = env_logger::builder().is_test(true).try_init();
        // NOTE: setting Job kind for the Pod owner to ensure `handle_instance_change` is not called
        let pod1 = make_pod_with_owners_and_phase("instance_name", "config_name", "Failed", "Job");
        let pod_name = pod1.name_unchecked();
        // Have one pod of the config still running to ensure that the config service is not deleted
        let pod2 = make_pod_with_owners_and_phase("foo", "config_name", "Running", "Job");
        let pod_list = make_obj_list(vec![pod1.clone(), pod2]);
        let ctx = Arc::new(controller_ctx_for_handle_ended_pod_if_needed(
            pod_list, false,
        ));
        // Configure the pod as previously running
        ctx.known_pods
            .write()
            .await
            .insert(pod_name.clone(), PodState::Running);
        reconcile_inner(Event::Apply(Arc::new(pod1)), ctx.clone())
            .await
            .unwrap();
        assert_eq!(
            ctx.known_pods.read().await.get(&pod_name).unwrap(),
            &PodState::Ended
        );
    }

    #[tokio::test]
    async fn test_reconcile_cleanup() {
        let _ = env_logger::builder().is_test(true).try_init();
        let phase = "Succeeded";
        // NOTE: setting Job kind for the Pod owner to ensure `handle_instance_change` is not called
        let pod1 = make_pod_with_owners_and_phase("instance_name", "config_name", phase, "Job");
        let pod_name = pod1.name_unchecked();
        // Unrelated pod that should be filtered out
        let pod2 = make_pod_with_owners_and_phase("foo", "config_name", phase, "Job");
        let pod_list = make_obj_list(vec![pod1.clone(), pod2]);
        let ctx = Arc::new(controller_ctx_for_handle_ended_pod_if_needed(
            pod_list, true,
        ));
        // Configure the pod as previously running
        ctx.known_pods
            .write()
            .await
            .insert(pod_name.clone(), PodState::Running);
        reconcile_inner(Event::Cleanup(Arc::new(pod1)), ctx.clone())
            .await
            .unwrap();
        assert_eq!(
            ctx.known_pods.read().await.get(&pod_name).unwrap(),
            &PodState::Deleted
        );
    }

    // TODO: directly test cleanup_svc_if_unsupported
}
