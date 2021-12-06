//
// Use this to allow DiscoveryHandlerInfo enum
// to use case similar to other properties (specifically avoiding CamelCase,
// in favor of camelCase)
//
#![allow(non_camel_case_types)]
use k8s_openapi::api::batch::v1::JobSpec;
use k8s_openapi::api::core::v1::PodSpec;
use k8s_openapi::api::core::v1::ServiceSpec;
use kube::CustomResource;
use kube::{
    api::{Api, ListParams, ObjectList},
    client::Client,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub type ConfigurationList = ObjectList<Configuration>;

/// This specifies which `DiscoveryHandler` should be used for discovery
/// and any details that need to be sent to the `DiscoveryHandler`.
#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryHandlerInfo {
    pub name: String,
    /// A string that a Discovery Handler knows how to parse to obtain necessary discovery details
    #[serde(default)]
    pub discovery_details: String,
}

/// This defines a workload that should be scheduled to nodes
/// that can access capability described by this Configuration.
/// The enum contents are boxed to use the heap instead of the stack.
/// See this clippy warning for more details:
/// https://rust-lang.github.io/rust-clippy/master/index.html#large_enum_variant
#[derive(Serialize, Deserialize, PartialEq, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum BrokerType {
    // Pod that should be scheduled to all nodes that can
    // access any capability described by this Configuration
    Pod(Box<PodSpec>),
    // Pod that should be deployed to each capability described by this Configuration
    Job(Box<JobSpec>),
}

/// Defines the information in the Akri Configuration CRD
///
/// A Configuration is the primary method for users to describe anticipated
/// capabilities.  For any specific capability found that is described by this
/// configuration, an Instance
/// is created.
#[derive(CustomResource, Serialize, Deserialize, Clone, Debug, JsonSchema, PartialEq)]
// group = API_NAMESPACE and version = API_VERSION
#[kube(group = "akri.sh", version = "v0", kind = "Configuration", namespaced)]
#[kube(apiextensions = "v1")]
#[serde(rename_all = "camelCase")]
pub struct ConfigurationSpec {
    /// This defines the `DiscoveryHandler` that should be used to
    /// discover the capability and any information needed by the `DiscoveryHandler`.
    pub discovery_handler: DiscoveryHandlerInfo,

    /// This defines a workload that should be scheduled to any
    /// node that can access any capability described by this
    /// configuration
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub broker_type: Option<BrokerType>,

    /// This defines a service that should be created to access
    /// any specific capability found that is described by this
    /// configuration. For each Configuration, several Instances
    /// can be found.  For each Instance, there is at most 1
    /// instance service.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance_service_spec: Option<ServiceSpec>,

    /// This defines a service that should be created to access
    /// all of the capabilities found that are described by this
    /// configuration. For each Configuration, there is at most
    /// 1 device capability service.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub configuration_service_spec: Option<ServiceSpec>,

    /// This defines some properties that will be set as
    /// environment variables in broker Pods that request
    /// resources discovered in response to this Configuration.
    /// These properties are also propagated in the Instances
    /// that represent the discovered resources.
    #[serde(default)]
    pub broker_properties: HashMap<String, String>,

    /// This defines the number of nodes that can schedule workloads for
    /// any given capability that is found
    #[serde(default = "default_capacity")]
    pub capacity: i32,
}

/// Get Configurations for a given namespace
///
/// Example:
///
/// ```no_run
/// use akri_shared::akri::configuration;
/// use kube::client::Client;
/// use kube::config;
///
/// # #[tokio::main]
/// # async fn main() {
/// let api_client = Client::try_default().await.unwrap();
/// let configs = configuration::get_configurations(&api_client).await.unwrap();
/// # }
/// ```
pub async fn get_configurations(kube_client: &Client) -> Result<ConfigurationList, anyhow::Error> {
    let configurations_client: Api<Configuration> = Api::all(kube_client.clone());
    let lp = ListParams::default();
    match configurations_client.list(&lp).await {
        Ok(configurations_retrieved) => {
            log::trace!("get_configurations return");
            Ok(configurations_retrieved)
        }
        Err(kube::Error::Api(ae)) => {
            log::trace!(
                "get_configurations kube_client.request returned kube error: {:?}",
                ae
            );
            Err(ae.into())
        }
        Err(e) => {
            log::trace!("get_configurations kube_client.request error: {:?}", e);
            Err(e.into())
        }
    }
}

/// Get Configuration for a given name and namespace
///
/// Example:
///
/// ```no_run
/// use akri_shared::akri::configuration;
/// use kube::client::Client;
/// use kube::config;
///
/// # #[tokio::main]
/// # async fn main() {
/// let api_client = Client::try_default().await.unwrap();
/// let config = configuration::find_configuration(
///     "config-1",
///     "default",
///     &api_client).await.unwrap();
/// # }
/// ```
pub async fn find_configuration(
    name: &str,
    namespace: &str,
    kube_client: &Client,
) -> Result<Configuration, anyhow::Error> {
    log::trace!("find_configuration enter");
    let configurations_client: Api<Configuration> = Api::namespaced(kube_client.clone(), namespace);

    log::trace!("find_configuration getting instance with name {}", name);

    match configurations_client.get(name).await {
        Ok(configuration_retrieved) => {
            log::trace!("find_configuration return");
            Ok(configuration_retrieved)
        }
        Err(e) => match e {
            kube::Error::Api(ae) => {
                log::trace!(
                    "find_configuration kube_client.request returned kube error: {:?}",
                    ae
                );
                Err(anyhow::anyhow!(ae))
            }
            _ => {
                log::trace!("find_configuration kube_client.request error: {:?}", e);
                Err(anyhow::anyhow!(e))
            }
        },
    }
}
fn default_capacity() -> i32 {
    1
}

#[cfg(test)]
mod crd_serialization_tests {
    use super::super::super::os::file;
    use super::*;
    use env_logger;

    #[test]
    fn test_config_defaults_with_serialization() {
        let _ = env_logger::builder().is_test(true).try_init();

        if serde_json::from_str::<ConfigurationSpec>(r#"{}"#).is_ok() {
            panic!("discovery handler is required");
        }

        serde_json::from_str::<ConfigurationSpec>(
            r#"{"discoveryHandler":{"name":"random", "discoveryDetails":"serialized details"}}"#,
        )
        .unwrap();
        if serde_json::from_str::<ConfigurationSpec>(r#"{"discoveryHandler":{"name":"random"}}"#)
            .is_err()
        {
            panic!("discovery details are not required");
        }
        if serde_json::from_str::<ConfigurationSpec>(r#"{"discoveryHandler":{}}"#).is_ok() {
            panic!("discovery handler name is required");
        }

        let json = r#"{"discoveryHandler":{"name":"onvif", "discoveryDetails":"{\"onvif\":{}}"}}"#;
        let deserialized: ConfigurationSpec = serde_json::from_str(json).unwrap();
        assert_eq!(default_capacity(), deserialized.capacity);
        assert_eq!(None, deserialized.broker_type);
        assert_eq!(None, deserialized.instance_service_spec);
        assert_eq!(None, deserialized.configuration_service_spec);
        assert_eq!(0, deserialized.broker_properties.len());
    }

    #[test]
    fn test_config_serialization() {
        let _ = env_logger::builder().is_test(true).try_init();
        let pod_spec_json = r#"{"containers": [{"image": "nginx:latest","name": "broker"}]}"#;
        let pod_spec: PodSpec = serde_json::from_str(pod_spec_json).unwrap();
        let json = r#"{"discoveryHandler":{"name":"random", "discoveryDetails":""}, "brokerType":{"pod":{"containers": [{"image": "nginx:latest","name": "broker"}]}}, "capacity":4}"#;
        let deserialized: ConfigurationSpec = serde_json::from_str(json).unwrap();
        assert_eq!(4, deserialized.capacity);
        assert_eq!(None, deserialized.instance_service_spec);
        assert_eq!(None, deserialized.configuration_service_spec);
        assert_eq!(0, deserialized.broker_properties.len());
        if let BrokerType::Pod(d_pod_spec) = deserialized.broker_type.as_ref().unwrap() {
            assert_eq!(&pod_spec, d_pod_spec.as_ref());
        } else {
            panic!("Expected Pod BrokerType");
        }
        let serialized = serde_json::to_string(&deserialized).unwrap();
        let expected_deserialized = r#"{"discoveryHandler":{"name":"random","discoveryDetails":""},"brokerType":{"pod":{"containers":[{"image":"nginx:latest","name":"broker"}]}},"brokerProperties":{},"capacity":4}"#;
        assert_eq!(expected_deserialized, serialized);
    }

    #[test]
    fn test_real_config() {
        let _ = env_logger::builder().is_test(true).try_init();
        let files = [
            "../test/yaml/akri-onvif-video-configuration.yaml",
            "../test/yaml/akri-debug-echo-pod-configuration.yaml",
            "../test/yaml/akri-debug-echo-job-configuration.yaml",
            "../test/yaml/akri-udev-video-configuration.yaml",
            "../test/yaml/akri-opcua-configuration.yaml",
        ];
        for file in &files {
            log::trace!("test file: {}", &file);
            let yaml = file::read_file_to_string(file);
            log::trace!("test file contents: {}", &yaml);
            let deserialized: Configuration = serde_yaml::from_str(&yaml).unwrap();
            log::trace!("test file deserialized: {:?}", &deserialized);
            let reserialized = serde_json::to_string(&deserialized).unwrap();
            log::trace!("test file reserialized: {:?}", &reserialized);
        }
    }

    #[test]
    fn test_expected_full_config() {
        let _ = env_logger::builder().is_test(true).try_init();

        let json = r#"{
            "capacity": 5,
            "discoveryHandler": {
                "name": "random",
                "discoveryDetails": ""
            },
            "instanceServiceSpec": {
                "ports": [
                    {
                        "name": "http",
                        "port": 6052,
                        "protocol": "TCP",
                        "targetPort": 6052
                    }
                ],
                "type": "ClusterIP"
            },
            "brokerType": {
                "pod": {
                    "containers": [
                        {
                            "image": "nginx:latest",
                            "name": "usb-camera-broker",
                            "resources": {
                                "limits": {
                                    "{{PLACEHOLDER}}": "1"
                                }
                            }
                        }
                    ],
                    "imagePullSecrets": [
                        {
                            "name": "regcred"
                        }
                    ]
                }
            },
            "configurationServiceSpec": {
                "ports": [
                    {
                        "name": "http",
                        "port": 6052,
                        "protocol": "TCP",
                        "targetPort": 6052
                    }
                ],
                "type": "ClusterIP"
            },
            "brokerProperties": {
                "resolution-height": "600",
                "resolution-width": "800"
            }
        }"#;

        let deserialized: ConfigurationSpec = serde_json::from_str(json).unwrap();
        assert_eq!(deserialized.discovery_handler.name, "random".to_string());
        assert!(deserialized.discovery_handler.discovery_details.is_empty());
        assert_eq!(5, deserialized.capacity);
        if let BrokerType::Job(_j) = deserialized.broker_type.unwrap() {
            panic!("Expected Pod BrokerType");
        }
        assert_ne!(None, deserialized.instance_service_spec);
        assert_ne!(None, deserialized.configuration_service_spec);
        assert_eq!(2, deserialized.broker_properties.len());
    }
}
