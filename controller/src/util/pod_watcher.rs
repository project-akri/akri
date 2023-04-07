use akri_shared::{
    akri::{
        configuration::Configuration,
        retry::{random_delay, MAX_INSTANCE_UPDATE_TRIES},
    },
    k8s,
    k8s::{
        pod::{AKRI_CONFIGURATION_LABEL_NAME, AKRI_INSTANCE_LABEL_NAME},
        service, KubeInterface, OwnershipInfo, OwnershipType,
    },
};
use async_std::sync::Mutex;
use futures::{StreamExt, TryStreamExt};
use k8s_openapi::api::core::v1::{Pod, ServiceSpec};
use kube::api::{Api, ListParams};
use kube_runtime::watcher::{default_backoff, watcher, Event};
use kube_runtime::WatchStreamExt;
use log::{error, info, trace};
use std::{collections::HashMap, sync::Arc};

type PodSlice = [Pod];

/// Pod states that BrokerPodWatcher is interested in
///
/// PodState describes the various states that the controller can
/// react to for Pods.
#[derive(Clone, Debug, PartialEq)]
enum PodState {
    /// Pod is in Pending state and no action is needed.
    Pending,
    /// Pod is in Running state and needs to ensure that
    /// instance and configuration services are running
    Running,
    /// Pod is in Failed/Completed/Succeeded state and
    /// needs to remove any instance and configuration
    /// services that are not supported by other Running
    /// Pods.  Also, at this point, if an Instance still
    /// exists, instance_action::handle_instance_change
    /// needs to be called to ensure that Pods are
    /// restarted
    Ended,
    /// Pod is in Deleted state and needs to remove any
    /// instance and configuration services that are not
    /// supported by other Running Pods. Also, at this
    /// point, if an Instance still exists, and the Pod is
    /// owned by the Instance,
    /// instance_action::handle_instance_change needs to be
    /// called to ensure that Pods are restarted. Akri
    /// places an Instance OwnerReference on all the Pods it
    /// deploys. This declares that the Instance owns that
    /// Pod and Akri's Controller explicitly manages its
    /// deployment. However, if the Pod is not owned by the
    /// Instance, Akri should not assume retry logic and
    /// should cease action. The owning object (ie Job) will
    /// handle retries as necessary.
    Deleted,
}

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
fn get_broker_pod_owner_kind(pod: &Pod) -> BrokerPodOwnerKind {
    let instance_kind = "Instance".to_string();
    let job_kind = "Job".to_string();
    match &pod.metadata.owner_references {
        Some(or) => {
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
        None => BrokerPodOwnerKind::Other,
    }
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
#[derive(Debug)]
pub struct BrokerPodWatcher {
    known_pods: HashMap<String, PodState>,
}

impl BrokerPodWatcher {
    /// Create new instance of BrokerPodWatcher
    pub fn new() -> Self {
        BrokerPodWatcher {
            known_pods: HashMap::new(),
        }
    }

    /// This watches for broker Pod events
    pub async fn watch(&mut self) -> anyhow::Result<()> {
        trace!("watch - enter");
        let kube_interface = k8s::KubeImpl::new().await?;
        let resource = Api::<Pod>::all(kube_interface.get_kube_client());
        let watcher = watcher(
            resource,
            ListParams::default().labels(AKRI_CONFIGURATION_LABEL_NAME),
        )
        .backoff(default_backoff());
        let mut informer = watcher.boxed();
        let synchronization = Arc::new(Mutex::new(()));
        let mut first_event = true;

        loop {
            let event = match informer.try_next().await {
                Err(e) => {
                    error!("Error during watch: {}", e);
                    continue;
                }
                Ok(None) => return Err(anyhow::anyhow!("Watch stream ended")),
                Ok(Some(event)) => event,
            };
            let _lock = synchronization.lock().await;
            self.handle_pod(event, &kube_interface, &mut first_event)
                .await?;
        }
    }

    /// Gets Pods phase and returns "Unknown" if no phase exists
    fn get_pod_phase(&mut self, pod: &Pod) -> String {
        if pod.status.is_some() {
            pod.status
                .as_ref()
                .unwrap()
                .phase
                .as_ref()
                .unwrap_or(&"Unknown".to_string())
                .to_string()
        } else {
            "Unknown".to_string()
        }
    }

    /// This takes an event off the Pod stream.  If a Pod is newly Running, ensure that
    /// the instance and configuration services are running.  If a Pod is no longer Running,
    /// ensure that the instance and configuration services are removed as needed.
    async fn handle_pod(
        &mut self,
        event: Event<Pod>,
        kube_interface: &impl KubeInterface,
        first_event: &mut bool,
    ) -> anyhow::Result<()> {
        trace!("handle_pod - enter [event: {:?}]", event);
        match event {
            Event::Applied(pod) => {
                info!(
                    "handle_pod - pod {:?} added or modified",
                    &pod.metadata.name
                );
                let phase = self.get_pod_phase(&pod);
                trace!("handle_pod - pod phase {:?}", &phase);
                match phase.as_str() {
                    "Unknown" | "Pending" => {
                        self.known_pods.insert(
                            pod.metadata.name.clone().ok_or_else(|| {
                                anyhow::format_err!("Pod {:?} does not have name", pod)
                            })?,
                            PodState::Pending,
                        );
                    }
                    "Running" => {
                        self.handle_running_pod_if_needed(&pod, kube_interface)
                            .await?;
                    }
                    "Succeeded" | "Failed" => {
                        self.handle_ended_pod_if_needed(&pod, kube_interface)
                            .await?;
                    }
                    _ => {
                        trace!("handle_pod - Unknown phase: {:?}", &phase);
                    }
                }
            }
            Event::Deleted(pod) => {
                info!("handle_pod - Deleted: {:?}", &pod.metadata.name);
                self.handle_deleted_pod_if_needed(&pod, kube_interface)
                    .await?;
            }
            Event::Restarted(pods) => {
                if *first_event {
                    info!(
                        "handle_pod - pod watcher [re]started. Pods are : {:?}",
                        pods
                    );
                } else {
                    return Err(anyhow::anyhow!(
                        "Pod watcher restarted - throwing error to restart controller"
                    ));
                }
            }
        };
        *first_event = false;
        Ok(())
    }

    /// This ensures that handle_running_pod is called only once for
    /// any Pod as it exits the Running phase.
    async fn handle_running_pod_if_needed(
        &mut self,
        pod: &Pod,
        kube_interface: &impl KubeInterface,
    ) -> anyhow::Result<()> {
        trace!("handle_running_pod_if_needed - enter");
        let pod_name = pod
            .metadata
            .name
            .clone()
            .ok_or_else(|| anyhow::format_err!("Pod {:?} does not have name", pod))?;
        let last_known_state = self.known_pods.get(&pod_name).unwrap_or(&PodState::Pending);
        trace!(
            "handle_running_pod_if_needed - last_known_state: {:?}",
            &last_known_state
        );
        // Ensure that, for each pod, handle_running_pod is called once
        // per transition into the Running state
        if last_known_state != &PodState::Running {
            trace!("handle_running_pod_if_needed - call handle_running_pod");
            self.handle_running_pod(pod, kube_interface).await?;
            self.known_pods.insert(pod_name, PodState::Running);
        }
        Ok(())
    }

    /// This ensures that handle_non_running_pod is called only once for
    /// any Pod as it enters the Ended phase.  Note that handle_non_running_pod
    /// will likely be called twice as a Pod leaves the Running phase, that is
    /// expected and accepted.
    async fn handle_ended_pod_if_needed(
        &mut self,
        pod: &Pod,
        kube_interface: &impl KubeInterface,
    ) -> anyhow::Result<()> {
        trace!("handle_ended_pod_if_needed - enter");
        let pod_name = pod
            .metadata
            .name
            .clone()
            .ok_or_else(|| anyhow::format_err!("Pod {:?} does not have name", pod))?;
        let last_known_state = self.known_pods.get(&pod_name).unwrap_or(&PodState::Pending);
        trace!(
            "handle_ended_pod_if_needed - last_known_state: {:?}",
            &last_known_state
        );
        // Ensure that, for each pod, handle_non_running_pod is called once
        // per transition into the Ended state
        if last_known_state != &PodState::Ended {
            trace!("handle_ended_pod_if_needed - call handle_non_running_pod");
            self.handle_non_running_pod(pod, kube_interface).await?;
            self.known_pods.insert(pod_name, PodState::Ended);
        }
        Ok(())
    }

    /// This ensures that handle_non_running_pod is called only once for
    /// any Pod as it enters the Ended phase.  Note that handle_non_running_pod
    /// will likely be called twice as a Pod leaves the Running phase, that is
    /// expected and accepted.
    async fn handle_deleted_pod_if_needed(
        &mut self,
        pod: &Pod,
        kube_interface: &impl KubeInterface,
    ) -> anyhow::Result<()> {
        trace!("handle_deleted_pod_if_needed - enter");
        let pod_name = pod
            .metadata
            .name
            .clone()
            .ok_or_else(|| anyhow::format_err!("Pod {:?} does not have name", pod))?;
        let last_known_state = self.known_pods.get(&pod_name).unwrap_or(&PodState::Pending);
        trace!(
            "handle_deleted_pod_if_needed - last_known_state: {:?}",
            &last_known_state
        );
        // Ensure that, for each pod, handle_non_running_pod is called once
        // per transition into the Deleted state
        if last_known_state != &PodState::Deleted {
            trace!("handle_deleted_pod_if_needed - call handle_non_running_pod");
            self.handle_non_running_pod(pod, kube_interface).await?;
            self.known_pods.insert(pod_name, PodState::Deleted);
        }
        Ok(())
    }

    /// Get instance id and configuration name from Pod annotations, return
    /// error if the annotations are not found.
    fn get_instance_and_configuration_from_pod(
        &self,
        pod: &Pod,
    ) -> anyhow::Result<(String, String)> {
        trace!("get_instance_and_configuration_from_pod - enter");
        let labels = pod
            .metadata
            .labels
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Pod doesn't have labels"))?;
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
    async fn handle_non_running_pod(
        &self,
        pod: &Pod,
        kube_interface: &impl KubeInterface,
    ) -> anyhow::Result<()> {
        trace!("handle_non_running_pod - enter");
        let namespace = pod.metadata.namespace.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Namespace not found for pod: {:?}", &pod.metadata.name)
        })?;
        let (instance_id, config_name) = self.get_instance_and_configuration_from_pod(pod)?;
        self.find_pods_and_cleanup_svc_if_unsupported(
            &instance_id,
            &config_name,
            namespace,
            true,
            kube_interface,
        )
        .await?;
        self.find_pods_and_cleanup_svc_if_unsupported(
            &instance_id,
            &config_name,
            namespace,
            false,
            kube_interface,
        )
        .await?;

        // Only redeploy Pods that are managed by the Akri Controller (controlled by an Instance OwnerReference)
        if get_broker_pod_owner_kind(pod) == BrokerPodOwnerKind::Instance {
            // Make sure instance has required Pods
            if let Ok(instance) = kube_interface.find_instance(&instance_id, namespace).await {
                super::instance_action::handle_instance_change(
                    &instance,
                    &super::instance_action::InstanceAction::Update,
                    kube_interface,
                )
                .await?;
            }
        }
        Ok(())
    }

    /// This searches existing Pods to determine if there are
    /// Services that need to be removed because they lack supporting
    /// Pods.  If any are found, the Service is removed.
    async fn find_pods_and_cleanup_svc_if_unsupported(
        &self,
        instance_id: &str,
        configuration_name: &str,
        namespace: &str,
        handle_instance_svc: bool,
        kube_interface: &impl KubeInterface,
    ) -> anyhow::Result<()> {
        trace!("find_pods_and_cleanup_svc_if_unsupported - enter");
        let (label, value) = if handle_instance_svc {
            (AKRI_INSTANCE_LABEL_NAME, instance_id)
        } else {
            (AKRI_CONFIGURATION_LABEL_NAME, configuration_name)
        };

        // Clean up instance service if there are no pods anymore
        let selector = format!("{}={}", label, value);
        trace!(
            "find_pods_and_cleanup_svc_if_unsupported - find_pods_with_label({})",
            selector
        );
        let pods = kube_interface.find_pods_with_label(&selector).await?;
        trace!(
            "find_pods_and_cleanup_svc_if_unsupported - found {} pods",
            pods.items.len()
        );

        let svc_name = service::create_service_app_name(
            configuration_name,
            instance_id,
            "svc",
            handle_instance_svc,
        );

        self.cleanup_svc_if_unsupported(&pods.items, &svc_name, namespace, kube_interface)
            .await
    }

    /// This determines if there are Services that need to be removed because
    /// they lack supporting Pods.  If any are found, the Service is removed.
    async fn cleanup_svc_if_unsupported(
        &self,
        pods: &PodSlice,
        svc_name: &str,
        svc_namespace: &str,
        kube_interface: &impl KubeInterface,
    ) -> anyhow::Result<()> {
        // Find the number of non-Terminating pods, if there aren't any (the only pods that exist are Terminating), we should remove the device capability service
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
        trace!(
            "cleanup_svc_if_unsupported - num_non_terminating_pods: {}",
            num_non_terminating_pods
        );

        if num_non_terminating_pods == 0 {
            trace!(
                "cleanup_svc_if_unsupported - service::remove_service app_name={:?}, namespace={:?}",
                &svc_name, &svc_namespace
            );
            kube_interface
                .remove_service(svc_name, svc_namespace)
                .await?;
            trace!("cleanup_svc_if_unsupported - service::remove_service succeeded");
        }
        Ok(())
    }

    /// This is called when a Pod enters the Running phase and ensures
    /// that isntance and configuration services are running as specified
    /// by the configuration.
    async fn handle_running_pod(
        &self,
        pod: &Pod,
        kube_interface: &impl KubeInterface,
    ) -> anyhow::Result<()> {
        trace!("handle_running_pod - enter");
        let namespace = pod.metadata.namespace.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Namespace not found for pod: {:?}", &pod.metadata.name)
        })?;
        let (instance_name, configuration_name) =
            self.get_instance_and_configuration_from_pod(pod)?;
        let configuration = match kube_interface
            .find_configuration(&configuration_name, namespace)
            .await
        {
            Ok(config) => config,
            _ => {
                // In this scenario, a configuration has likely been deleted in the middle of handle_running_pod.
                // There is no need to propogate the error and bring down the Controller.
                trace!(
                    "handle_running_pod - no configuration found for {}",
                    &configuration_name
                );
                return Ok(());
            }
        };
        let instance = match kube_interface
            .find_instance(&instance_name, namespace)
            .await
        {
            Ok(instance) => instance,
            _ => {
                // In this scenario, a instance has likely been deleted in the middle of handle_running_pod.
                // There is no need to propogate the error and bring down the Controller.
                trace!(
                    "handle_running_pod - no instance found for {}",
                    &instance_name
                );
                return Ok(());
            }
        };
        let instance_uid = instance
            .metadata
            .uid
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("UID not found for instance: {}", instance_name))?;
        self.add_instance_and_configuration_services(
            &instance_name,
            instance_uid,
            namespace,
            &configuration_name,
            &configuration,
            kube_interface,
        )
        .await?;

        Ok(())
    }

    /// This creates new service or updates existing service with ownership.
    #[allow(clippy::too_many_arguments)]
    async fn create_or_update_service(
        &self,
        instance_name: &str,
        configuration_name: &str,
        namespace: &str,
        label_name: &str,
        label_value: &str,
        ownership: OwnershipInfo,
        service_spec: &ServiceSpec,
        is_instance_service: bool,
        kube_interface: &impl KubeInterface,
    ) -> anyhow::Result<()> {
        trace!(
            "create_or_update_service - instance={:?} with ownership:{:?}",
            instance_name,
            &ownership
        );

        let mut create_new_service = true;
        if let Ok(existing_svcs) = kube_interface
            .find_services(&format!("{}={}", label_name, label_value))
            .await
        {
            for existing_svc in existing_svcs {
                let mut existing_svc = existing_svc.clone();
                let svc_name = existing_svc.metadata.name.clone().ok_or_else(|| {
                    anyhow::format_err!("Service {:?} does not have name", existing_svc)
                })?;
                let svc_namespace = existing_svc.metadata.namespace.as_ref().unwrap().clone();
                trace!(
                    "create_or_update_service - Update existing svc={:?}",
                    &svc_name
                );
                service::update_ownership(&mut existing_svc, ownership.clone(), true)?;
                trace!("create_or_update_service - calling service::update_service name:{} namespace: {}", &svc_name, &svc_namespace);
                kube_interface
                    .update_service(&existing_svc, &svc_name, &svc_namespace)
                    .await?;
                trace!("create_or_update_service - service::update_service succeeded");
                create_new_service = false;
            }
        }

        if create_new_service {
            let new_instance_svc = service::create_new_service_from_spec(
                namespace,
                instance_name,
                configuration_name,
                ownership.clone(),
                service_spec,
                is_instance_service,
            )?;
            trace!(
                "create_or_update_service - New instance svc spec={:?}",
                new_instance_svc
            );

            kube_interface
                .create_service(&new_instance_svc, namespace)
                .await?;
            trace!("create_or_update_service - service::create_service succeeded");
        }
        Ok(())
    }

    /// This creates the broker Service and the capability Service.
    async fn add_instance_and_configuration_services(
        &self,
        instance_name: &str,
        instance_uid: &str,
        namespace: &str,
        configuration_name: &str,
        configuration: &Configuration,
        kube_interface: &impl KubeInterface,
    ) -> anyhow::Result<()> {
        trace!(
            "add_instance_and_configuration_services - instance={:?}",
            instance_name
        );
        if let Some(instance_service_spec) = &configuration.spec.instance_service_spec {
            let ownership = OwnershipInfo::new(
                OwnershipType::Instance,
                instance_name.to_string(),
                instance_uid.to_string(),
            );
            // Try up to MAX_INSTANCE_UPDATE_TRIES times to update/create/get instance
            for x in 0..MAX_INSTANCE_UPDATE_TRIES {
                match self
                    .create_or_update_service(
                        instance_name,
                        configuration_name,
                        namespace,
                        AKRI_INSTANCE_LABEL_NAME,
                        instance_name,
                        ownership.clone(),
                        instance_service_spec,
                        true,
                        kube_interface,
                    )
                    .await
                {
                    Ok(_) => break,
                    Err(e) => {
                        if x == (MAX_INSTANCE_UPDATE_TRIES - 1) {
                            return Err(e);
                        }
                        random_delay().await;
                    }
                }
            }
        }

        if let Some(configuration_service_spec) = &configuration.spec.configuration_service_spec {
            let configuration_uid = configuration.metadata.uid.as_ref().ok_or_else(|| {
                anyhow::anyhow!("UID not found for configuration: {}", configuration_name)
            })?;
            let ownership = OwnershipInfo::new(
                OwnershipType::Configuration,
                configuration_name.to_string(),
                configuration_uid.clone(),
            );
            // Try up to MAX_INSTANCE_UPDATE_TRIES times to update/create/get instance
            for x in 0..MAX_INSTANCE_UPDATE_TRIES {
                match self
                    .create_or_update_service(
                        instance_name,
                        configuration_name,
                        namespace,
                        AKRI_CONFIGURATION_LABEL_NAME,
                        configuration_name,
                        ownership.clone(),
                        configuration_service_spec,
                        false,
                        kube_interface,
                    )
                    .await
                {
                    Ok(_) => break,
                    Err(e) => {
                        if x == (MAX_INSTANCE_UPDATE_TRIES - 1) {
                            return Err(e);
                        }
                        random_delay().await;
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::shared_test_utils::config_for_tests;
    use super::super::shared_test_utils::config_for_tests::PodList;
    use super::*;
    use akri_shared::{k8s::MockKubeInterface, os::file};
    use k8s_openapi::api::core::v1::PodSpec;
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::{ObjectMeta, OwnerReference};

    fn create_pods_with_phase(result_file: &'static str, specified_phase: &'static str) -> PodList {
        let pods_json = file::read_file_to_string(result_file);
        let phase_adjusted_json = pods_json.replace(
            "\"phase\": \"Running\"",
            &format!("\"phase\": \"{}\"", specified_phase),
        );
        let pods: PodList = serde_json::from_str(&phase_adjusted_json).unwrap();
        pods
    }

    fn make_pod_with_owner_references(owner_references: Vec<OwnerReference>) -> Pod {
        Pod {
            spec: Some(PodSpec::default()),
            metadata: ObjectMeta {
                owner_references: Some(owner_references),
                ..Default::default()
            },
            ..Default::default()
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
            get_broker_pod_owner_kind(&make_pod_with_owner_references(owner_references)),
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
            get_broker_pod_owner_kind(&make_pod_with_owner_references(owner_references)),
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
            get_broker_pod_owner_kind(&make_pod_with_owner_references(owner_references)),
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
            get_broker_pod_owner_kind(&make_pod_with_owner_references(owner_references)),
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
            get_broker_pod_owner_kind(&make_pod_with_owner_references(owner_references)),
            BrokerPodOwnerKind::Job
        );
    }

    // Test that watcher errors on restarts unless it is the first restart (aka initial startup)
    #[tokio::test]
    async fn test_handle_watcher_restart() {
        let _ = env_logger::builder().is_test(true).try_init();
        let mut pod_watcher = BrokerPodWatcher::new();
        let mut first_event = true;
        assert!(pod_watcher
            .handle_pod(
                Event::Restarted(Vec::new()),
                &MockKubeInterface::new(),
                &mut first_event
            )
            .await
            .is_ok());
        first_event = false;
        assert!(pod_watcher
            .handle_pod(
                Event::Restarted(Vec::new()),
                &MockKubeInterface::new(),
                &mut first_event
            )
            .await
            .is_err());
    }

    #[tokio::test]
    async fn test_handle_pod_added_unready() {
        let _ = env_logger::builder().is_test(true).try_init();

        for phase in &["Unknown", "Pending"] {
            let pod_list = create_pods_with_phase(
                "../test/json/running-pod-list-for-config-a-local.json",
                phase,
            );
            let pod = pod_list.items.first().unwrap().clone();
            let mut pod_watcher = BrokerPodWatcher::new();
            trace!(
                "test_handle_pod_added_unready phase:{}, Event::Applied",
                &phase
            );
            pod_watcher
                .handle_pod(Event::Applied(pod), &MockKubeInterface::new(), &mut false)
                .await
                .unwrap();
            trace!(
                "test_handle_pod_added_unready pod_watcher:{:?}",
                &pod_watcher
            );
            assert_eq!(1, pod_watcher.known_pods.len());
            assert_eq!(
                &PodState::Pending,
                pod_watcher
                    .known_pods
                    .get(&"config-a-b494b6-pod".to_string())
                    .unwrap()
            )
        }
    }

    #[tokio::test]
    async fn test_handle_pod_modified_unready() {
        let _ = env_logger::builder().is_test(true).try_init();

        for phase in &["Unknown", "Pending"] {
            let pod_list = create_pods_with_phase(
                "../test/json/running-pod-list-for-config-a-local.json",
                phase,
            );
            let pod = pod_list.items.first().unwrap().clone();
            let mut pod_watcher = BrokerPodWatcher::new();
            trace!(
                "test_handle_pod_modified_unready phase:{}, Event::Applied",
                &phase
            );
            pod_watcher
                .handle_pod(Event::Applied(pod), &MockKubeInterface::new(), &mut false)
                .await
                .unwrap();
            trace!(
                "test_handle_pod_added_unready pod_watcher:{:?}",
                &pod_watcher
            );
            assert_eq!(1, pod_watcher.known_pods.len());
            assert_eq!(
                &PodState::Pending,
                pod_watcher
                    .known_pods
                    .get(&"config-a-b494b6-pod".to_string())
                    .unwrap()
            )
        }
    }

    #[tokio::test]
    async fn test_handle_pod_modified_ready() {
        let _ = env_logger::builder().is_test(true).try_init();

        let pods_json =
            file::read_file_to_string("../test/json/running-pod-list-for-config-a-local.json");
        let pod_list: PodList = serde_json::from_str(&pods_json).unwrap();
        let pod = pod_list.items.first().unwrap().clone();
        let mut pod_watcher = BrokerPodWatcher::new();
        let mut mock = MockKubeInterface::new();
        configure_for_handle_pod(
            &mut mock,
            &HandlePod {
                running: Some(HandlePodRunning {
                    find_config_name: "config-a",
                    find_config_namespace: "config-a-namespace",
                    find_config_result: "../test/json/config-a.json",
                    find_config_error: false,

                    find_instance_name: "config-a-b494b6",
                    find_instance_result: "../test/json/local-instance.json",

                    find_instance_service: FindServices {
                        find_services_selector: "akri.sh/instance=config-a-b494b6",
                        find_services_result: "../test/json/empty-list.json",
                        find_services_error: false,
                    },
                    new_instance_svc_name: "config-a-b494b6-svc",

                    find_configuration_service: FindServices {
                        find_services_selector: "akri.sh/configuration=config-a",
                        find_services_result: "../test/json/empty-list.json",
                        find_services_error: false,
                    },
                    new_configuration_svc_name: "config-a-svc",
                }),
                ended: None,
            },
        );

        pod_watcher
            .handle_pod(Event::Applied(pod), &mock, &mut false)
            .await
            .unwrap();
        trace!(
            "test_handle_pod_added_unready pod_watcher:{:?}",
            &pod_watcher
        );
        assert_eq!(1, pod_watcher.known_pods.len());
        assert_eq!(
            &PodState::Running,
            pod_watcher
                .known_pods
                .get(&"config-a-b494b6-pod".to_string())
                .unwrap()
        )
    }

    #[tokio::test]
    async fn test_handle_pod_modified_ready_no_config() {
        let _ = env_logger::builder().is_test(true).try_init();

        let pods_json =
            file::read_file_to_string("../test/json/running-pod-list-for-config-a-local.json");
        let pod_list: PodList = serde_json::from_str(&pods_json).unwrap();
        let pod = pod_list.items.first().unwrap().clone();
        let mut pod_watcher = BrokerPodWatcher::new();
        let mut mock = MockKubeInterface::new();
        configure_for_handle_pod(
            &mut mock,
            &HandlePod {
                running: Some(HandlePodRunning {
                    find_config_name: "config-a",
                    find_config_namespace: "config-a-namespace",
                    find_config_result: "../test/json/config-a.json",
                    find_config_error: true,

                    find_instance_name: "config-a-b494b6",
                    find_instance_result: "../test/json/local-instance.json",

                    find_instance_service: FindServices {
                        find_services_selector: "akri.sh/instance=config-a-b494b6",
                        find_services_result: "../test/json/empty-list.json",
                        find_services_error: false,
                    },
                    new_instance_svc_name: "config-a-b494b6-svc",

                    find_configuration_service: FindServices {
                        find_services_selector: "akri.sh/configuration=config-a",
                        find_services_result: "../test/json/empty-list.json",
                        find_services_error: false,
                    },
                    new_configuration_svc_name: "config-a-svc",
                }),
                ended: None,
            },
        );

        pod_watcher
            .handle_pod(Event::Applied(pod), &mock, &mut false)
            .await
            .unwrap();
        trace!(
            "test_handle_pod_modified_ready_no_config pod_watcher:{:?}",
            &pod_watcher
        );
        assert_eq!(1, pod_watcher.known_pods.len());
        assert_eq!(
            &PodState::Running,
            pod_watcher
                .known_pods
                .get(&"config-a-b494b6-pod".to_string())
                .unwrap()
        )
    }

    #[tokio::test]
    async fn test_handle_pod_modified_failed() {
        let _ = env_logger::builder().is_test(true).try_init();

        let pod_list = create_pods_with_phase(
            "../test/json/running-pod-list-for-config-a-local.json",
            "Failed",
        );
        let pod = pod_list.items.first().unwrap().clone();
        let mut pod_watcher = BrokerPodWatcher::new();
        let mut mock = MockKubeInterface::new();
        configure_for_handle_pod(
            &mut mock,
            &HandlePod {
                running: None,
                ended: Some(CleanupServices {
                    find_svc_selector: "controller=akri.sh",
                    find_svc_result: "../test/json/running-svc-list-for-config-a-local.json",
                    cleanup_services: vec![
                        CleanupService {
                            find_pod_selector: "akri.sh/configuration=config-a",
                            find_pod_result: "../test/json/empty-list.json",
                            remove_service: Some(RemoveService {
                                remove_service_name: "config-a-svc",
                                remove_service_namespace: "config-a-namespace",
                            }),
                        },
                        CleanupService {
                            find_pod_selector: "akri.sh/instance=config-a-b494b6",
                            find_pod_result: "../test/json/empty-list.json",
                            remove_service: Some(RemoveService {
                                remove_service_name: "config-a-b494b6-svc",
                                remove_service_namespace: "config-a-namespace",
                            }),
                        },
                    ],
                    find_instance_id: "config-a-b494b6",
                    find_instance_namespace: "config-a-namespace",
                    find_instance_result: "",
                    find_instance_result_error: true,
                }),
            },
        );

        pod_watcher
            .handle_pod(Event::Applied(pod), &mock, &mut false)
            .await
            .unwrap();
        trace!(
            "test_handle_pod_added_unready pod_watcher:{:?}",
            &pod_watcher
        );
        assert_eq!(1, pod_watcher.known_pods.len());
        assert_eq!(
            &PodState::Ended,
            pod_watcher
                .known_pods
                .get(&"config-a-b494b6-pod".to_string())
                .unwrap()
        )
    }

    #[tokio::test]
    async fn test_handle_pod_deleted() {
        let _ = env_logger::builder().is_test(true).try_init();

        let pod_list = create_pods_with_phase(
            "../test/json/running-pod-list-for-config-a-local.json",
            "Failed",
        );
        let pod = pod_list.items.first().unwrap().clone();
        let mut pod_watcher = BrokerPodWatcher::new();
        let mut mock = MockKubeInterface::new();
        configure_for_handle_pod(
            &mut mock,
            &HandlePod {
                running: None,
                ended: Some(CleanupServices {
                    find_svc_selector: "controller=akri.sh",
                    find_svc_result: "../test/json/running-svc-list-for-config-a-local.json",
                    cleanup_services: vec![
                        CleanupService {
                            find_pod_selector: "akri.sh/configuration=config-a",
                            find_pod_result: "../test/json/empty-list.json",
                            remove_service: Some(RemoveService {
                                remove_service_name: "config-a-svc",
                                remove_service_namespace: "config-a-namespace",
                            }),
                        },
                        CleanupService {
                            find_pod_selector: "akri.sh/instance=config-a-b494b6",
                            find_pod_result: "../test/json/empty-list.json",
                            remove_service: Some(RemoveService {
                                remove_service_name: "config-a-b494b6-svc",
                                remove_service_namespace: "config-a-namespace",
                            }),
                        },
                    ],
                    find_instance_id: "config-a-b494b6",
                    find_instance_namespace: "config-a-namespace",
                    find_instance_result: "",
                    find_instance_result_error: true,
                }),
            },
        );

        pod_watcher
            .handle_pod(Event::Deleted(pod), &mock, &mut false)
            .await
            .unwrap();
        trace!(
            "test_handle_pod_added_unready pod_watcher:{:?}",
            &pod_watcher
        );
        assert_eq!(1, pod_watcher.known_pods.len());
        assert_eq!(
            &PodState::Deleted,
            pod_watcher
                .known_pods
                .get(&"config-a-b494b6-pod".to_string())
                .unwrap()
        )
    }

    #[tokio::test]
    async fn test_handle_pod_succeeded() {
        let _ = env_logger::builder().is_test(true).try_init();

        let pod_list = create_pods_with_phase(
            "../test/json/running-pod-list-for-config-a-local.json",
            "Succeeded",
        );
        let pod = pod_list.items.first().unwrap().clone();
        let mut pod_watcher = BrokerPodWatcher::new();
        let mut mock = MockKubeInterface::new();
        configure_for_handle_pod(
            &mut mock,
            &HandlePod {
                running: None,
                ended: Some(CleanupServices {
                    find_svc_selector: "controller=akri.sh",
                    find_svc_result: "../test/json/running-svc-list-for-config-a-local.json",
                    cleanup_services: vec![
                        CleanupService {
                            find_pod_selector: "akri.sh/configuration=config-a",
                            find_pod_result: "../test/json/empty-list.json",
                            remove_service: Some(RemoveService {
                                remove_service_name: "config-a-svc",
                                remove_service_namespace: "config-a-namespace",
                            }),
                        },
                        CleanupService {
                            find_pod_selector: "akri.sh/instance=config-a-b494b6",
                            find_pod_result: "../test/json/empty-list.json",
                            remove_service: Some(RemoveService {
                                remove_service_name: "config-a-b494b6-svc",
                                remove_service_namespace: "config-a-namespace",
                            }),
                        },
                    ],
                    find_instance_id: "config-a-b494b6",
                    find_instance_namespace: "config-a-namespace",
                    find_instance_result: "",
                    find_instance_result_error: true,
                }),
            },
        );

        pod_watcher
            .handle_pod(Event::Applied(pod), &mock, &mut false)
            .await
            .unwrap();
        trace!(
            "test_handle_pod_added_unready pod_watcher:{:?}",
            &pod_watcher
        );
        assert_eq!(1, pod_watcher.known_pods.len());
        assert_eq!(
            &PodState::Ended,
            pod_watcher
                .known_pods
                .get(&"config-a-b494b6-pod".to_string())
                .unwrap()
        )
    }

    #[tokio::test]
    async fn test_handle_pod_add_or_modify_unknown_phase() {
        let _ = env_logger::builder().is_test(true).try_init();

        let phase = "Foo";
        {
            let pod_list = create_pods_with_phase(
                "../test/json/running-pod-list-for-config-a-local.json",
                phase,
            );
            let pod = pod_list.items.first().unwrap().clone();
            let mut pod_watcher = BrokerPodWatcher::new();
            trace!(
                "test_handle_pod_added_unready phase:{}, Event::Applied",
                &phase
            );
            pod_watcher
                .handle_pod(Event::Applied(pod), &MockKubeInterface::new(), &mut false)
                .await
                .unwrap();
            trace!(
                "test_handle_pod_added_unready pod_watcher:{:?}",
                &pod_watcher
            );
            assert_eq!(0, pod_watcher.known_pods.len());
        }
        {
            let pod_list = create_pods_with_phase(
                "../test/json/running-pod-list-for-config-a-local.json",
                phase,
            );
            let pod = pod_list.items.first().unwrap().clone();
            let mut pod_watcher = BrokerPodWatcher::new();
            trace!(
                "test_handle_pod_added_unready phase:{}, Event::Applied",
                &phase
            );
            pod_watcher
                .handle_pod(Event::Applied(pod), &MockKubeInterface::new(), &mut false)
                .await
                .unwrap();
            trace!(
                "test_handle_pod_added_unready pod_watcher:{:?}",
                &pod_watcher
            );
            assert_eq!(0, pod_watcher.known_pods.len());
        }
    }

    #[tokio::test]
    async fn test_handle_running_pod_if_needed_do_nothing() {
        let _ = env_logger::builder().is_test(true).try_init();

        let pods_json =
            file::read_file_to_string("../test/json/running-pod-list-for-config-a-local.json");
        let pod_list: PodList = serde_json::from_str(&pods_json).unwrap();
        let pod = pod_list.items.first().unwrap();

        let mut pod_watcher = BrokerPodWatcher::new();
        pod_watcher
            .known_pods
            .insert("config-a-b494b6-pod".to_string(), PodState::Running);
        pod_watcher
            .handle_running_pod_if_needed(pod, &MockKubeInterface::new())
            .await
            .unwrap();
        assert_eq!(1, pod_watcher.known_pods.len());
        assert_eq!(
            &PodState::Running,
            pod_watcher
                .known_pods
                .get(&"config-a-b494b6-pod".to_string())
                .unwrap()
        )
    }

    #[tokio::test]
    async fn test_handle_ended_pod_if_needed_do_nothing() {
        let _ = env_logger::builder().is_test(true).try_init();

        let pods_json =
            file::read_file_to_string("../test/json/running-pod-list-for-config-a-local.json");
        let pod_list: PodList = serde_json::from_str(&pods_json).unwrap();
        let pod = pod_list.items.first().unwrap();

        let mut pod_watcher = BrokerPodWatcher::new();
        pod_watcher
            .known_pods
            .insert("config-a-b494b6-pod".to_string(), PodState::Ended);
        pod_watcher
            .handle_ended_pod_if_needed(pod, &MockKubeInterface::new())
            .await
            .unwrap();
        assert_eq!(1, pod_watcher.known_pods.len());
        assert_eq!(
            &PodState::Ended,
            pod_watcher
                .known_pods
                .get(&"config-a-b494b6-pod".to_string())
                .unwrap()
        )
    }

    #[tokio::test]
    async fn test_handle_deleted_pod_if_needed_do_nothing() {
        let _ = env_logger::builder().is_test(true).try_init();

        let pods_json =
            file::read_file_to_string("../test/json/running-pod-list-for-config-a-local.json");
        let pod_list: PodList = serde_json::from_str(&pods_json).unwrap();
        let pod = pod_list.items.first().unwrap();

        let mut pod_watcher = BrokerPodWatcher::new();
        pod_watcher
            .known_pods
            .insert("config-a-b494b6-pod".to_string(), PodState::Deleted);
        pod_watcher
            .handle_deleted_pod_if_needed(pod, &MockKubeInterface::new())
            .await
            .unwrap();
        assert_eq!(1, pod_watcher.known_pods.len());
        assert_eq!(
            &PodState::Deleted,
            pod_watcher
                .known_pods
                .get(&"config-a-b494b6-pod".to_string())
                .unwrap()
        )
    }

    #[test]
    fn test_get_pod_phase() {
        let _ = env_logger::builder().is_test(true).try_init();

        for phase in &[
            "Unknown",
            "Pending",
            "Running",
            "Ended",
            "Failed",
            "Succeeded",
            "Foo",
            "",
        ] {
            let pod_list = create_pods_with_phase(
                "../test/json/running-pod-list-for-config-a-local.json",
                phase,
            );
            let pod = pod_list.items.first().unwrap().clone();
            let mut pod_watcher = BrokerPodWatcher::new();

            assert_eq!(phase.to_string(), pod_watcher.get_pod_phase(&pod));
        }

        {
            let pod_list = create_pods_with_phase(
                "../test/json/running-pod-list-for-config-a-local.json",
                "Foo",
            );
            let mut pod = pod_list.items.first().unwrap().clone();
            pod.status = None;

            let mut pod_watcher = BrokerPodWatcher::new();

            assert_eq!("Unknown", pod_watcher.get_pod_phase(&pod));
        }
    }

    #[tokio::test]
    async fn test_cleanup_svc_if_unsupported() {
        let _ = env_logger::builder().is_test(true).try_init();
        let watcher = BrokerPodWatcher::new();
        let pod_list = create_pods_with_phase(
            "../test/json/running-pod-list-for-config-a-local.json",
            "Succeeded",
        );
        let pod = pod_list.items.first().unwrap().to_owned();
        let svc_name = "some-service";
        let svc_namespace = "default";
        let mut mock = MockKubeInterface::new();
        mock.expect_remove_service()
            .times(1)
            .returning(|_, _| Ok(()));
        watcher
            .cleanup_svc_if_unsupported(&[pod], svc_name, svc_namespace, &mock)
            .await
            .unwrap();
    }

    #[test]
    fn test_get_instance_and_configuration_from_pod() {
        let _ = env_logger::builder().is_test(true).try_init();

        let pod_list = create_pods_with_phase(
            "../test/json/running-pod-list-for-config-a-local.json",
            "Foo",
        );
        let orig_pod = pod_list.items.first().unwrap();

        let pod_watcher = BrokerPodWatcher::new();
        assert!(pod_watcher
            .get_instance_and_configuration_from_pod(orig_pod)
            .is_ok());

        let mut instanceless_pod = orig_pod.clone();
        instanceless_pod
            .metadata
            .labels
            .as_mut()
            .unwrap()
            .remove(AKRI_INSTANCE_LABEL_NAME);
        assert!(pod_watcher
            .get_instance_and_configuration_from_pod(&instanceless_pod)
            .is_err());

        let mut configurationless_pod = orig_pod.clone();
        configurationless_pod
            .metadata
            .labels
            .as_mut()
            .unwrap()
            .remove(AKRI_CONFIGURATION_LABEL_NAME);
        assert!(pod_watcher
            .get_instance_and_configuration_from_pod(&configurationless_pod)
            .is_err());
    }

    #[tokio::test]
    async fn test_create_or_update_service_successful_update() {
        let _ = env_logger::builder().is_test(true).try_init();

        let config_json = file::read_file_to_string("../test/json/config-a.json");
        let config: Configuration = serde_json::from_str(&config_json).unwrap();

        let pod_watcher = BrokerPodWatcher::new();
        let mut mock = MockKubeInterface::new();
        config_for_tests::configure_find_services(
            &mut mock,
            "akri.sh/instance=config-a-b494b6",
            "../test/json/running-instance-svc-list-for-config-a-local.json",
            false,
        );
        config_for_tests::configure_update_service(
            &mut mock,
            "node-a-config-a-b494b6-svc",
            "config-a-namespace",
            false,
        );
        let ownership = OwnershipInfo::new(
            OwnershipType::Instance,
            "object".to_string(),
            "object_uid".to_string(),
        );
        pod_watcher
            .create_or_update_service(
                "config-a-b494b6",
                "config-a",
                "config-a-namespace",
                AKRI_INSTANCE_LABEL_NAME,
                "config-a-b494b6",
                ownership,
                &config.spec.instance_service_spec.unwrap().clone(),
                true,
                &mock,
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_create_or_update_service_failed_update() {
        let _ = env_logger::builder().is_test(true).try_init();

        let config_json = file::read_file_to_string("../test/json/config-a.json");
        let config: Configuration = serde_json::from_str(&config_json).unwrap();

        let pod_watcher = BrokerPodWatcher::new();
        let mut mock = MockKubeInterface::new();
        config_for_tests::configure_find_services(
            &mut mock,
            "akri.sh/instance=config-a-b494b6",
            "../test/json/running-instance-svc-list-for-config-a-local.json",
            false,
        );
        config_for_tests::configure_update_service(
            &mut mock,
            "node-a-config-a-b494b6-svc",
            "config-a-namespace",
            true,
        );
        let ownership = OwnershipInfo::new(
            OwnershipType::Instance,
            "object".to_string(),
            "object_uid".to_string(),
        );

        assert!(pod_watcher
            .create_or_update_service(
                "config-a-b494b6",
                "config-a",
                "config-a-namespace",
                AKRI_INSTANCE_LABEL_NAME,
                "config-a-b494b6",
                ownership,
                &config.spec.instance_service_spec.unwrap().clone(),
                true,
                &mock
            )
            .await
            .is_err());
    }

    #[tokio::test]
    async fn test_create_or_update_service_successful_create() {
        let _ = env_logger::builder().is_test(true).try_init();

        let config_json = file::read_file_to_string("../test/json/config-a.json");
        let config: Configuration = serde_json::from_str(&config_json).unwrap();

        let pod_watcher = BrokerPodWatcher::new();
        let mut mock = MockKubeInterface::new();
        config_for_tests::configure_find_services(
            &mut mock,
            "akri.sh/instance=config-a-b494b6",
            "../test/json/empty-list.json",
            false,
        );
        config_for_tests::configure_add_service(
            &mut mock,
            "config-a-b494b6-svc",
            "config-a-namespace",
            AKRI_INSTANCE_LABEL_NAME,
            "config-a-b494b6",
        );
        let ownership = OwnershipInfo::new(
            OwnershipType::Instance,
            "object".to_string(),
            "object_uid".to_string(),
        );

        pod_watcher
            .create_or_update_service(
                "config-a-b494b6",
                "config-a",
                "config-a-namespace",
                AKRI_INSTANCE_LABEL_NAME,
                "config-a-b494b6",
                ownership,
                &config.spec.instance_service_spec.unwrap().clone(),
                true,
                &mock,
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_create_or_update_service_failed_create() {
        let _ = env_logger::builder().is_test(true).try_init();

        let config_json = file::read_file_to_string("../test/json/config-a.json");
        let config: Configuration = serde_json::from_str(&config_json).unwrap();

        let pod_watcher = BrokerPodWatcher::new();
        let mut mock = MockKubeInterface::new();
        config_for_tests::configure_find_services(
            &mut mock,
            "akri.sh/instance=config-a-b494b6",
            "../test/json/empty-list.json",
            false,
        );
        mock.expect_create_service()
            .returning(move |_, _| Err(anyhow::anyhow!("Failure")));

        let ownership = OwnershipInfo::new(
            OwnershipType::Instance,
            "object".to_string(),
            "object_uid".to_string(),
        );

        assert!(pod_watcher
            .create_or_update_service(
                "config-a-b494b6",
                "config-a",
                "config-a-namespace",
                AKRI_INSTANCE_LABEL_NAME,
                "config-a-b494b6",
                ownership,
                &config.spec.instance_service_spec.unwrap().clone(),
                true,
                &mock
            )
            .await
            .is_err());
    }

    #[derive(Clone)]
    struct RemoveService {
        remove_service_name: &'static str,
        remove_service_namespace: &'static str,
    }

    #[derive(Clone)]
    struct CleanupService {
        find_pod_selector: &'static str,
        find_pod_result: &'static str,
        remove_service: Option<RemoveService>,
    }

    #[derive(Clone)]
    struct CleanupServices {
        // This field is used for testing
        #[allow(dead_code)]
        find_svc_selector: &'static str,
        // This field is used for testing
        #[allow(dead_code)]
        find_svc_result: &'static str,
        cleanup_services: Vec<CleanupService>,
        find_instance_id: &'static str,
        find_instance_namespace: &'static str,
        find_instance_result: &'static str,
        find_instance_result_error: bool,
    }

    fn configure_for_cleanup_broker_and_configuration_svcs(
        mock: &mut MockKubeInterface,
        work: &CleanupServices,
    ) {
        for i in 0..work.cleanup_services.len() {
            let cleanup_service = &work.cleanup_services[i];
            config_for_tests::configure_find_pods(
                mock,
                cleanup_service.find_pod_selector,
                cleanup_service.find_pod_result,
                false,
            );
            if let Some(remove_service) = &cleanup_service.remove_service {
                config_for_tests::configure_remove_service(
                    mock,
                    remove_service.remove_service_name,
                    remove_service.remove_service_namespace,
                );
            }
        }

        config_for_tests::configure_find_instance(
            mock,
            work.find_instance_id,
            work.find_instance_namespace,
            work.find_instance_result,
            work.find_instance_result_error,
        );
    }

    #[derive(Clone)]
    struct FindServices {
        find_services_selector: &'static str,
        find_services_result: &'static str,
        find_services_error: bool,
    }

    #[derive(Clone)]
    struct HandlePodRunning {
        find_config_name: &'static str,
        find_config_namespace: &'static str,
        find_config_result: &'static str,
        find_config_error: bool,

        find_instance_name: &'static str,
        find_instance_result: &'static str,

        find_instance_service: FindServices,
        new_instance_svc_name: &'static str,

        find_configuration_service: FindServices,
        new_configuration_svc_name: &'static str,
    }

    fn configure_for_running_pod_work(mock: &mut MockKubeInterface, work: &HandlePodRunning) {
        config_for_tests::configure_find_config(
            mock,
            work.find_config_name,
            work.find_config_namespace,
            work.find_config_result,
            work.find_config_error,
        );
        if !work.find_config_error {
            config_for_tests::configure_find_instance(
                mock,
                work.find_instance_name,
                work.find_config_namespace,
                work.find_instance_result,
                false,
            );

            config_for_tests::configure_find_services(
                mock,
                work.find_instance_service.find_services_selector,
                work.find_instance_service.find_services_result,
                work.find_instance_service.find_services_error,
            );
            if work.find_instance_service.find_services_error {
                config_for_tests::configure_update_service(
                    mock,
                    work.new_instance_svc_name,
                    work.find_config_namespace,
                    false,
                );
            } else {
                config_for_tests::configure_add_service(
                    mock,
                    work.new_instance_svc_name,
                    work.find_config_namespace,
                    AKRI_INSTANCE_LABEL_NAME,
                    work.find_instance_name,
                );
            }

            config_for_tests::configure_find_services(
                mock,
                work.find_configuration_service.find_services_selector,
                work.find_configuration_service.find_services_result,
                work.find_configuration_service.find_services_error,
            );
            if work.find_configuration_service.find_services_error {
                config_for_tests::configure_update_service(
                    mock,
                    work.new_configuration_svc_name,
                    work.find_config_namespace,
                    false,
                );
            } else {
                config_for_tests::configure_add_service(
                    mock,
                    work.new_configuration_svc_name,
                    work.find_config_namespace,
                    AKRI_CONFIGURATION_LABEL_NAME,
                    work.find_config_name,
                );
            }
        }
    }

    #[derive(Clone)]
    struct HandlePod {
        running: Option<HandlePodRunning>,
        ended: Option<CleanupServices>,
    }

    fn configure_for_handle_pod(mock: &mut MockKubeInterface, handle_pod: &HandlePod) {
        if let Some(running) = &handle_pod.running {
            configure_for_running_pod_work(mock, running);
        }

        if let Some(ended) = &handle_pod.ended {
            configure_for_cleanup_broker_and_configuration_svcs(mock, ended);
        }
    }
}
