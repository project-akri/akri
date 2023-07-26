use super::{API_NAMESPACE, API_VERSION};
use kube::{
    api::{Api, DeleteParams, ListParams, ObjectList, ObjectMeta, Patch, PatchParams, PostParams},
    Client, CustomResource,
};

use k8s_openapi::apimachinery::pkg::apis::meta::v1::OwnerReference;
use schemars::JsonSchema;
use std::collections::HashMap;

pub type InstanceList = ObjectList<Instance>;

/// Defines the information in the Instance CRD
///
/// An Instance is a specific instance described by
/// a Configuration.  For example, a Configuration
/// may describe many cameras, each camera will be represented by a
/// Instance.
#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
// group = API_NAMESPACE and version = API_VERSION
#[kube(group = "akri.sh", version = "v0", kind = "Instance", namespaced)]
pub struct InstanceSpec {
    /// This contains the name of the corresponding Configuration
    pub configuration_name: String,

    /// This defines some properties that will be set as
    /// environment variables in broker Pods that request
    /// the resource this Instance represents.
    /// It contains the `Configuration.broker_properties` from
    /// this Instance's Configuration and the `Device.properties`
    /// set by the Discovery Handler that discovered the resource
    /// this Instance represents.
    #[serde(default)]
    pub broker_properties: HashMap<String, String>,

    /// This defines whether the capability is to be shared by multiple nodes
    #[serde(default = "default_shared")]
    pub shared: bool,

    /// This contains a list of the nodes that can access this capability instance
    #[serde(default)]
    pub nodes: Vec<String>,

    /// This contains a map of capability slots to node names.  The number of
    /// slots corresponds to the associated Configuration.capacity
    /// field.  Each slot will either map to an empty string (if the slot has not
    /// been claimed) or to a node name (corresponding to the node that has claimed
    /// the slot)
    #[serde(default)]
    pub device_usage: HashMap<String, String>,
}

/// Get Instances for a given namespace
///
/// Example:
///
/// ```no_run
/// use akri_shared::akri::instance;
/// use kube::client::Client;
/// use kube::config;
///
/// # #[tokio::main]
/// # async fn main() {
/// let api_client = Client::try_default().await.unwrap();
/// let instances = instance::get_instances(&api_client).await.unwrap();
/// # }
/// ```
pub async fn get_instances(kube_client: &Client) -> Result<InstanceList, anyhow::Error> {
    log::trace!("get_instances enter");
    let instances_client: Api<Instance> = Api::all(kube_client.clone());
    let lp = ListParams::default();
    match instances_client.list(&lp).await {
        Ok(instances_retrieved) => {
            log::trace!("get_instances return");
            Ok(instances_retrieved)
        }
        Err(kube::Error::Api(ae)) => {
            log::trace!(
                "get_instances kube_client.request returned kube error: {:?}",
                ae
            );
            Err(ae.into())
        }
        Err(e) => {
            log::trace!("get_instances kube_client.request error: {:?}", e);
            Err(e.into())
        }
    }
}

/// Get Instance for a given name and namespace
///
/// Example:
///
/// ```no_run
/// use akri_shared::akri::instance;
/// use kube::client::Client;
/// use kube::config;
///
/// # #[tokio::main]
/// # async fn main() {
/// let api_client = Client::try_default().await.unwrap();
/// let instance = instance::find_instance(
///     "config-1",
///     "default",
///     &api_client).await.unwrap();
/// # }
/// ```
pub async fn find_instance(
    name: &str,
    namespace: &str,
    kube_client: &Client,
) -> Result<Instance, anyhow::Error> {
    log::trace!("find_instance enter");
    let instances_client: Api<Instance> = Api::namespaced(kube_client.clone(), namespace);

    log::trace!("find_instance getting instance with name {}", name);

    match instances_client.get(name).await {
        Ok(instance_retrieved) => {
            log::trace!("find_instance return");
            Ok(instance_retrieved)
        }
        Err(e) => match e {
            kube::Error::Api(ae) => {
                log::trace!(
                    "find_instance kube_client.request returned kube error: {:?}",
                    ae
                );
                Err(anyhow::anyhow!(ae))
            }
            _ => {
                log::trace!("find_instance kube_client.request error: {:?}", e);
                Err(anyhow::anyhow!(e))
            }
        },
    }
}

/// Create Instance
///
/// Example:
///
/// ```no_run
/// use akri_shared::akri::instance::InstanceSpec;
/// use akri_shared::akri::instance;
/// use kube::client::Client;
/// use kube::config;
///
/// # #[tokio::main]
/// # async fn main() {
/// let api_client = Client::try_default().await.unwrap();
/// let instance = instance::create_instance(
///     &InstanceSpec {
///         configuration_name: "capability_configuration_name".to_string(),
///         shared: true,
///         nodes: Vec::new(),
///         device_usage: std::collections::HashMap::new(),
///         broker_properties: std::collections::HashMap::new(),
///     },
///     "instance-1",
///     "default",
///     "config-1",
///     "abcdefgh-ijkl-mnop-qrst-uvwxyz012345",
///     &api_client).await.unwrap();
/// # }
/// ```
pub async fn create_instance(
    instance_to_create: &InstanceSpec,
    name: &str,
    namespace: &str,
    owner_config_name: &str,
    owner_config_uid: &str,
    kube_client: &Client,
) -> Result<(), anyhow::Error> {
    log::trace!("create_instance enter");
    let instances_client: Api<Instance> = Api::namespaced(kube_client.clone(), namespace);

    let mut instance = Instance::new(name, instance_to_create.clone());
    instance.metadata = ObjectMeta {
        name: Some(name.to_string()),
        owner_references: Some(vec![OwnerReference {
            api_version: format!("{}/{}", API_NAMESPACE, API_VERSION),
            kind: "Configuration".to_string(),
            controller: Some(true),
            block_owner_deletion: Some(true),
            name: owner_config_name.to_string(),
            uid: owner_config_uid.to_string(),
        }]),
        ..Default::default()
    };
    match instances_client
        .create(&PostParams::default(), &instance)
        .await
    {
        Ok(_instance_created) => {
            log::trace!("create_instance return");
            Ok(())
        }
        Err(kube::Error::Api(ae)) => {
            log::trace!(
                "create_instance kube_client.request returned kube error: {:?}",
                ae
            );
            Err(ae.into())
        }
        Err(e) => {
            log::trace!("create_instance kube_client.request error: {:?}", e);
            Err(e.into())
        }
    }
}

/// Delete Instance for a given name and namespace
///
/// Example:
///
/// ```no_run
/// use akri_shared::akri::instance;
/// use kube::client::Client;
/// use kube::config;
///
/// # #[tokio::main]
/// # async fn main() {
/// let api_client = Client::try_default().await.unwrap();
/// let instance = instance::delete_instance(
///     "instance-1",
///     "default",
///     &api_client).await.unwrap();
/// # }
/// ```
pub async fn delete_instance(
    name: &str,
    namespace: &str,
    kube_client: &Client,
) -> Result<(), anyhow::Error> {
    log::trace!("delete_instance enter");
    let instances_client: Api<Instance> = Api::namespaced(kube_client.clone(), namespace);
    let instance_delete_params = DeleteParams::default();
    log::trace!("delete_instance instances_client.delete(name, &instance_delete_params).await?");
    match instances_client.delete(name, &instance_delete_params).await {
        Ok(_void_response) => {
            log::trace!("delete_instance return");
            Ok(())
        }
        Err(kube::Error::Api(ae)) => {
            log::trace!(
                "delete_instance kube_client.request returned kube error: {:?}",
                ae
            );
            Err(ae.into())
        }
        Err(e) => {
            log::trace!("delete_instance kube_client.request error: {:?}", e);
            Err(e.into())
        }
    }
}

/// Update Instance
///
/// Example:
///
/// ```no_run
/// use akri_shared::akri::instance::InstanceSpec;
/// use akri_shared::akri::instance;
/// use kube::client::Client;
/// use kube::config;
///
/// # #[tokio::main]
/// # async fn main() {
/// let api_client = Client::try_default().await.unwrap();
/// let instance = instance::update_instance(
///     &InstanceSpec {
///         configuration_name: "capability_configuration_name".to_string(),
///         shared: true,
///         nodes: Vec::new(),
///         device_usage: std::collections::HashMap::new(),
///         broker_properties: std::collections::HashMap::new(),
///     },
///     "instance-1",
///     "default",
///     &api_client).await.unwrap();
/// # }
/// ```
pub async fn update_instance(
    instance_to_update: &InstanceSpec,
    name: &str,
    namespace: &str,
    kube_client: &Client,
) -> Result<(), anyhow::Error> {
    log::trace!("update_instance enter");
    let instances_client: Api<Instance> = Api::namespaced(kube_client.clone(), namespace);
    let modified_instance = Instance::new(name, instance_to_update.clone());
    match instances_client
        .patch(
            name,
            &PatchParams::default(),
            &Patch::Merge(&modified_instance),
        )
        .await
    {
        Ok(_instance_modified) => {
            log::trace!("update_instance return");
            Ok(())
        }
        Err(kube::Error::Api(ae)) => {
            log::trace!(
                "update_instance kube_client.request returned kube error: {:?}",
                ae
            );
            Err(ae.into())
        }
        Err(e) => {
            log::trace!("update_instance kube_client.request error: {:?}", e);
            Err(e.into())
        }
    }
}

fn default_shared() -> bool {
    false
}

pub mod device_usage {
    #[derive(PartialEq, Clone, Debug, Default)]
    pub enum DeviceUsageKind {
        /// Device is free
        #[default]
        Free,
        /// Device is reserved by Instance Device Plugin
        Instance,
        /// Device is reserved by Configuration Device Plugin
        Configuration(String),
    }
    #[derive(Debug, PartialEq, Eq)]
    pub struct ParseNodeUsageError;
    #[derive(PartialEq, Clone, Debug, Default)]
    pub struct NodeUsage {
        kind: DeviceUsageKind,
        node_name: String,
    }

    impl std::fmt::Display for NodeUsage {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match &self.kind {
                DeviceUsageKind::Free => write!(f, ""),
                DeviceUsageKind::Configuration(vdev_id) => {
                    write!(f, "C:{}:{}", vdev_id, self.node_name)
                }
                DeviceUsageKind::Instance => write!(f, "{}", self.node_name),
            }
        }
    }

    impl std::str::FromStr for NodeUsage {
        type Err = ParseNodeUsageError;
        fn from_str(s: &str) -> Result<Self, Self::Err> {
            if s.is_empty() {
                return Ok(NodeUsage {
                    kind: DeviceUsageKind::Free,
                    node_name: s.to_string(),
                });
            }

            // Format "C:<vdev_id>:<node_name>"
            if let Some((vdev_id, node_name)) = s.strip_prefix("C:").and_then(|s| s.split_once(':'))
            {
                if node_name.is_empty() {
                    return Err(ParseNodeUsageError);
                }
                return Ok(NodeUsage {
                    kind: DeviceUsageKind::Configuration(vdev_id.to_string()),
                    node_name: node_name.to_string(),
                });
            }

            // Format "<node_name>"
            Ok(NodeUsage {
                kind: DeviceUsageKind::Instance,
                node_name: s.to_string(),
            })
        }
    }

    impl NodeUsage {
        pub fn create(kind: &DeviceUsageKind, node_name: &str) -> Result<Self, anyhow::Error> {
            match kind {
                DeviceUsageKind::Free => {
                    if !node_name.is_empty() {
                        return Err(anyhow::anyhow!(
                            "Invalid input parameter, node name: {} provided for free node usage",
                            node_name
                        ));
                    };
                }
                _ => {
                    if node_name.is_empty() {
                        return Err(anyhow::anyhow!(
                            "Invalid input parameter, no node name provided for node usage"
                        ));
                    };
                }
            };

            Ok(Self {
                kind: kind.clone(),
                node_name: node_name.into(),
            })
        }

        pub fn get_kind(&self) -> DeviceUsageKind {
            self.kind.clone()
        }

        pub fn get_node_name(&self) -> String {
            self.node_name.clone()
        }

        pub fn is_same_node(&self, node_name: &str) -> bool {
            self.node_name == node_name
        }
    }
}

#[cfg(test)]
mod crd_serializeation_tests {
    use super::super::super::os::file;
    use super::*;
    use env_logger;

    #[test]
    #[should_panic]
    fn test_instance_no_class_name_failure() {
        let _ = env_logger::builder().is_test(true).try_init();

        let json = r#"{}"#;
        let _: InstanceSpec = serde_json::from_str(json).unwrap();
    }

    #[test]
    fn test_instance_defaults_with_json_serialization() {
        let _ = env_logger::builder().is_test(true).try_init();

        let json = r#"{"configurationName": "foo"}"#;
        let deserialized: InstanceSpec = serde_json::from_str(json).unwrap();
        assert_eq!("foo".to_string(), deserialized.configuration_name);
        assert_eq!(0, deserialized.broker_properties.len());
        assert_eq!(default_shared(), deserialized.shared);
        assert_eq!(0, deserialized.nodes.len());
        assert_eq!(0, deserialized.device_usage.len());

        let serialized = serde_json::to_string(&deserialized).unwrap();
        let expected_deserialized = r#"{"configurationName":"foo","brokerProperties":{},"shared":false,"nodes":[],"deviceUsage":{}}"#;
        assert_eq!(expected_deserialized, serialized);
    }

    #[test]
    fn test_instance_defaults_with_yaml_serialization() {
        let _ = env_logger::builder().is_test(true).try_init();

        let json = r#"
        configurationName: foo
        "#;
        let deserialized: InstanceSpec = serde_yaml::from_str(json).unwrap();
        assert_eq!("foo".to_string(), deserialized.configuration_name);
        assert_eq!(0, deserialized.broker_properties.len());
        assert_eq!(default_shared(), deserialized.shared);
        assert_eq!(0, deserialized.nodes.len());
        assert_eq!(0, deserialized.device_usage.len());

        let serialized = serde_json::to_string(&deserialized).unwrap();
        let expected_deserialized = r#"{"configurationName":"foo","brokerProperties":{},"shared":false,"nodes":[],"deviceUsage":{}}"#;
        assert_eq!(expected_deserialized, serialized);
    }

    #[test]
    fn test_instance_serialization() {
        let _ = env_logger::builder().is_test(true).try_init();

        let json = r#"{"configurationName":"blah","brokerProperties":{"a":"two"},"shared":true,"nodes":["n1","n2"],"deviceUsage":{"0":"","1":"n1"}}"#;
        let deserialized: InstanceSpec = serde_json::from_str(json).unwrap();
        assert_eq!("blah".to_string(), deserialized.configuration_name);
        assert_eq!(1, deserialized.broker_properties.len());
        assert!(deserialized.shared);
        assert_eq!(2, deserialized.nodes.len());
        assert_eq!(2, deserialized.device_usage.len());

        let _ = serde_json::to_string(&deserialized).unwrap();
    }

    #[test]
    fn test_real_instance() {
        let _ = env_logger::builder().is_test(true).try_init();

        let files = [
            "../test/yaml/akri-instance-onvif-camera.yaml",
            "../test/yaml/akri-instance-usb-camera.yaml",
        ];
        for file in &files {
            let yaml = file::read_file_to_string(file);
            let deserialized: Instance = serde_yaml::from_str(&yaml).unwrap();
            let _ = serde_json::to_string(&deserialized).unwrap();
        }
    }
}
