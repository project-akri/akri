use super::akri::{
    configuration,
    configuration::{Configuration, ConfigurationList},
    instance,
    instance::{Instance, InstanceList, InstanceSpec},
    retry::{random_delay, MAX_INSTANCE_UPDATE_TRIES},
    API_NAMESPACE, API_VERSION,
};
use async_trait::async_trait;
use k8s_openapi::api::batch::v1::Job;
use k8s_openapi::api::core::v1::{Node, Pod, Service};
use kube::{api::ObjectList, client::Client};
use mockall::{automock, predicate::*};

pub mod job;
pub mod node;
pub mod pod;
pub mod service;

pub const NODE_SELECTOR_OP_IN: &str = "In";
pub const OBJECT_NAME_FIELD: &str = "metadata.name";
pub const RESOURCE_REQUIREMENTS_KEY: &str = "{{PLACEHOLDER}}";
pub const ERROR_NOT_FOUND: u16 = 404;
pub const ERROR_CONFLICT: u16 = 409;

/// OwnershipType defines what type of Kubernetes object
/// an object is dependent on
#[derive(Clone, Debug)]
pub enum OwnershipType {
    Configuration,
    Instance,
    Pod,
    Service,
}

/// OwnershipInfo provides enough information to identify
/// the Kubernetes object an object depends on
#[derive(Clone, Debug)]
pub struct OwnershipInfo {
    object_type: OwnershipType,
    object_uid: String,
    object_name: String,
}

impl OwnershipInfo {
    pub fn new(object_type: OwnershipType, object_name: String, object_uid: String) -> Self {
        OwnershipInfo {
            object_type,
            object_uid,
            object_name,
        }
    }

    pub fn get_api_version(&self) -> String {
        match self.object_type {
            OwnershipType::Instance | OwnershipType::Configuration => {
                format!("{}/{}", API_NAMESPACE, API_VERSION)
            }
            OwnershipType::Pod | OwnershipType::Service => "core/v1".to_string(),
        }
    }

    pub fn get_kind(&self) -> String {
        match self.object_type {
            OwnershipType::Instance => "Instance",
            OwnershipType::Configuration => "Configuration",
            OwnershipType::Pod => "Pod",
            OwnershipType::Service => "Service",
        }
        .to_string()
    }

    pub fn get_controller(&self) -> Option<bool> {
        Some(true)
    }

    pub fn get_block_owner_deletion(&self) -> Option<bool> {
        Some(true)
    }

    pub fn get_name(&self) -> String {
        self.object_name.clone()
    }

    pub fn get_uid(&self) -> String {
        self.object_uid.clone()
    }
}

#[automock]
#[async_trait]
pub trait KubeInterface: Send + Sync {
    fn get_kube_client(&self) -> Client;

    async fn find_node(&self, name: &str) -> Result<Node, anyhow::Error>;

    async fn find_pods_with_label(&self, selector: &str) -> Result<ObjectList<Pod>, anyhow::Error>;
    async fn find_pods_with_field(&self, selector: &str) -> Result<ObjectList<Pod>, anyhow::Error>;
    async fn create_pod(&self, pod_to_create: &Pod, namespace: &str) -> Result<(), anyhow::Error>;
    async fn remove_pod(&self, pod_to_remove: &str, namespace: &str) -> Result<(), anyhow::Error>;

    async fn find_jobs_with_label(&self, selector: &str) -> Result<ObjectList<Job>, anyhow::Error>;
    async fn find_jobs_with_field(&self, selector: &str) -> Result<ObjectList<Job>, anyhow::Error>;
    async fn create_job(&self, job_to_create: &Job, namespace: &str) -> Result<(), anyhow::Error>;
    async fn remove_job(&self, job_to_remove: &str, namespace: &str) -> Result<(), anyhow::Error>;

    async fn find_services(&self, selector: &str) -> Result<ObjectList<Service>, anyhow::Error>;
    async fn create_service(
        &self,
        svc_to_create: &Service,
        namespace: &str,
    ) -> Result<(), anyhow::Error>;
    async fn remove_service(
        &self,
        svc_to_remove: &str,
        namespace: &str,
    ) -> Result<(), anyhow::Error>;
    async fn update_service(
        &self,
        svc_to_update: &Service,
        name: &str,
        namespace: &str,
    ) -> Result<(), anyhow::Error>;

    async fn find_configuration(
        &self,
        name: &str,
        namespace: &str,
    ) -> Result<Configuration, anyhow::Error>;
    async fn get_configurations(&self) -> Result<ConfigurationList, anyhow::Error>;

    async fn find_instance(&self, name: &str, namespace: &str) -> Result<Instance, anyhow::Error>;
    async fn get_instances(&self) -> Result<InstanceList, anyhow::Error>;
    async fn create_instance(
        &self,
        instance_to_create: &InstanceSpec,
        name: &str,
        namespace: &str,
        owner_config_name: &str,
        owner_config_uid: &str,
    ) -> Result<(), anyhow::Error>;
    async fn delete_instance(&self, name: &str, namespace: &str) -> Result<(), anyhow::Error>;
    async fn update_instance(
        &self,
        instance_to_update: &InstanceSpec,
        name: &str,
        namespace: &str,
    ) -> Result<(), anyhow::Error>;
}

#[derive(Clone)]
pub struct KubeImpl {
    client: kube::Client,
}

impl KubeImpl {
    /// Create new instance of KubeImpl
    pub async fn new() -> Result<Self, anyhow::Error> {
        Ok(KubeImpl {
            client: Client::try_default().await?,
        })
    }
}

#[async_trait]
impl KubeInterface for KubeImpl {
    /// Return of clone of KubeImpl's client
    fn get_kube_client(&self) -> Client {
        self.client.clone()
    }

    /// Get Kuberenetes node for specified name
    ///
    /// Example:
    ///
    /// ```no_run
    /// use akri_shared::k8s;
    /// use akri_shared::k8s::KubeInterface;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let kube = k8s::KubeImpl::new().await.unwrap();
    /// let node = kube.find_node("node-a").await.unwrap();
    /// # }
    /// ```
    async fn find_node(&self, name: &str) -> Result<Node, anyhow::Error> {
        node::find_node(name, self.get_kube_client()).await
    }

    /// Get Kuberenetes pods with specified label selector
    ///
    /// Example:
    ///
    /// ```no_run
    /// use akri_shared::k8s;
    /// use akri_shared::k8s::KubeInterface;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let kube = k8s::KubeImpl::new().await.unwrap();
    /// let interesting_pods = kube.find_pods_with_label("label=interesting").await.unwrap();
    /// # }
    /// ```
    async fn find_pods_with_label(&self, selector: &str) -> Result<ObjectList<Pod>, anyhow::Error> {
        pod::find_pods_with_selector(Some(selector.to_string()), None, self.get_kube_client()).await
    }
    /// Get Kuberenetes pods with specified field selector
    ///
    /// Example:
    ///
    /// ```no_run
    /// use akri_shared::k8s;
    /// use akri_shared::k8s::KubeInterface;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let kube = k8s::KubeImpl::new().await.unwrap();
    /// let pods_on_node_a = kube.find_pods_with_field("spec.nodeName=node-a").await.unwrap();
    /// # }
    /// ```
    async fn find_pods_with_field(&self, selector: &str) -> Result<ObjectList<Pod>, anyhow::Error> {
        pod::find_pods_with_selector(None, Some(selector.to_string()), self.get_kube_client()).await
    }
    /// Create Kuberenetes pod
    ///
    /// Example:
    ///
    /// ```no_run
    /// use akri_shared::k8s;
    /// use akri_shared::k8s::KubeInterface;
    /// use k8s_openapi::api::core::v1::Pod;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let kube = k8s::KubeImpl::new().await.unwrap();
    /// kube.create_pod(&Pod::default(), "pod_namespace").await.unwrap();
    /// # }
    /// ```
    async fn create_pod(&self, pod_to_create: &Pod, namespace: &str) -> Result<(), anyhow::Error> {
        pod::create_pod(pod_to_create, namespace, self.get_kube_client()).await
    }
    /// Remove Kubernetes pod
    ///
    /// Example:
    ///
    /// ```no_run
    /// use akri_shared::k8s;
    /// use akri_shared::k8s::KubeInterface;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let kube = k8s::KubeImpl::new().await.unwrap();
    /// kube.remove_pod("pod_to_remove", "pod_namespace").await.unwrap();
    /// # }
    /// ```
    async fn remove_pod(&self, pod_to_remove: &str, namespace: &str) -> Result<(), anyhow::Error> {
        pod::remove_pod(pod_to_remove, namespace, self.get_kube_client()).await
    }

    /// Find Kuberenetes Jobs with specified label selector
    ///
    /// Example:
    ///
    /// ```no_run
    /// use akri_shared::k8s;
    /// use akri_shared::k8s::KubeInterface;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let kube = k8s::KubeImpl::new().await.unwrap();
    /// let interesting_jobs = kube.find_jobs_with_label("label=interesting").await.unwrap();
    /// # }
    /// ```
    async fn find_jobs_with_label(&self, selector: &str) -> Result<ObjectList<Job>, anyhow::Error> {
        job::find_jobs_with_selector(Some(selector.to_string()), None, self.get_kube_client()).await
    }
    /// Find Kuberenetes Jobs with specified field selector
    ///
    /// Example:
    ///
    /// ```no_run
    /// use akri_shared::k8s;
    /// use akri_shared::k8s::KubeInterface;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let kube = k8s::KubeImpl::new().await.unwrap();
    /// let jobs_on_node_a = kube.find_jobs_with_field("spec.nodeName=node-a").await.unwrap();
    /// # }
    /// ```
    async fn find_jobs_with_field(&self, selector: &str) -> Result<ObjectList<Job>, anyhow::Error> {
        job::find_jobs_with_selector(None, Some(selector.to_string()), self.get_kube_client()).await
    }

    /// Create Kuberenetes job
    ///
    /// Example:
    ///
    /// ```no_run
    /// use akri_shared::k8s;
    /// use akri_shared::k8s::KubeInterface;
    /// use k8s_openapi::api::batch::v1::Job;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let kube = k8s::KubeImpl::new().await.unwrap();
    /// kube.create_job(&Job::default(), "job_namespace").await.unwrap();
    /// # }
    /// ```
    async fn create_job(&self, job_to_create: &Job, namespace: &str) -> Result<(), anyhow::Error> {
        job::create_job(job_to_create, namespace, self.get_kube_client()).await
    }
    /// Remove Kubernetes job
    ///
    /// Example:
    ///
    /// ```no_run
    /// use akri_shared::k8s;
    /// use akri_shared::k8s::KubeInterface;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let kube = k8s::KubeImpl::new().await.unwrap();
    /// kube.remove_job("job_to_remove", "job_namespace").await.unwrap();
    /// # }
    /// ```
    async fn remove_job(&self, job_to_remove: &str, namespace: &str) -> Result<(), anyhow::Error> {
        job::remove_job(job_to_remove, namespace, self.get_kube_client()).await
    }

    /// Get Kuberenetes services with specified label selector
    ///
    /// Example:
    ///
    /// ```no_run
    /// use akri_shared::k8s;
    /// use akri_shared::k8s::KubeInterface;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let kube = k8s::KubeImpl::new().await.unwrap();
    /// let interesting_services = kube.find_services("label=interesting").await.unwrap();
    /// # }
    /// ```
    async fn find_services(&self, selector: &str) -> Result<ObjectList<Service>, anyhow::Error> {
        service::find_services_with_selector(selector, self.get_kube_client()).await
    }
    /// Create Kubernetes service
    ///
    /// Example:
    ///
    /// ```no_run
    /// use akri_shared::k8s;
    /// use akri_shared::k8s::KubeInterface;
    /// use k8s_openapi::api::core::v1::Service;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let kube = k8s::KubeImpl::new().await.unwrap();
    /// kube.create_service(&Service::default(), "service_namespace").await.unwrap();
    /// # }
    /// ```
    async fn create_service(
        &self,
        svc_to_create: &Service,
        namespace: &str,
    ) -> Result<(), anyhow::Error> {
        service::create_service(svc_to_create, namespace, self.get_kube_client()).await
    }
    /// Remove Kubernetes service
    ///
    /// Example:
    ///
    /// ```no_run
    /// use akri_shared::k8s;
    /// use akri_shared::k8s::KubeInterface;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let kube = k8s::KubeImpl::new().await.unwrap();
    /// kube.remove_service("service_to_remove", "service_namespace").await.unwrap();
    /// # }
    /// ```
    async fn remove_service(
        &self,
        svc_to_remove: &str,
        namespace: &str,
    ) -> Result<(), anyhow::Error> {
        service::remove_service(svc_to_remove, namespace, self.get_kube_client()).await
    }
    /// Update Kubernetes service
    ///
    /// Example:
    ///
    /// ```no_run
    /// use akri_shared::k8s;
    /// use akri_shared::k8s::KubeInterface;
    /// use k8s_openapi::api::core::v1::Service;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let kube = k8s::KubeImpl::new().await.unwrap();
    /// let selector = "environment=production,app=nginx";
    /// for svc in kube.find_services(&selector).await.unwrap() {
    ///     let svc_name = &svc.metadata.name.clone().unwrap();
    ///     let svc_namespace = &svc.metadata.namespace.as_ref().unwrap().clone();
    ///     let updated_svc = kube.update_service(
    ///         &svc,
    ///         &svc_name,
    ///         &svc_namespace).await.unwrap();
    /// }
    /// # }
    /// ```
    async fn update_service(
        &self,
        svc_to_update: &Service,
        name: &str,
        namespace: &str,
    ) -> Result<(), anyhow::Error> {
        service::update_service(svc_to_update, name, namespace, self.get_kube_client()).await
    }

    // Get Akri Configuration with given name and namespace
    ///
    /// Example:
    ///
    /// ```no_run
    /// use akri_shared::k8s;
    /// use akri_shared::k8s::KubeInterface;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let kube = k8s::KubeImpl::new().await.unwrap();
    /// let config = kube.find_configuration("config-1", "config-namespace").await.unwrap();
    /// # }
    /// ```
    async fn find_configuration(
        &self,
        name: &str,
        namespace: &str,
    ) -> Result<Configuration, anyhow::Error> {
        configuration::find_configuration(name, namespace, &self.get_kube_client()).await
    }
    // Get Akri Configurations with given namespace
    ///
    /// Example:
    ///
    /// ```no_run
    /// use akri_shared::k8s;
    /// use akri_shared::k8s::KubeInterface;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let kube = k8s::KubeImpl::new().await.unwrap();
    /// let configs = kube.get_configurations().await.unwrap();
    /// # }
    /// ```
    async fn get_configurations(&self) -> Result<ConfigurationList, anyhow::Error> {
        configuration::get_configurations(&self.get_kube_client()).await
    }

    // Get Akri Instance with given name and namespace
    ///
    /// Example:
    ///
    /// ```no_run
    /// use akri_shared::k8s;
    /// use akri_shared::k8s::KubeInterface;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let kube = k8s::KubeImpl::new().await.unwrap();
    /// let instance = kube.find_instance("instance-1", "instance-namespace").await.unwrap();
    /// # }
    /// ```
    async fn find_instance(&self, name: &str, namespace: &str) -> Result<Instance, anyhow::Error> {
        instance::find_instance(name, namespace, &self.get_kube_client()).await
    }
    // Get Akri Instances with given namespace
    ///
    /// Example:
    ///
    /// ```no_run
    /// use akri_shared::k8s;
    /// use akri_shared::k8s::KubeInterface;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let kube = k8s::KubeImpl::new().await.unwrap();
    /// let instances = kube.get_instances().await.unwrap();
    /// # }
    /// ```
    async fn get_instances(&self) -> Result<InstanceList, anyhow::Error> {
        instance::get_instances(&self.get_kube_client()).await
    }

    /// Create Akri Instance
    ///
    /// Example:
    ///
    /// ```no_run
    /// use akri_shared::k8s;
    /// use akri_shared::k8s::KubeInterface;
    /// use akri_shared::akri::instance::InstanceSpec;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let kube = k8s::KubeImpl::new().await.unwrap();
    /// kube.create_instance(
    ///     &InstanceSpec{
    ///         configuration_name: "capability_configuration_name".to_string(),
    ///         shared: true,
    ///         nodes: Vec::new(),
    ///         device_usage: std::collections::HashMap::new(),
    ///         broker_properties: std::collections::HashMap::new(),
    ///     },
    ///     "instance-1",
    ///     "instance-namespace",
    ///     "config-1",
    ///     "abcdefgh-ijkl-mnop-qrst-uvwxyz012345"
    /// ).await.unwrap();
    /// # }
    /// ```
    async fn create_instance(
        &self,
        instance_to_create: &InstanceSpec,
        name: &str,
        namespace: &str,
        owner_config_name: &str,
        owner_config_uid: &str,
    ) -> Result<(), anyhow::Error> {
        instance::create_instance(
            instance_to_create,
            name,
            namespace,
            owner_config_name,
            owner_config_uid,
            &self.get_kube_client(),
        )
        .await
    }
    // Delete Akri Instance
    ///
    /// Example:
    ///
    /// ```no_run
    /// use akri_shared::k8s;
    /// use akri_shared::k8s::KubeInterface;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let kube = k8s::KubeImpl::new().await.unwrap();
    /// kube.delete_instance(
    ///     "instance-1",
    ///     "instance-namespace"
    /// ).await.unwrap();
    /// # }
    /// ```
    async fn delete_instance(&self, name: &str, namespace: &str) -> Result<(), anyhow::Error> {
        instance::delete_instance(name, namespace, &self.get_kube_client()).await
    }
    /// Update Akri Instance
    ///
    /// Example:
    ///
    /// ```no_run
    /// use akri_shared::k8s;
    /// use akri_shared::k8s::KubeInterface;
    /// use akri_shared::akri::instance::InstanceSpec;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let kube = k8s::KubeImpl::new().await.unwrap();
    /// kube.update_instance(
    ///     &InstanceSpec{
    ///         configuration_name: "capability_configuration_name".to_string(),
    ///         shared: true,
    ///         nodes: Vec::new(),
    ///         device_usage: std::collections::HashMap::new(),
    ///         broker_properties: std::collections::HashMap::new(),
    ///     },
    ///     "instance-1",
    ///     "instance-namespace"
    /// ).await.unwrap();
    /// # }
    /// ```
    async fn update_instance(
        &self,
        instance_to_update: &InstanceSpec,
        name: &str,
        namespace: &str,
    ) -> Result<(), anyhow::Error> {
        instance::update_instance(instance_to_update, name, namespace, &self.get_kube_client())
            .await
    }
}

/// This deletes an Instance unless it has already been deleted by another node
/// or fails after multiple retries.
pub async fn try_delete_instance(
    kube_interface: &dyn KubeInterface,
    instance_name: &str,
    instance_namespace: &str,
) -> Result<(), anyhow::Error> {
    for x in 0..MAX_INSTANCE_UPDATE_TRIES {
        match kube_interface
            .delete_instance(instance_name, instance_namespace)
            .await
        {
            Ok(()) => {
                log::trace!("try_delete_instance - deleted Instance {}", instance_name);
                break;
            }
            Err(e) => {
                if let Some(ae) = e.downcast_ref::<kube::error::ErrorResponse>() {
                    if ae.code == ERROR_NOT_FOUND {
                        log::trace!(
                            "try_delete_instance - discovered Instance {} already deleted",
                            instance_name
                        );
                        break;
                    }
                }
                log::error!(
                    "try_delete_instance - tried to delete Instance {} but still exists. {} retries left.",
                    instance_name, MAX_INSTANCE_UPDATE_TRIES - x - 1
                );
                if x == MAX_INSTANCE_UPDATE_TRIES - 1 {
                    return Err(e);
                }
            }
        }
        random_delay().await;
    }
    Ok(())
}

#[cfg(test)]
pub mod test_ownership {
    use super::*;

    #[tokio::test]
    async fn test_try_delete_instance() {
        let mut mock_kube_interface = MockKubeInterface::new();
        mock_kube_interface
            .expect_delete_instance()
            .times(1)
            .returning(move |_, _| {
                let error_response = kube::error::ErrorResponse {
                    status: "random".to_string(),
                    message: "blah".to_string(),
                    reason: "NotFound".to_string(),
                    code: 404,
                };
                Err(error_response.into())
            });
        try_delete_instance(&mock_kube_interface, "instance_name", "instance_namespace")
            .await
            .unwrap();
    }

    // Test that succeeds on second try
    #[tokio::test]
    async fn test_try_delete_instance_sequence() {
        let mut seq = mockall::Sequence::new();
        let mut mock_kube_interface = MockKubeInterface::new();
        mock_kube_interface
            .expect_delete_instance()
            .times(1)
            .returning(move |_, _| {
                let error_response = kube::error::ErrorResponse {
                    status: "random".to_string(),
                    message: "blah".to_string(),
                    reason: "SomeError".to_string(),
                    code: 401,
                };
                Err(error_response.into())
            })
            .in_sequence(&mut seq);
        mock_kube_interface
            .expect_delete_instance()
            .times(1)
            .returning(move |_, _| Ok(()))
            .in_sequence(&mut seq);
        try_delete_instance(&mock_kube_interface, "instance_name", "instance_namespace")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_ownership_from_config() {
        let name = "asdf";
        let uid = "zxcv";
        let ownership = OwnershipInfo::new(
            OwnershipType::Configuration,
            name.to_string(),
            uid.to_string(),
        );
        assert_eq!(
            format!("{}/{}", API_NAMESPACE, API_VERSION),
            ownership.get_api_version()
        );
        assert_eq!("Configuration", &ownership.get_kind());
        assert!(ownership.get_controller().unwrap());
        assert!(ownership.get_block_owner_deletion().unwrap());
        assert_eq!(name, &ownership.get_name());
        assert_eq!(uid, &ownership.get_uid());
    }
    #[tokio::test]
    async fn test_ownership_from_instance() {
        let name = "asdf";
        let uid = "zxcv";
        let ownership =
            OwnershipInfo::new(OwnershipType::Instance, name.to_string(), uid.to_string());
        assert_eq!(
            format!("{}/{}", API_NAMESPACE, API_VERSION),
            ownership.get_api_version()
        );
        assert_eq!("Instance", &ownership.get_kind());
        assert!(ownership.get_controller().unwrap());
        assert!(ownership.get_block_owner_deletion().unwrap());
        assert_eq!(name, &ownership.get_name());
        assert_eq!(uid, &ownership.get_uid());
    }
    #[tokio::test]
    async fn test_ownership_from_pod() {
        let name = "asdf";
        let uid = "zxcv";
        let ownership = OwnershipInfo::new(OwnershipType::Pod, name.to_string(), uid.to_string());
        assert_eq!("core/v1", ownership.get_api_version());
        assert_eq!("Pod", &ownership.get_kind());
        assert!(ownership.get_controller().unwrap());
        assert!(ownership.get_block_owner_deletion().unwrap());
        assert_eq!(name, &ownership.get_name());
        assert_eq!(uid, &ownership.get_uid());
    }
    #[tokio::test]
    async fn test_ownership_from_service() {
        let name = "asdf";
        let uid = "zxcv";
        let ownership =
            OwnershipInfo::new(OwnershipType::Service, name.to_string(), uid.to_string());
        assert_eq!("core/v1", ownership.get_api_version());
        assert_eq!("Service", &ownership.get_kind());
        assert!(ownership.get_controller().unwrap());
        assert!(ownership.get_block_owner_deletion().unwrap());
        assert_eq!(name, &ownership.get_name());
        assert_eq!(uid, &ownership.get_uid());
    }
}
