//
// Use this to allow ProtocolHandler enum
// to use case similar to other properties (specifically avoiding CamelCase,
// in favor of camelCase)
//
#![allow(non_camel_case_types)]

use super::API_CONFIGURATIONS;
use super::API_NAMESPACE;
use super::API_VERSION;
use k8s_openapi::api::core::v1::PodSpec;
use k8s_openapi::api::core::v1::ServiceSpec;
use kube::{
    api::{ListParams, Object, ObjectList, RawApi, Void},
    client::APIClient,
};
use std::collections::HashMap;

pub type KubeAkriConfig = Object<Configuration, Void>;
pub type KubeAkriConfigList = ObjectList<Object<Configuration, Void>>;

/// This defines the supported types of protocols
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub enum ProtocolHandler {
    nessie(NessieDiscoveryHandlerConfig),
    onvif(OnvifDiscoveryHandlerConfig),
    udev(UdevDiscoveryHandlerConfig),
    opcua(OpcuaDiscoveryHandlerConfig),
    debugEcho(DebugEchoDiscoveryHandlerConfig),
}

/// This defines the types of supported filters
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum FilterType {
    /// If the filter type is Exclude, any items NOT found in the
    /// list are accepted
    Exclude,
    /// If the filter type is Include, only items found in the
    /// list are accepted
    Include,
}

/// The default filter type is `Include`
fn default_action() -> FilterType {
    FilterType::Include
}

/// This defines a filter list.
///
/// The items list can either define the only acceptable
/// items (Include) or can define the only unacceptable items
/// (Exclude)
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FilterList {
    /// This defines a list of items that will be evaluated as part
    /// of the filtering process
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<String>,
    /// This defines what the evaluation of items will be.  The default
    /// is `Include`
    #[serde(default = "default_action")]
    pub action: FilterType,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct NessieDiscoveryHandlerConfig {
    pub nessie_url: String,
}

/// This defines the ONVIF data stored in the Configuration
/// CRD
///
/// The ONVIF discovery handler is structured to store a filter list for
/// ip addresses, mac addresses, and ONVIF scopes.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct OnvifDiscoveryHandlerConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ip_addresses: Option<FilterList>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mac_addresses: Option<FilterList>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scopes: Option<FilterList>,
    #[serde(default = "default_discovery_timeout_seconds")]
    pub discovery_timeout_seconds: i32,
}

fn default_discovery_timeout_seconds() -> i32 {
    1
}

/// This defines the UDEV data stored in the Configuration
/// CRD
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UdevDiscoveryHandlerConfig {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub udev_rules: Vec<String>,
}

/// This defines the OPCUA data stored in the Configuration
/// CRD
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct OpcuaDiscoveryHandlerConfig {}

/// This defines the DebugEcho data stored in the Configuration
/// CRD
///
/// DebugEcho is used for testing Akri.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DebugEchoDiscoveryHandlerConfig {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub descriptions: Vec<String>,
    pub shared: bool,
}

/// Defines the information in the Akri Configuration CRD
///
/// A Configuration is the primary method for users to describe anticipated
/// capabilities.  For any specific capability found that is described by this
/// configuration, an Instance
/// is created.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Configuration {
    /// This defines the capability protocol
    pub protocol: ProtocolHandler,

    /// This defines the number of nodes that can schedule worloads for
    /// any given capability that is found
    #[serde(default = "default_capacity")]
    pub capacity: i32,
    /// This defines the units that the capacity is measured by
    #[serde(default = "default_units")]
    pub units: String,

    /// This defines a workload that should be scheduled to any
    /// node that can access any capability described by this
    /// configuration
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub broker_pod_spec: Option<PodSpec>,

    /// This defines a service that should be created to access
    /// any specific capability found that is described by this
    /// configuration. For each Configuration, several Instances
    /// can be found.  For each Instance, there is at most 1
    /// instance service.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance_service_spec: Option<ServiceSpec>,

    /// This defines a service that should be created to access
    /// all of the capabilities found that are described by this
    /// configuration. For each Configurataion, there is at most
    /// 1 device capability service.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub configuration_service_spec: Option<ServiceSpec>,

    /// This defines some properties that will be propogated to
    /// any Instance
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub properties: HashMap<String, String>,
}

/// Get Configurations for a given namespace
///
/// Example:
///
/// ```no_run
/// use akri_shared::akri::configuration;
/// use kube::client::APIClient;
/// use kube::config;
///
/// # #[tokio::main]
/// # async fn main() {
/// let api_client = APIClient::new(config::incluster_config().unwrap());
/// let dccs = configuration::get_configurations(&api_client).await.unwrap();
/// # }
/// ```
pub async fn get_configurations(
    kube_client: &APIClient,
) -> Result<KubeAkriConfigList, Box<dyn std::error::Error + Send + Sync + 'static>> {
    log::trace!("get_configurations enter");
    let akri_config_type = RawApi::customResource(API_CONFIGURATIONS)
        .group(API_NAMESPACE)
        .version(API_VERSION);

    log::trace!("get_configurations kube_client.request::<KubeAkriInstanceList>(akri_config_type.list(...)?).await?");

    let dcc_list_params = ListParams {
        ..Default::default()
    };
    match kube_client
        .request::<KubeAkriConfigList>(akri_config_type.list(&dcc_list_params)?)
        .await
    {
        Ok(configs_retrieved) => {
            log::trace!("get_configurations return");
            Ok(configs_retrieved)
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
/// use kube::client::APIClient;
/// use kube::config;
///
/// # #[tokio::main]
/// # async fn main() {
/// let api_client = APIClient::new(config::incluster_config().unwrap());
/// let dcc = configuration::find_configuration(
///     "dcc-1",
///     "default",
///     &api_client).await.unwrap();
/// # }
/// ```
pub async fn find_configuration(
    name: &str,
    namespace: &str,
    kube_client: &APIClient,
) -> Result<KubeAkriConfig, Box<dyn std::error::Error + Send + Sync + 'static>> {
    log::trace!("find_configuration enter");
    let akri_config_type = RawApi::customResource(API_CONFIGURATIONS)
        .group(API_NAMESPACE)
        .version(API_VERSION)
        .within(&namespace);

    log::trace!("find_configuration kube_client.request::<KubeAkriConfig>(akri_config_type.get(...)?).await?");

    match kube_client
        .request::<KubeAkriConfig>(akri_config_type.get(&name)?)
        .await
    {
        Ok(config_retrieved) => {
            log::trace!("find_configuration return");
            Ok(config_retrieved)
        }
        Err(kube::Error::Api(ae)) => {
            log::trace!(
                "find_configuration kube_client.request returned kube error: {:?}",
                ae
            );
            Err(ae.into())
        }
        Err(e) => {
            log::trace!("find_configuration kube_client.request error: {:?}", e);
            Err(e.into())
        }
    }
}

fn default_capacity() -> i32 {
    1
}
fn default_units() -> String {
    "pod".to_string()
}

#[cfg(test)]
mod crd_serializeation_tests {
    use super::super::super::os::file;
    use super::*;
    use env_logger;

    #[derive(Serialize, Deserialize, Clone, Debug)]
    #[serde(rename_all = "camelCase")]
    struct ConfigurationCRD {
        api_version: String,
        kind: String,
        metadata: HashMap<String, String>,
        spec: Configuration,
    }

    #[test]
    fn test_config_defaults_with_serialization() {
        let _ = env_logger::builder().is_test(true).try_init();

        {
            if serde_json::from_str::<Configuration>(r#"{}"#).is_ok() {
                panic!("protocol is required");
            }
            if serde_json::from_str::<Configuration>(r#"{"protocol":{"onvif":{}}}"#).is_err() {
                panic!("onvif protocol doesn't require anything");
            }
            if serde_json::from_str::<Configuration>(r#"{"protocol":{"udev":{}}}"#).is_ok() {
                panic!("udev protocol requires udevRules");
            }
        }

        let json = r#"{"protocol":{"onvif":{}}}"#;
        let deserialized: Configuration = serde_json::from_str(json).unwrap();
        match deserialized.protocol {
            ProtocolHandler::onvif(_) => {}
            _ => panic!("protocol should be Onvif"),
        }
        assert_eq!(default_capacity(), deserialized.capacity);
        assert_eq!(default_units(), deserialized.units);
        assert_eq!(None, deserialized.broker_pod_spec);
        assert_eq!(None, deserialized.instance_service_spec);
        assert_eq!(None, deserialized.configuration_service_spec);
        assert_eq!(0, deserialized.properties.len());

        let serialized = serde_json::to_string(&deserialized).unwrap();
        let expected_deserialized =
            r#"{"protocol":{"onvif":{"discoveryTimeoutSeconds":1}},"capacity":1,"units":"pod"}"#;
        assert_eq!(expected_deserialized, serialized);
    }

    #[test]
    fn test_config_serialization() {
        let _ = env_logger::builder().is_test(true).try_init();

        let json = r#"{"protocol":{"onvif":{"discoveryTimeoutSeconds":5}}, "capacity":4, "units":"slaphappies"}"#;
        let deserialized: Configuration = serde_json::from_str(json).unwrap();
        match &deserialized.protocol {
            ProtocolHandler::onvif(discovery_handler_config) => {
                assert_eq!(discovery_handler_config.discovery_timeout_seconds, 5);
            }
            _ => panic!("protocol should be Onvif"),
        }
        assert_eq!(4, deserialized.capacity);
        assert_eq!("slaphappies".to_string(), deserialized.units);
        assert_eq!(None, deserialized.broker_pod_spec);
        assert_eq!(None, deserialized.instance_service_spec);
        assert_eq!(None, deserialized.configuration_service_spec);
        assert_eq!(0, deserialized.properties.len());

        let serialized = serde_json::to_string(&deserialized).unwrap();
        let expected_deserialized = r#"{"protocol":{"onvif":{"discoveryTimeoutSeconds":5}},"capacity":4,"units":"slaphappies"}"#;
        assert_eq!(expected_deserialized, serialized);
    }

    #[test]
    fn test_real_config() {
        let _ = env_logger::builder().is_test(true).try_init();

        let files = [
            "../test/yaml/akri-onvif-video.yaml",
            "../test/yaml/akri-debug-echo-foo.yaml",
            "../test/yaml/akri-udev-video.yaml",
        ];
        for file in &files {
            log::trace!("test file: {}", &file);
            let yaml = file::read_file_to_string(&file);
            log::trace!("test file contents: {}", &yaml);
            let deserialized: ConfigurationCRD = serde_yaml::from_str(&yaml).unwrap();
            log::trace!("test file deserialized: {:?}", &deserialized);
            let reserialized = serde_json::to_string(&deserialized).unwrap();
            log::trace!("test file reserialized: {:?}", &reserialized);
        }
    }

    #[test]
    fn test_expected_full_config() {
        let _ = env_logger::builder().is_test(true).try_init();

        let json = r#"{
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
                "brokerPodSpec": {
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
                },
                "capacity": 5,
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
                "protocol": {
                    "udev": {
                        "udevRules":[]
                    }
                },
                "properties": {
                    "resolution-height": "600",
                    "resolution-width": "800"
                },
                "units": "cameras"
            }
        "#;
        let deserialized: Configuration = serde_json::from_str(json).unwrap();
        match deserialized.protocol {
            ProtocolHandler::udev(_) => {}
            _ => panic!("protocol as !Udev should be error"),
        }
        assert_eq!(5, deserialized.capacity);
        assert_eq!("cameras".to_string(), deserialized.units);
        assert_ne!(None, deserialized.broker_pod_spec);
        assert_ne!(None, deserialized.instance_service_spec);
        assert_ne!(None, deserialized.configuration_service_spec);
        assert_eq!(2, deserialized.properties.len());
    }
}
