use super::{API_INSTANCES, API_NAMESPACE, API_VERSION};
use kube::{
    api::{
        DeleteParams, ListParams, Object, ObjectList, ObjectMeta, OwnerReference, PatchParams,
        PostParams, RawApi, TypeMeta, Void,
    },
    client::APIClient,
};
use std::collections::HashMap;

pub type KubeAkriInstance = Object<Instance, Void>;
pub type KubeAkriInstanceList = ObjectList<Object<Instance, Void>>;

/// Defines the information in the Instance CRD
///
/// An Instance is a specific instance described by
/// a Configuration.  For example, a Configuration
/// may describe many cameras, each camera will be represented by a
/// Instance.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Instance {
    /// This contains the name of the corresponding Configuration
    pub configuration_name: String,

    /// This stores information about the capability that must be communicated to
    /// a protocol broker
    #[serde(default)]
    pub metadata: HashMap<String, String>,

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

    /// This is a placeholder for eventual RBAC support
    #[serde(default = "default_rbac")]
    pub rbac: String,
}

/// Get Instances for a given namespace
///
/// Example:
///
/// ```no_run
/// use akri_shared::akri::instance;
/// use kube::client::APIClient;
/// use kube::config;
///
/// # #[tokio::main]
/// # async fn main() {
/// let api_client = APIClient::new(config::incluster_config().unwrap());
/// let instances = instance::get_instances(&api_client).await.unwrap();
/// # }
/// ```
pub async fn get_instances(
    kube_client: &APIClient,
) -> Result<KubeAkriInstanceList, Box<dyn std::error::Error + Send + Sync + 'static>> {
    log::trace!("get_instances enter");
    let akri_instance_type = RawApi::customResource(API_INSTANCES)
        .group(API_NAMESPACE)
        .version(API_VERSION);

    log::trace!("get_instances kube_client.request::<KubeAkriInstanceList>(akri_instance_type.list(...)?).await?");

    let instance_list_params = ListParams {
        ..Default::default()
    };
    match kube_client
        .request::<KubeAkriInstanceList>(akri_instance_type.list(&instance_list_params)?)
        .await
    {
        Ok(configs_retrieved) => {
            log::trace!("get_instances return");
            Ok(configs_retrieved)
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
/// use kube::client::APIClient;
/// use kube::config;
///
/// # #[tokio::main]
/// # async fn main() {
/// let api_client = APIClient::new(config::incluster_config().unwrap());
/// let instance = instance::find_instance(
///     "dcc-1",
///     "default",
///     &api_client).await.unwrap();
/// # }
/// ```
pub async fn find_instance(
    name: &str,
    namespace: &str,
    kube_client: &APIClient,
) -> Result<KubeAkriInstance, kube::Error> {
    log::trace!("find_instance enter");
    let akri_instance_type = RawApi::customResource(API_INSTANCES)
        .group(API_NAMESPACE)
        .version(API_VERSION)
        .within(&namespace);

    log::trace!(
        "find_instance kube_client.request::<KubeAkriInstance>(akri_instance_type.get(...)?).await?"
    );

    match kube_client
        .request::<KubeAkriInstance>(akri_instance_type.get(&name)?)
        .await
    {
        Ok(config_retrieved) => {
            log::trace!("find_instance return");
            Ok(config_retrieved)
        }
        Err(e) => match e {
            kube::Error::Api(ae) => {
                log::trace!(
                    "find_instance kube_client.request returned kube error: {:?}",
                    ae
                );
                Err(kube::Error::Api(ae))
            }
            _ => {
                log::trace!("find_instance kube_client.request error: {:?}", e);
                Err(e)
            }
        },
    }
}

/// Create Instance
///
/// Example:
///
/// ```no_run
/// use akri_shared::akri::instance::Instance;
/// use akri_shared::akri::instance;
/// use kube::client::APIClient;
/// use kube::config;
///
/// # #[tokio::main]
/// # async fn main() {
/// let api_client = APIClient::new(config::incluster_config().unwrap());
/// let instance = instance::create_instance(
///     &Instance {
///         configuration_name: "capability_configuration_name".to_string(),
///         shared: true,
///         nodes: Vec::new(),
///         device_usage: std::collections::HashMap::new(),
///         metadata: std::collections::HashMap::new(),
///         rbac: "".to_string(),
///     },
///     "instance-1",
///     "default",
///     "config-1",
///     "abcdefgh-ijkl-mnop-qrst-uvwxyz012345",
///     &api_client).await.unwrap();
/// # }
/// ```
pub async fn create_instance(
    instance_to_create: &Instance,
    name: &str,
    namespace: &str,
    owner_config_name: &str,
    owner_config_uid: &str,
    kube_client: &APIClient,
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    log::trace!("create_instance enter");
    let akri_instance_type = RawApi::customResource(API_INSTANCES)
        .group(API_NAMESPACE)
        .version(API_VERSION)
        .within(&namespace);

    let kube_instance = KubeAkriInstance {
        metadata: ObjectMeta {
            name: name.to_string(),
            ownerReferences: vec![OwnerReference {
                apiVersion: format!("{}/{}", API_NAMESPACE, API_VERSION),
                kind: "Configuration".to_string(),
                controller: true,
                blockOwnerDeletion: true,
                name: owner_config_name.to_string(),
                uid: owner_config_uid.to_string(),
            }],
            ..Default::default()
        },
        spec: instance_to_create.clone(),
        status: None,
        types: TypeMeta {
            apiVersion: Some(format!("{}/{}", API_NAMESPACE, API_VERSION)),
            kind: Some("Instance".to_string()),
        },
    };
    let binary_instance = serde_json::to_vec(&kube_instance)?;
    log::trace!("create_instance akri_instance_type.create");
    let instance_create_params = PostParams::default();
    let create_request = akri_instance_type
        .create(&instance_create_params, binary_instance)
        .expect("failed to create request");
    log::trace!("create_instance kube_client.request::<KubeAkriInstance>(akri_instance_type.create(...)?).await?");
    match kube_client
        .request::<KubeAkriInstance>(create_request)
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
/// use kube::client::APIClient;
/// use kube::config;
///
/// # #[tokio::main]
/// # async fn main() {
/// let api_client = APIClient::new(config::incluster_config().unwrap());
/// let instance = instance::delete_instance(
///     "instance-1",
///     "default",
///     &api_client).await.unwrap();
/// # }
/// ```
pub async fn delete_instance(
    name: &str,
    namespace: &str,
    kube_client: &APIClient,
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    log::trace!("delete_instance enter");
    let akri_instance_type = RawApi::customResource(API_INSTANCES)
        .group(API_NAMESPACE)
        .version(API_VERSION)
        .within(&namespace);

    log::trace!("delete_instance akri_instance_type.delete");
    let instance_delete_params = DeleteParams::default();
    let delete_request = akri_instance_type
        .delete(name, &instance_delete_params)
        .expect("failed to delete request");
    log::trace!("delete_instance kube_client.request::<KubeAkriInstance>(akri_instance_type.delete(...)?).await?");
    match kube_client.request::<Void>(delete_request).await {
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
/// use akri_shared::akri::instance::Instance;
/// use akri_shared::akri::instance;
/// use kube::client::APIClient;
/// use kube::config;
///
/// # #[tokio::main]
/// # async fn main() {
/// let api_client = APIClient::new(config::incluster_config().unwrap());
/// let instance = instance::update_instance(
///     &Instance {
///         configuration_name: "capability_configuration_name".to_string(),
///         shared: true,
///         nodes: Vec::new(),
///         device_usage: std::collections::HashMap::new(),
///         metadata: std::collections::HashMap::new(),
///         rbac: "".to_string(),
///     },
///     "instance-1",
///     "default",
///     &api_client).await.unwrap();
/// # }
/// ```
pub async fn update_instance(
    instance_to_update: &Instance,
    name: &str,
    namespace: &str,
    kube_client: &APIClient,
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    log::trace!("update_instance enter");
    let akri_instance_type = RawApi::customResource(API_INSTANCES)
        .group(API_NAMESPACE)
        .version(API_VERSION)
        .within(&namespace);

    let existing_kube_akri_instance_type = find_instance(name, namespace, kube_client).await?;
    let modified_kube_instance = KubeAkriInstance {
        metadata: existing_kube_akri_instance_type.metadata,
        spec: instance_to_update.clone(),
        status: existing_kube_akri_instance_type.status,
        types: existing_kube_akri_instance_type.types,
    };
    log::trace!(
        "update_instance wrapped_instance: {:?}",
        serde_json::to_string(&modified_kube_instance).unwrap()
    );
    let binary_instance = serde_json::to_vec(&modified_kube_instance)?;

    log::trace!("update_instance akri_instance_type.patch");
    let instance_patch_params = PatchParams::default();
    let patch_request = akri_instance_type
        .patch(name, &instance_patch_params, binary_instance)
        .expect("failed to create request");
    log::trace!("update_instance kube_client.request::<KubeAkriInstance>(akri_instance_type.patch(...)?).await?");
    match kube_client.request::<KubeAkriInstance>(patch_request).await {
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
fn default_rbac() -> String {
    "".to_string()
}

#[cfg(test)]
mod crd_serializeation_tests {
    use super::super::super::os::file;
    use super::*;
    use env_logger;

    #[derive(Serialize, Deserialize, Clone, Debug)]
    #[serde(rename_all = "camelCase")]
    struct InstanceCRD {
        api_version: String,
        kind: String,
        metadata: HashMap<String, String>,
        spec: Instance,
    }

    #[test]
    #[should_panic]
    fn test_instance_no_class_name_failure() {
        let _ = env_logger::builder().is_test(true).try_init();

        let json = r#"{}"#;
        let _: Instance = serde_json::from_str(json).unwrap();
    }

    #[test]
    fn test_instance_defaults_with_json_serialization() {
        let _ = env_logger::builder().is_test(true).try_init();

        let json = r#"{"configurationName": "foo"}"#;
        let deserialized: Instance = serde_json::from_str(json).unwrap();
        assert_eq!("foo".to_string(), deserialized.configuration_name);
        assert_eq!(0, deserialized.metadata.len());
        assert_eq!(default_shared(), deserialized.shared);
        assert_eq!(0, deserialized.nodes.len());
        assert_eq!(0, deserialized.device_usage.len());
        assert_eq!(0, deserialized.rbac.len());

        let serialized = serde_json::to_string(&deserialized).unwrap();
        let expected_deserialized = r#"{"configurationName":"foo","metadata":{},"shared":false,"nodes":[],"deviceUsage":{},"rbac":""}"#;
        assert_eq!(expected_deserialized, serialized);
    }

    #[test]
    fn test_instance_defaults_with_yaml_serialization() {
        let _ = env_logger::builder().is_test(true).try_init();

        let json = r#"
        configurationName: foo
        "#;
        let deserialized: Instance = serde_yaml::from_str(json).unwrap();
        assert_eq!("foo".to_string(), deserialized.configuration_name);
        assert_eq!(0, deserialized.metadata.len());
        assert_eq!(default_shared(), deserialized.shared);
        assert_eq!(0, deserialized.nodes.len());
        assert_eq!(0, deserialized.device_usage.len());
        assert_eq!(0, deserialized.rbac.len());

        let serialized = serde_json::to_string(&deserialized).unwrap();
        let expected_deserialized = r#"{"configurationName":"foo","metadata":{},"shared":false,"nodes":[],"deviceUsage":{},"rbac":""}"#;
        assert_eq!(expected_deserialized, serialized);
    }

    #[test]
    fn test_instance_serialization() {
        let _ = env_logger::builder().is_test(true).try_init();

        let json = r#"{"configurationName":"blah","metadata":{"a":"two"},"shared":true,"nodes":["n1","n2"],"deviceUsage":{"0":"","1":"n1"}}"#;
        let deserialized: Instance = serde_json::from_str(json).unwrap();
        assert_eq!("blah".to_string(), deserialized.configuration_name);
        assert_eq!(1, deserialized.metadata.len());
        assert_eq!(true, deserialized.shared);
        assert_eq!(2, deserialized.nodes.len());
        assert_eq!(2, deserialized.device_usage.len());
        assert_eq!(0, deserialized.rbac.len());

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
            let yaml = file::read_file_to_string(&file);
            let deserialized: InstanceCRD = serde_yaml::from_str(&yaml).unwrap();
            let _ = serde_json::to_string(&deserialized).unwrap();
        }
    }
}
