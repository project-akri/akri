use super::akri::{
    configuration,
    configuration::{KubeAkriConfig, KubeAkriConfigList},
    instance,
    instance::{Instance, KubeAkriInstance, KubeAkriInstanceList},
    API_NAMESPACE, API_VERSION,
};
use async_trait::async_trait;
use futures::executor::block_on;
use k8s_openapi::api::core::v1::{
    NodeSpec, NodeStatus, Pod, PodSpec, PodStatus, Service, ServiceSpec, ServiceStatus,
};
use kube::{
    api::{Object, ObjectList},
    client::APIClient,
    config,
};

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

    pub fn get_controller(&self) -> bool {
        true
    }

    pub fn get_block_owner_deletion(&self) -> bool {
        true
    }

    pub fn get_name(&self) -> String {
        self.object_name.clone()
    }

    pub fn get_uid(&self) -> String {
        self.object_uid.clone()
    }
}

//
// mockall and async_trait do not work effortlessly together ... to enable both,
// follow the example here:
//     https://github.com/mibes/mockall-async/blob/53aec15219a720ef5ac483959ff8821cb7d656ae/src/main.rs
//
// When async traits are supported by Rust without the async_trait crate, we should
// add:
//    #[automock]
//

/// KubeInterface can query and modify Kubernetes.
///
/// An implementation of KubeInterface can query and modify Kubernetes Nodes, Pods,
/// and Services, as well as modifying Akri CRDs like Configurations
/// and Instances.
#[async_trait]
pub trait KubeInterface: Send + Sync {
    fn get_kube_client(&self) -> APIClient;

    async fn find_node(
        &self,
        name: &str,
    ) -> Result<Object<NodeSpec, NodeStatus>, Box<dyn std::error::Error + Send + Sync + 'static>>;

    async fn find_pods_with_label(
        &self,
        selector: &str,
    ) -> Result<
        ObjectList<Object<PodSpec, PodStatus>>,
        Box<dyn std::error::Error + Send + Sync + 'static>,
    >;
    async fn find_pods_with_field(
        &self,
        selector: &str,
    ) -> Result<
        ObjectList<Object<PodSpec, PodStatus>>,
        Box<dyn std::error::Error + Send + Sync + 'static>,
    >;
    async fn create_pod(
        &self,
        pod_to_create: &Pod,
        namespace: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>>;
    async fn remove_pod(
        &self,
        pod_to_remove: &str,
        namespace: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>>;

    async fn find_services(
        &self,
        selector: &str,
    ) -> Result<
        ObjectList<Object<ServiceSpec, ServiceStatus>>,
        Box<dyn std::error::Error + Send + Sync + 'static>,
    >;
    async fn create_service(
        &self,
        svc_to_create: &Service,
        namespace: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>>;
    async fn remove_service(
        &self,
        svc_to_remove: &str,
        namespace: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>>;
    async fn update_service(
        &self,
        svc_to_update: &Object<ServiceSpec, ServiceStatus>,
        name: &str,
        namespace: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>>;

    async fn find_configuration(
        &self,
        name: &str,
        namespace: &str,
    ) -> Result<KubeAkriConfig, Box<dyn std::error::Error + Send + Sync + 'static>>;
    async fn get_configurations(
        &self,
    ) -> Result<KubeAkriConfigList, Box<dyn std::error::Error + Send + Sync + 'static>>;

    async fn find_instance(
        &self,
        name: &str,
        namespace: &str,
    ) -> Result<KubeAkriInstance, Box<dyn std::error::Error + Send + Sync + 'static>>;
    async fn get_instances(
        &self,
    ) -> Result<KubeAkriInstanceList, Box<dyn std::error::Error + Send + Sync + 'static>>;
    async fn create_instance(
        &self,
        instance_to_create: &Instance,
        name: &str,
        namespace: &str,
        owner_config_name: &str,
        owner_config_uid: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>>;
    async fn delete_instance(
        &self,
        name: &str,
        namespace: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>>;
    async fn update_instance(
        &self,
        instance_to_update: &Instance,
        name: &str,
        namespace: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>>;
}

/// Create new KubeInetrace implementation
pub fn create_kube_interface() -> impl KubeInterface {
    KubeImpl::new()
}

#[derive(Clone)]
struct KubeImpl {
    kube_configuration: kube::config::Configuration,
}

impl KubeImpl {
    /// Create new instance of KubeImpl
    fn new() -> Self {
        KubeImpl {
            kube_configuration: match std::env::var("KUBERNETES_PORT") {
                Ok(_val) => {
                    log::trace!("Loading in-cluster config");
                    config::incluster_config().unwrap() // pub fn incluster_config() -> Result<Configuration> {
                }
                Err(_e) => {
                    log::trace!("Loading config file");
                    block_on(config::load_kube_config()).unwrap() // pub async fn load_kube_config() -> Result<Configuration>
                }
            },
        }
    }
}

#[async_trait]
impl KubeInterface for KubeImpl {
    /// Create new APIClient using KubeImpl's kube::config::Configuration
    fn get_kube_client(&self) -> APIClient {
        APIClient::new(self.kube_configuration.clone())
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
    /// let kube = k8s::create_kube_interface();
    /// let node = kube.find_node("node-a").await.unwrap();
    /// # }
    /// ```
    async fn find_node(
        &self,
        name: &str,
    ) -> Result<Object<NodeSpec, NodeStatus>, Box<dyn std::error::Error + Send + Sync + 'static>>
    {
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
    /// let kube = k8s::create_kube_interface();
    /// let interesting_pods = kube.find_pods_with_label("label=interesting").await.unwrap();
    /// # }
    /// ```
    async fn find_pods_with_label(
        &self,
        selector: &str,
    ) -> Result<
        ObjectList<Object<PodSpec, PodStatus>>,
        Box<dyn std::error::Error + Send + Sync + 'static>,
    > {
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
    /// let kube = k8s::create_kube_interface();
    /// let pods_on_node_a = kube.find_pods_with_field("spec.nodeName=node-a").await.unwrap();
    /// # }
    /// ```
    async fn find_pods_with_field(
        &self,
        selector: &str,
    ) -> Result<
        ObjectList<Object<PodSpec, PodStatus>>,
        Box<dyn std::error::Error + Send + Sync + 'static>,
    > {
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
    /// let kube = k8s::create_kube_interface();
    /// kube.create_pod(&Pod::default(), "pod_namespace").await.unwrap();
    /// # }
    /// ```
    async fn create_pod(
        &self,
        pod_to_create: &Pod,
        namespace: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
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
    /// let kube = k8s::create_kube_interface();
    /// kube.remove_pod("pod_to_remove", "pod_namespace").await.unwrap();
    /// # }
    /// ```
    async fn remove_pod(
        &self,
        pod_to_remove: &str,
        namespace: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        pod::remove_pod(pod_to_remove, namespace, self.get_kube_client()).await
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
    /// let kube = k8s::create_kube_interface();
    /// let interesting_services = kube.find_services("label=interesting").await.unwrap();
    /// # }
    /// ```
    async fn find_services(
        &self,
        selector: &str,
    ) -> Result<
        ObjectList<Object<ServiceSpec, ServiceStatus>>,
        Box<dyn std::error::Error + Send + Sync + 'static>,
    > {
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
    /// let kube = k8s::create_kube_interface();
    /// kube.create_service(&Service::default(), "service_namespace").await.unwrap();
    /// # }
    /// ```
    async fn create_service(
        &self,
        svc_to_create: &Service,
        namespace: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
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
    /// let kube = k8s::create_kube_interface();
    /// kube.remove_service("service_to_remove", "service_namespace").await.unwrap();
    /// # }
    /// ```
    async fn remove_service(
        &self,
        svc_to_remove: &str,
        namespace: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
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
    /// let kube = k8s::create_kube_interface();
    /// let selector = "environment=production,app=nginx";
    /// for svc in kube.find_services(&selector).await.unwrap() {
    ///     let svc_name = &svc.metadata.name.clone();
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
        svc_to_update: &Object<ServiceSpec, ServiceStatus>,
        name: &str,
        namespace: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
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
    /// let kube = k8s::create_kube_interface();
    /// let dcc = kube.find_configuration("dcc-1", "dcc-namespace").await.unwrap();
    /// # }
    /// ```
    async fn find_configuration(
        &self,
        name: &str,
        namespace: &str,
    ) -> Result<KubeAkriConfig, Box<dyn std::error::Error + Send + Sync + 'static>> {
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
    /// let kube = k8s::create_kube_interface();
    /// let dccs = kube.get_configurations().await.unwrap();
    /// # }
    /// ```
    async fn get_configurations(
        &self,
    ) -> Result<KubeAkriConfigList, Box<dyn std::error::Error + Send + Sync + 'static>> {
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
    /// let kube = k8s::create_kube_interface();
    /// let instance = kube.find_instance("instance-1", "instance-namespace").await.unwrap();
    /// # }
    /// ```
    async fn find_instance(
        &self,
        name: &str,
        namespace: &str,
    ) -> Result<KubeAkriInstance, Box<dyn std::error::Error + Send + Sync + 'static>> {
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
    /// let kube = k8s::create_kube_interface();
    /// let instances = kube.get_instances().await.unwrap();
    /// # }
    /// ```
    async fn get_instances(
        &self,
    ) -> Result<KubeAkriInstanceList, Box<dyn std::error::Error + Send + Sync + 'static>> {
        instance::get_instances(&self.get_kube_client()).await
    }
    /// Create Akri Instance
    ///
    /// Example:
    ///
    /// ```no_run
    /// use akri_shared::k8s;
    /// use akri_shared::k8s::KubeInterface;
    /// use akri_shared::akri::instance::Instance;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let kube = k8s::create_kube_interface();
    /// kube.create_instance(
    ///     &Instance{
    ///         configuration_name: "capability_configuration_name".to_string(),
    ///         shared: true,
    ///         nodes: Vec::new(),
    ///         device_usage: std::collections::HashMap::new(),
    ///         metadata: std::collections::HashMap::new(),
    ///         rbac: "".to_string(),
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
        instance_to_create: &Instance,
        name: &str,
        namespace: &str,
        owner_config_name: &str,
        owner_config_uid: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
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
    /// let kube = k8s::create_kube_interface();
    /// kube.delete_instance(
    ///     "instance-1",
    ///     "instance-namespace"
    /// ).await.unwrap();
    /// # }
    /// ```
    async fn delete_instance(
        &self,
        name: &str,
        namespace: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        instance::delete_instance(name, namespace, &self.get_kube_client()).await
    }
    /// Update Akri Instance
    ///
    /// Example:
    ///
    /// ```no_run
    /// use akri_shared::k8s;
    /// use akri_shared::k8s::KubeInterface;
    /// use akri_shared::akri::instance::Instance;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let kube = k8s::create_kube_interface();
    /// kube.update_instance(
    ///     &Instance{
    ///         configuration_name: "capability_configuration_name".to_string(),
    ///         shared: true,
    ///         nodes: Vec::new(),
    ///         device_usage: std::collections::HashMap::new(),
    ///         metadata: std::collections::HashMap::new(),
    ///         rbac: "".to_string(),
    ///     },
    ///     "instance-1",
    ///     "instance-namespace"
    /// ).await.unwrap();
    /// # }
    /// ```
    async fn update_instance(
        &self,
        instance_to_update: &Instance,
        name: &str,
        namespace: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        instance::update_instance(instance_to_update, name, namespace, &self.get_kube_client())
            .await
    }
}

pub mod test_kube {
    use super::super::akri::{
        configuration::{KubeAkriConfig, KubeAkriConfigList},
        instance::{Instance, KubeAkriInstance, KubeAkriInstanceList},
    };
    use super::KubeInterface;
    use async_trait::async_trait;
    use k8s_openapi::api::core::v1::{
        NodeSpec, NodeStatus, Pod, PodSpec, PodStatus, Service, ServiceSpec, ServiceStatus,
    };
    use kube::{
        api::{Object, ObjectList},
        client::APIClient,
    };
    use mockall::predicate::*;
    use mockall::*;

    //
    // mockall and async_trait do not work effortlessly together ... to enable both,
    // follow the example here:
    //     https://github.com/mibes/mockall-async/blob/53aec15219a720ef5ac483959ff8821cb7d656ae/src/main.rs
    //
    // We can probably eliminate this when async traits are supported by Rust without
    // the async_trait crate.
    //
    mock! {
        pub KubeImpl {
            fn get_kube_client(&self) -> APIClient;

            fn find_node(&self, name: &str) -> Result<Object<NodeSpec, NodeStatus>, Box<dyn std::error::Error + Send + Sync + 'static>>;

            fn find_pods_with_label(&self, selector: &str) -> Result<ObjectList<Object<PodSpec, PodStatus>>, Box<dyn std::error::Error + Send + Sync + 'static>>;
            fn find_pods_with_field(&self, selector: &str) -> Result<ObjectList<Object<PodSpec, PodStatus>>, Box<dyn std::error::Error + Send + Sync + 'static>>;
            fn create_pod(&self, pod_to_create: &Pod, namespace: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>>;
            fn remove_pod(&self, pod_to_remove: &str, namespace: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>>;

            fn find_services(&self, selector: &str) -> Result<ObjectList<Object<ServiceSpec, ServiceStatus>>, Box<dyn std::error::Error + Send + Sync + 'static>>;
            fn create_service(&self, svc_to_create: &Service, namespace: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>>;
            fn remove_service(&self, svc_to_remove: &str, namespace: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>>;
            fn update_service(&self, svc_to_update: &Object<ServiceSpec, ServiceStatus>, name: &str, namespace: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>>;

            fn find_configuration(&self, name: &str, namespace: &str) -> Result<KubeAkriConfig, Box<dyn std::error::Error + Send + Sync + 'static>>;
            fn get_configurations(&self) -> Result<KubeAkriConfigList, Box<dyn std::error::Error + Send + Sync + 'static>>;

            fn find_instance(&self, name: &str, namespace: &str) -> Result<KubeAkriInstance, Box<dyn std::error::Error + Send + Sync + 'static>>;
            fn get_instances(&self) -> Result<KubeAkriInstanceList, Box<dyn std::error::Error + Send + Sync + 'static>>;
            fn create_instance(&self, instance_to_create: &Instance, name: &str, namespace: &str, owner_config_name: &str, owner_config_uid: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>>;
            fn delete_instance(&self, name: &str, namespace: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>>;
            fn update_instance(&self, instance_to_update: &Instance, name: &str, namespace: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>>;
        }
    }

    #[async_trait]
    impl KubeInterface for MockKubeImpl {
        fn get_kube_client(&self) -> APIClient {
            self.get_kube_client()
        }

        async fn find_node(
            &self,
            name: &str,
        ) -> Result<Object<NodeSpec, NodeStatus>, Box<dyn std::error::Error + Send + Sync + 'static>>
        {
            self.find_node(name)
        }

        async fn find_pods_with_label(
            &self,
            selector: &str,
        ) -> Result<
            ObjectList<Object<PodSpec, PodStatus>>,
            Box<dyn std::error::Error + Send + Sync + 'static>,
        > {
            self.find_pods_with_label(selector)
        }
        async fn find_pods_with_field(
            &self,
            selector: &str,
        ) -> Result<
            ObjectList<Object<PodSpec, PodStatus>>,
            Box<dyn std::error::Error + Send + Sync + 'static>,
        > {
            self.find_pods_with_field(selector)
        }
        async fn create_pod(
            &self,
            pod_to_create: &Pod,
            namespace: &str,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
            self.create_pod(pod_to_create, namespace)
        }
        async fn remove_pod(
            &self,
            pod_to_remove: &str,
            namespace: &str,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
            self.remove_pod(pod_to_remove, namespace)
        }

        async fn find_services(
            &self,
            selector: &str,
        ) -> Result<
            ObjectList<Object<ServiceSpec, ServiceStatus>>,
            Box<dyn std::error::Error + Send + Sync + 'static>,
        > {
            self.find_services(selector)
        }
        async fn create_service(
            &self,
            svc_to_create: &Service,
            namespace: &str,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
            self.create_service(svc_to_create, namespace)
        }
        async fn remove_service(
            &self,
            svc_to_remove: &str,
            namespace: &str,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
            self.remove_service(svc_to_remove, namespace)
        }
        async fn update_service(
            &self,
            svc_to_update: &Object<ServiceSpec, ServiceStatus>,
            name: &str,
            namespace: &str,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
            self.update_service(svc_to_update, name, namespace)
        }

        async fn find_configuration(
            &self,
            name: &str,
            namespace: &str,
        ) -> Result<KubeAkriConfig, Box<dyn std::error::Error + Send + Sync + 'static>> {
            self.find_configuration(name, namespace)
        }
        async fn get_configurations(
            &self,
        ) -> Result<KubeAkriConfigList, Box<dyn std::error::Error + Send + Sync + 'static>>
        {
            self.get_configurations()
        }

        async fn find_instance(
            &self,
            name: &str,
            namespace: &str,
        ) -> Result<KubeAkriInstance, Box<dyn std::error::Error + Send + Sync + 'static>> {
            self.find_instance(name, namespace)
        }
        async fn get_instances(
            &self,
        ) -> Result<KubeAkriInstanceList, Box<dyn std::error::Error + Send + Sync + 'static>>
        {
            self.get_instances()
        }
        async fn create_instance(
            &self,
            instance_to_create: &Instance,
            name: &str,
            namespace: &str,
            owner_config_name: &str,
            owner_config_uid: &str,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
            self.create_instance(
                instance_to_create,
                name,
                namespace,
                owner_config_name,
                owner_config_uid,
            )
        }
        async fn delete_instance(
            &self,
            name: &str,
            namespace: &str,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
            self.delete_instance(name, namespace)
        }
        async fn update_instance(
            &self,
            instance_to_update: &Instance,
            name: &str,
            namespace: &str,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
            self.update_instance(instance_to_update, name, namespace)
        }
    }
}

#[cfg(test)]
pub mod test_ownership {
    use super::*;

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
        assert_eq!(true, ownership.get_controller());
        assert_eq!(true, ownership.get_block_owner_deletion());
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
        assert_eq!(true, ownership.get_controller());
        assert_eq!(true, ownership.get_block_owner_deletion());
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
        assert_eq!(true, ownership.get_controller());
        assert_eq!(true, ownership.get_block_owner_deletion());
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
        assert_eq!(true, ownership.get_controller());
        assert_eq!(true, ownership.get_block_owner_deletion());
        assert_eq!(name, &ownership.get_name());
        assert_eq!(uid, &ownership.get_uid());
    }
}
