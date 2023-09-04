use super::{
    super::akri::API_NAMESPACE, OwnershipInfo, ERROR_CONFLICT, ERROR_NOT_FOUND,
    NODE_SELECTOR_OP_IN, OBJECT_NAME_FIELD, RESOURCE_REQUIREMENTS_KEY,
};
use either::Either;
use k8s_openapi::api::core::v1::{
    Affinity, NodeAffinity, NodeSelector, NodeSelectorRequirement, NodeSelectorTerm, Pod, PodSpec,
    ResourceRequirements,
};
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{ObjectMeta, OwnerReference};
use kube::{
    api::{Api, DeleteParams, ListParams, ObjectList, PostParams},
    client::Client,
};
use log::{error, info, trace};
use std::collections::BTreeMap;

pub const APP_LABEL_ID: &str = "app";
pub const CONTROLLER_LABEL_ID: &str = "controller";
pub const AKRI_CONFIGURATION_LABEL_NAME: &str = "akri.sh/configuration";
pub const AKRI_INSTANCE_LABEL_NAME: &str = "akri.sh/instance";
pub const AKRI_TARGET_NODE_LABEL_NAME: &str = "akri.sh/target-node";

/// Get Kubernetes Pods with a given label or field selector
///
/// Example:
///
/// ```no_run
/// use akri_shared::k8s::pod;
/// use kube::client::Client;
/// use kube::config;
///
/// # #[tokio::main]
/// # async fn main() {
/// let label_selector = Some("environment=production,app=nginx".to_string());
/// let api_client = Client::try_default().await.unwrap();
/// for pod in pod::find_pods_with_selector(label_selector, None, api_client).await.unwrap() {
///     println!("found pod: {}", pod.metadata.name.unwrap())
/// }
/// # }
/// ```
///
/// ```no_run
/// use akri_shared::k8s::pod;
/// use kube::client::Client;
/// use kube::config;
///
/// # #[tokio::main]
/// # async fn main() {
/// let field_selector = Some("spec.nodeName=node-a".to_string());
/// let api_client = Client::try_default().await.unwrap();
/// for pod in pod::find_pods_with_selector(None, field_selector, api_client).await.unwrap() {
///     println!("found pod: {}", pod.metadata.name.unwrap())
/// }
/// # }
/// ```
pub async fn find_pods_with_selector(
    label_selector: Option<String>,
    field_selector: Option<String>,
    kube_client: Client,
) -> Result<ObjectList<Pod>, anyhow::Error> {
    trace!(
        "find_pods_with_selector with label_selector={:?} field_selector={:?}",
        &label_selector,
        &field_selector
    );
    let pods: Api<Pod> = Api::all(kube_client);
    let pod_list_params = ListParams {
        label_selector,
        field_selector,
        ..Default::default()
    };
    trace!("find_pods_with_selector PRE pods.list(...).await?");
    let result = pods.list(&pod_list_params).await;
    trace!("find_pods_with_selector return");
    Ok(result?)
}

/// Create name for Kubernetes Pod.
///
/// Example:
///
/// ```no_run
/// use akri_shared::k8s::pod;
///
/// let svc_name = pod::create_broker_app_name(
///     "capability_config",
///     Some("node-a"),
///     true,
///     "pod");
/// ```
pub fn create_broker_app_name(
    instance_name: &str,
    node_to_run_broker_on: Option<&str>,
    capability_is_shared: bool,
    app_name_suffix: &str,
) -> String {
    let normalized_instance_name = instance_name.replace('.', "-");
    if capability_is_shared {
        // If the device capability is shared, the instance name will not contain any
        // node-specific content.  To ensure uniqueness of the Pod/Job we are creating,
        // prepend the node name here.
        match node_to_run_broker_on {
            Some(n) => format!("{}-{}-{}", n, normalized_instance_name, app_name_suffix),
            None => format!("{}-{}", normalized_instance_name, app_name_suffix),
        }
    } else {
        // If the device capability is NOT shared, the instance name will contain
        // node-specific content, which guarntees uniqueness.
        format!("{}-{}", normalized_instance_name, app_name_suffix)
    }
}

type ResourceQuantityType = BTreeMap<String, Quantity>;

/// Create Kubernetes Pod based on Device Capabililty Instance & Config.
///
/// Example:
///
/// ```no_run
/// use akri_shared::k8s::{
///     OwnershipInfo,
///     OwnershipType,
///     pod
/// };
/// use kube::client::Client;
/// use kube::config;
/// use k8s_openapi::api::core::v1::PodSpec;
///
/// # #[tokio::main]
/// # async fn main() {
/// let api_client = Client::try_default().await.unwrap();
/// let svc = pod::create_new_pod_from_spec(
///     "pod_namespace",
///     "capability_instance",
///     "capability_config",
///     OwnershipInfo::new(
///         OwnershipType::Instance,
///         "capability_instance".to_string(),
///         "instance_uid".to_string()
///     ),
///     "akri.sh/capability_name",
///     "node-a",
///     true,
///     &PodSpec::default()).unwrap();
/// # }
/// ```
#[allow(clippy::too_many_arguments)]
pub fn create_new_pod_from_spec(
    pod_namespace: &str,
    instance_name: &str,
    configuration_name: &str,
    ownership: OwnershipInfo,
    resource_limit_name: &str,
    node_to_run_pod_on: &str,
    capability_is_shared: bool,
    pod_spec: &PodSpec,
) -> anyhow::Result<Pod> {
    trace!("create_new_pod_from_spec enter");

    let app_name = create_broker_app_name(
        instance_name,
        Some(node_to_run_pod_on),
        capability_is_shared,
        "pod",
    );
    let mut labels: BTreeMap<String, String> = BTreeMap::new();
    labels.insert(APP_LABEL_ID.to_string(), app_name.clone());
    labels.insert(CONTROLLER_LABEL_ID.to_string(), API_NAMESPACE.to_string());
    labels.insert(
        AKRI_CONFIGURATION_LABEL_NAME.to_string(),
        configuration_name.to_string(),
    );
    labels.insert(
        AKRI_INSTANCE_LABEL_NAME.to_string(),
        instance_name.to_string(),
    );
    labels.insert(
        AKRI_TARGET_NODE_LABEL_NAME.to_string(),
        node_to_run_pod_on.to_string(),
    );

    let owner_references: Vec<OwnerReference> = vec![OwnerReference {
        api_version: ownership.get_api_version(),
        kind: ownership.get_kind(),
        controller: ownership.get_controller(),
        block_owner_deletion: ownership.get_block_owner_deletion(),
        name: ownership.get_name(),
        uid: ownership.get_uid(),
    }];

    let mut modified_pod_spec = pod_spec.clone();
    modify_pod_spec(
        &mut modified_pod_spec,
        resource_limit_name,
        Some(node_to_run_pod_on),
    );

    let result = Pod {
        spec: Some(modified_pod_spec),
        metadata: ObjectMeta {
            name: Some(app_name),
            namespace: Some(pod_namespace.to_string()),
            labels: Some(labels),
            owner_references: Some(owner_references),
            ..Default::default()
        },
        ..Default::default()
    };

    trace!("create_new_pod_from_spec return");
    Ok(result)
}

pub fn modify_pod_spec(
    pod_spec: &mut PodSpec,
    resource_limit_name: &str,
    node_to_run_pod_on: Option<&str>,
) {
    let insert_akri_resources = |map: &mut ResourceQuantityType| {
        if map.contains_key(RESOURCE_REQUIREMENTS_KEY) {
            let placeholder_value = map.get(RESOURCE_REQUIREMENTS_KEY).unwrap().clone();
            map.insert(resource_limit_name.to_string(), placeholder_value);
            map.remove(RESOURCE_REQUIREMENTS_KEY);
        }
    };
    for container in pod_spec.containers.iter_mut().chain(
        pod_spec
            .init_containers
            .as_mut()
            .unwrap_or(&mut Vec::default())
            .iter_mut(),
    ) {
        if let Some(resources) = container.resources.as_ref() {
            container.resources = Some(ResourceRequirements {
                limits: {
                    match resources.limits.clone() {
                        Some(mut map) => {
                            insert_akri_resources(&mut map);
                            Some(map)
                        }
                        None => None,
                    }
                },
                requests: {
                    match resources.requests.clone() {
                        Some(mut map) => {
                            insert_akri_resources(&mut map);
                            Some(map)
                        }
                        None => None,
                    }
                },
            });
        };
    }
    if let Some(node_name) = node_to_run_pod_on {
        // Ensure that the modified PodSpec has the required Affinity settings
        pod_spec
            .affinity
            .get_or_insert(Affinity::default())
            .node_affinity
            .get_or_insert(NodeAffinity {
                ..Default::default()
            })
            .required_during_scheduling_ignored_during_execution
            .get_or_insert(NodeSelector {
                node_selector_terms: vec![],
            })
            .node_selector_terms
            .push(NodeSelectorTerm {
                match_fields: Some(vec![NodeSelectorRequirement {
                    key: OBJECT_NAME_FIELD.to_string(),
                    operator: NODE_SELECTOR_OP_IN.to_string(), // need to find if there is an equivalent to: v1.NODE_SELECTOR_OP_IN,
                    values: Some(vec![node_name.to_string()]),
                }]),
                ..Default::default()
            });
    }
}

#[cfg(test)]
mod broker_podspec_tests {
    use super::super::super::akri::API_VERSION;
    use super::super::OwnershipType;
    use super::*;
    use env_logger;
    use k8s_openapi::api::core::v1::Container;

    #[test]
    fn test_create_broker_app_name() {
        let _ = env_logger::builder().is_test(true).try_init();

        assert_eq!(
            "node-instance-name-suffix",
            create_broker_app_name("instance.name", Some("node"), true, "suffix")
        );
        assert_eq!(
            "instance-name-suffix",
            create_broker_app_name("instance.name", Some("node"), false, "suffix")
        );

        assert_eq!(
            "node-instance-name-suffix",
            create_broker_app_name("instance-name", Some("node"), true, "suffix")
        );
        assert_eq!(
            "instance-name-suffix",
            create_broker_app_name("instance-name", Some("node"), false, "suffix")
        );

        assert_eq!(
            "node-1-0-0-1-suffix",
            create_broker_app_name("1-0-0-1", Some("node"), true, "suffix")
        );
        assert_eq!(
            "1-0-0-1-suffix",
            create_broker_app_name("1-0-0-1", Some("node"), false, "suffix")
        );
    }

    #[test]
    fn test_create_broker_app_name_job() {
        let _ = env_logger::builder().is_test(true).try_init();

        assert_eq!(
            "node-instance-name-1-job",
            create_broker_app_name("instance.name", Some("node"), true, "1-job")
        );
    }

    #[test]
    fn test_pod_spec_creation() {
        let image = "image".to_string();
        let mut placeholder_limits: ResourceQuantityType = BTreeMap::new();
        placeholder_limits.insert(RESOURCE_REQUIREMENTS_KEY.to_string(), Default::default());
        placeholder_limits.insert("do-not-change-this".to_string(), Default::default());
        let placeholder_requests = placeholder_limits.clone();
        do_pod_spec_creation_test(
            vec![image.clone()],
            vec![Container {
                image: Some(image),
                resources: Some(ResourceRequirements {
                    limits: Some(placeholder_limits),
                    requests: Some(placeholder_requests),
                }),
                ..Default::default()
            }],
            None,
        );
    }

    #[test]
    fn test_pod_spec_creation_with_multiple_containers() {
        let mut placeholder_limits1: ResourceQuantityType = BTreeMap::new();
        placeholder_limits1.insert(RESOURCE_REQUIREMENTS_KEY.to_string(), Default::default());
        placeholder_limits1.insert("do-not-change-this".to_string(), Default::default());
        let placeholder_requests1 = placeholder_limits1.clone();
        let mut placeholder_limits2: ResourceQuantityType = BTreeMap::new();
        placeholder_limits2.insert(RESOURCE_REQUIREMENTS_KEY.to_string(), Default::default());
        placeholder_limits2.insert("do-not-change-this".to_string(), Default::default());
        let placeholder_requests2 = placeholder_limits2.clone();
        do_pod_spec_creation_test(
            vec!["image1".to_string(), "image2".to_string()],
            vec![
                Container {
                    image: Some("image1".to_string()),
                    resources: Some(ResourceRequirements {
                        limits: Some(placeholder_limits1),
                        requests: Some(placeholder_requests1),
                    }),
                    ..Default::default()
                },
                Container {
                    image: Some("image2".to_string()),
                    resources: Some(ResourceRequirements {
                        limits: Some(placeholder_limits2),
                        requests: Some(placeholder_requests2),
                    }),
                    ..Default::default()
                },
            ],
            None,
        );
    }

    #[test]
    fn test_pod_spec_creation_with_init_containers() {
        let mut placeholder_limits: ResourceQuantityType = BTreeMap::new();
        placeholder_limits.insert(RESOURCE_REQUIREMENTS_KEY.to_string(), Default::default());
        placeholder_limits.insert("do-not-change-this".to_string(), Default::default());
        do_pod_spec_creation_test(
            vec![
                "image1".to_string(),
                "image2".to_string(),
                "image3".to_string(),
            ],
            vec![
                Container {
                    image: Some("image1".to_string()),
                    resources: Some(ResourceRequirements {
                        limits: Some(placeholder_limits.clone()),
                        requests: Some(placeholder_limits.clone()),
                    }),
                    ..Default::default()
                },
                Container {
                    image: Some("image2".to_string()),
                    resources: Some(ResourceRequirements {
                        limits: Some(placeholder_limits.clone()),
                        requests: Some(placeholder_limits.clone()),
                    }),
                    ..Default::default()
                },
            ],
            Some(vec![Container {
                image: Some("image3".to_string()),
                resources: Some(ResourceRequirements {
                    limits: Some(placeholder_limits.clone()),
                    requests: Some(placeholder_limits.clone()),
                }),
                ..Default::default()
            }]),
        );
    }

    fn do_pod_spec_creation_test(
        image_names: Vec<String>,
        container_specs: Vec<Container>,
        init_containers_specs: Option<Vec<Container>>,
    ) {
        let _ = env_logger::builder().is_test(true).try_init();

        let num_containers = container_specs.len();
        let num_init_containers = init_containers_specs
            .as_ref()
            .unwrap_or(&Vec::default())
            .len();
        let pod_spec = PodSpec {
            containers: container_specs,
            init_containers: init_containers_specs,
            affinity: Some(Affinity {
                node_affinity: Some(NodeAffinity {
                    required_during_scheduling_ignored_during_execution: Some(NodeSelector {
                        node_selector_terms: vec![NodeSelectorTerm {
                            match_fields: Some(vec![NodeSelectorRequirement {
                                key: "do-not-change-this".to_string(),
                                operator: NODE_SELECTOR_OP_IN.to_string(), // need to find if there is an equivalent to: v1.NODE_SELECTOR_OP_IN,
                                values: Some(vec!["existing-node-affinity".to_string()]),
                            }]),
                            ..Default::default()
                        }],
                        //..Default::default()
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        let pod_namespace = "pod_namespace".to_string();
        let instance_name = "instance_name".to_string();
        let instance_uid = "instance_uid".to_string();
        let configuration_name = "configuration_name".to_string();
        let resource_limit_name = "resource_limit_name".to_string();
        let node_to_run_pod_on = "node_to_run_pod_on".to_string();

        for capability_is_shared in &[true, false] {
            let pod = create_new_pod_from_spec(
                &pod_namespace,
                &instance_name,
                &configuration_name,
                OwnershipInfo::new(
                    OwnershipType::Instance,
                    instance_name.clone(),
                    instance_uid.clone(),
                ),
                &resource_limit_name,
                &node_to_run_pod_on,
                *capability_is_shared,
                &pod_spec,
            )
            .unwrap();

            let app_name = create_broker_app_name(
                &instance_name,
                Some(&node_to_run_pod_on),
                *capability_is_shared,
                "pod",
            );

            // Validate the metadata name/namesapce
            assert_eq!(&app_name, &pod.metadata.clone().name.unwrap());
            assert_eq!(&pod_namespace, &pod.metadata.clone().namespace.unwrap());

            // Validate the labels added
            assert_eq!(
                &&app_name,
                &pod.metadata
                    .clone()
                    .labels
                    .unwrap()
                    .get(APP_LABEL_ID)
                    .unwrap()
            );
            assert_eq!(
                &&API_NAMESPACE.to_string(),
                &pod.metadata
                    .clone()
                    .labels
                    .unwrap()
                    .get(CONTROLLER_LABEL_ID)
                    .unwrap()
            );
            assert_eq!(
                &&configuration_name,
                &pod.metadata
                    .clone()
                    .labels
                    .unwrap()
                    .get(AKRI_CONFIGURATION_LABEL_NAME)
                    .unwrap()
            );
            assert_eq!(
                &&instance_name,
                &pod.metadata
                    .clone()
                    .labels
                    .unwrap()
                    .get(AKRI_INSTANCE_LABEL_NAME)
                    .unwrap()
            );
            assert_eq!(
                &&node_to_run_pod_on,
                &pod.metadata
                    .clone()
                    .labels
                    .unwrap()
                    .get(AKRI_TARGET_NODE_LABEL_NAME)
                    .unwrap()
            );

            // Validate ownerReference
            assert_eq!(
                instance_name,
                pod.metadata
                    .clone()
                    .owner_references
                    .unwrap()
                    .get(0)
                    .unwrap()
                    .name
            );
            assert_eq!(
                instance_uid,
                pod.metadata
                    .clone()
                    .owner_references
                    .unwrap()
                    .get(0)
                    .unwrap()
                    .uid
            );
            assert_eq!(
                "Instance",
                &pod.metadata
                    .clone()
                    .owner_references
                    .unwrap()
                    .get(0)
                    .unwrap()
                    .kind
            );
            assert_eq!(
                &format!("{}/{}", API_NAMESPACE, API_VERSION),
                &pod.metadata
                    .clone()
                    .owner_references
                    .unwrap()
                    .get(0)
                    .unwrap()
                    .api_version
            );
            assert!(pod
                .metadata
                .clone()
                .owner_references
                .unwrap()
                .get(0)
                .unwrap()
                .controller
                .unwrap());
            assert!(pod
                .metadata
                .clone()
                .owner_references
                .unwrap()
                .get(0)
                .unwrap()
                .block_owner_deletion
                .unwrap());

            // Validate existing and new affinity exist
            assert_eq!(
                &2,
                &pod.spec
                    .clone()
                    .unwrap()
                    .affinity
                    .unwrap()
                    .node_affinity
                    .unwrap()
                    .required_during_scheduling_ignored_during_execution
                    .unwrap()
                    .node_selector_terms
                    .len()
            );

            // Validate existing affinity unchanged
            assert_eq!(
                "do-not-change-this",
                &pod.spec
                    .clone()
                    .unwrap()
                    .affinity
                    .unwrap()
                    .node_affinity
                    .unwrap()
                    .required_during_scheduling_ignored_during_execution
                    .unwrap()
                    .node_selector_terms
                    .get(0)
                    .unwrap()
                    .match_fields
                    .as_ref()
                    .unwrap()
                    .get(0)
                    .unwrap()
                    .key
            );
            assert_eq!(
                "In",
                &pod.spec
                    .clone()
                    .unwrap()
                    .affinity
                    .unwrap()
                    .node_affinity
                    .unwrap()
                    .required_during_scheduling_ignored_during_execution
                    .unwrap()
                    .node_selector_terms
                    .get(0)
                    .unwrap()
                    .match_fields
                    .as_ref()
                    .unwrap()
                    .get(0)
                    .unwrap()
                    .operator
            );
            assert_eq!(
                &&vec!["existing-node-affinity".to_string()],
                &pod.spec
                    .clone()
                    .unwrap()
                    .affinity
                    .unwrap()
                    .node_affinity
                    .unwrap()
                    .required_during_scheduling_ignored_during_execution
                    .unwrap()
                    .node_selector_terms
                    .get(0)
                    .unwrap()
                    .match_fields
                    .as_ref()
                    .unwrap()
                    .get(0)
                    .unwrap()
                    .values
                    .as_ref()
                    .unwrap()
            );

            // Validate the affinity added
            assert_eq!(
                "metadata.name",
                &pod.spec
                    .clone()
                    .unwrap()
                    .affinity
                    .unwrap()
                    .node_affinity
                    .unwrap()
                    .required_during_scheduling_ignored_during_execution
                    .unwrap()
                    .node_selector_terms
                    .get(1)
                    .unwrap()
                    .match_fields
                    .as_ref()
                    .unwrap()
                    .get(0)
                    .unwrap()
                    .key
            );
            assert_eq!(
                "In",
                &pod.spec
                    .clone()
                    .unwrap()
                    .affinity
                    .unwrap()
                    .node_affinity
                    .unwrap()
                    .required_during_scheduling_ignored_during_execution
                    .unwrap()
                    .node_selector_terms
                    .get(1)
                    .unwrap()
                    .match_fields
                    .as_ref()
                    .unwrap()
                    .get(0)
                    .unwrap()
                    .operator
            );
            assert_eq!(
                &&vec![node_to_run_pod_on.clone()],
                &pod.spec
                    .clone()
                    .unwrap()
                    .affinity
                    .unwrap()
                    .node_affinity
                    .unwrap()
                    .required_during_scheduling_ignored_during_execution
                    .unwrap()
                    .node_selector_terms
                    .get(1)
                    .unwrap()
                    .match_fields
                    .as_ref()
                    .unwrap()
                    .get(0)
                    .unwrap()
                    .values
                    .as_ref()
                    .unwrap()
            );

            // Validate image name remanes unchanged
            for i in 0..num_containers {
                assert_eq!(
                    &image_names.get(i).unwrap(),
                    &pod.spec
                        .clone()
                        .unwrap()
                        .containers
                        .get(i)
                        .unwrap()
                        .image
                        .as_ref()
                        .unwrap()
                );

                // Validate existing limits/requires unchanged
                assert_eq!(
                    &true,
                    &pod.spec
                        .clone()
                        .unwrap()
                        .containers
                        .get(i)
                        .unwrap()
                        .resources
                        .as_ref()
                        .unwrap()
                        .limits
                        .as_ref()
                        .unwrap()
                        .contains_key("do-not-change-this")
                );
                assert_eq!(
                    &true,
                    &pod.spec
                        .clone()
                        .unwrap()
                        .containers
                        .get(i)
                        .unwrap()
                        .resources
                        .as_ref()
                        .unwrap()
                        .requests
                        .as_ref()
                        .unwrap()
                        .contains_key("do-not-change-this")
                );
                // Validate the limits/requires added
                assert_eq!(
                    &false,
                    &pod.spec
                        .clone()
                        .unwrap()
                        .containers
                        .get(i)
                        .unwrap()
                        .resources
                        .as_ref()
                        .unwrap()
                        .limits
                        .as_ref()
                        .unwrap()
                        .contains_key(RESOURCE_REQUIREMENTS_KEY)
                );
                assert_eq!(
                    &false,
                    &pod.spec
                        .clone()
                        .unwrap()
                        .containers
                        .get(i)
                        .unwrap()
                        .resources
                        .as_ref()
                        .unwrap()
                        .requests
                        .as_ref()
                        .unwrap()
                        .contains_key(RESOURCE_REQUIREMENTS_KEY)
                );
                assert_eq!(
                    &true,
                    &pod.spec
                        .clone()
                        .unwrap()
                        .containers
                        .get(i)
                        .unwrap()
                        .resources
                        .as_ref()
                        .unwrap()
                        .limits
                        .as_ref()
                        .unwrap()
                        .contains_key(&resource_limit_name.clone())
                );
                assert_eq!(
                    &true,
                    &pod.spec
                        .clone()
                        .unwrap()
                        .containers
                        .get(i)
                        .unwrap()
                        .resources
                        .as_ref()
                        .unwrap()
                        .requests
                        .as_ref()
                        .unwrap()
                        .contains_key(&resource_limit_name.clone())
                );
            }

            for i in 0..num_init_containers {
                assert_eq!(
                    &image_names.get(num_containers + i).unwrap(),
                    &pod.spec
                        .clone()
                        .unwrap()
                        .init_containers
                        .unwrap()
                        .get(i)
                        .unwrap()
                        .image
                        .as_ref()
                        .unwrap()
                );

                // Validate existing limits/requires unchanged
                assert_eq!(
                    &true,
                    &pod.spec
                        .clone()
                        .unwrap()
                        .init_containers
                        .unwrap()
                        .get(i)
                        .unwrap()
                        .resources
                        .as_ref()
                        .unwrap()
                        .limits
                        .as_ref()
                        .unwrap()
                        .contains_key("do-not-change-this")
                );
                assert_eq!(
                    &true,
                    &pod.spec
                        .clone()
                        .unwrap()
                        .init_containers
                        .unwrap()
                        .get(i)
                        .unwrap()
                        .resources
                        .as_ref()
                        .unwrap()
                        .requests
                        .as_ref()
                        .unwrap()
                        .contains_key("do-not-change-this")
                );
                // Validate the limits/requires added
                assert_eq!(
                    &false,
                    &pod.spec
                        .clone()
                        .unwrap()
                        .init_containers
                        .unwrap()
                        .get(i)
                        .unwrap()
                        .resources
                        .as_ref()
                        .unwrap()
                        .limits
                        .as_ref()
                        .unwrap()
                        .contains_key(RESOURCE_REQUIREMENTS_KEY)
                );
                assert_eq!(
                    &false,
                    &pod.spec
                        .clone()
                        .unwrap()
                        .init_containers
                        .unwrap()
                        .get(i)
                        .unwrap()
                        .resources
                        .as_ref()
                        .unwrap()
                        .requests
                        .as_ref()
                        .unwrap()
                        .contains_key(RESOURCE_REQUIREMENTS_KEY)
                );
                assert_eq!(
                    &true,
                    &pod.spec
                        .clone()
                        .unwrap()
                        .init_containers
                        .unwrap()
                        .get(i)
                        .unwrap()
                        .resources
                        .as_ref()
                        .unwrap()
                        .limits
                        .as_ref()
                        .unwrap()
                        .contains_key(&resource_limit_name.clone())
                );
                assert_eq!(
                    &true,
                    &pod.spec
                        .clone()
                        .unwrap()
                        .init_containers
                        .unwrap()
                        .get(i)
                        .unwrap()
                        .resources
                        .as_ref()
                        .unwrap()
                        .requests
                        .as_ref()
                        .unwrap()
                        .contains_key(&resource_limit_name.clone())
                );
            }
        }
    }
}

/// Create Kubernetes Pod
///
/// Example:
///
/// ```no_run
/// use akri_shared::k8s::pod;
/// use kube::client::Client;
/// use kube::config;
/// use k8s_openapi::api::core::v1::Pod;
///
/// # #[tokio::main]
/// # async fn main() {
/// let api_client = Client::try_default().await.unwrap();
/// pod::create_pod(&Pod::default(), "pod_namespace", api_client).await.unwrap();
/// # }
/// ```
pub async fn create_pod(
    pod_to_create: &Pod,
    namespace: &str,
    kube_client: Client,
) -> Result<(), anyhow::Error> {
    trace!("create_pod enter");
    let pods: Api<Pod> = Api::namespaced(kube_client, namespace);
    info!("create_pod pods.create(...).await?:");
    match pods.create(&PostParams::default(), pod_to_create).await {
        Ok(created_pod) => {
            info!(
                "create_pod pods.create return: {:?}",
                created_pod.metadata.name
            );
            Ok(())
        }
        Err(kube::Error::Api(ae)) => {
            if ae.code == ERROR_CONFLICT {
                trace!("create_pod - pod already exists");
                Ok(())
            } else {
                error!(
                    "create_pod pods.create [{:?}] returned kube error: {:?}",
                    serde_json::to_string(&pod_to_create),
                    ae
                );
                Err(anyhow::anyhow!(ae))
            }
        }
        Err(e) => {
            error!(
                "create_pod pods.create [{:?}] error: {:?}",
                serde_json::to_string(&pod_to_create),
                e
            );
            Err(anyhow::anyhow!(e))
        }
    }
}

/// Remove Kubernetes Pod
///
/// Example:
///
/// ```no_run
/// use akri_shared::k8s::pod;
/// use kube::client::Client;
/// use kube::config;
///
/// # #[tokio::main]
/// # async fn main() {
/// let api_client = Client::try_default().await.unwrap();
/// pod::remove_pod("pod_to_remove", "pod_namespace", api_client).await.unwrap();
/// # }
/// ```
pub async fn remove_pod(
    pod_to_remove: &str,
    namespace: &str,
    kube_client: Client,
) -> Result<(), anyhow::Error> {
    trace!("remove_pod enter");
    let pods: Api<Pod> = Api::namespaced(kube_client, namespace);
    info!("remove_pod pods.delete(...).await?:");
    match pods.delete(pod_to_remove, &DeleteParams::default()).await {
        Ok(deleted_pod) => match deleted_pod {
            Either::Left(spec) => {
                info!("remove_pod pods.delete return: {:?}", &spec.metadata.name);
                Ok(())
            }
            Either::Right(status) => {
                info!("remove_pod pods.delete return: {:?}", &status.status);
                Ok(())
            }
        },
        Err(kube::Error::Api(ae)) => {
            if ae.code == ERROR_NOT_FOUND {
                trace!("remove_pod - pod already removed");
                Ok(())
            } else {
                error!(
                    "remove_pod pods.delete [{:?}] returned kube error: {:?}",
                    &pod_to_remove, ae
                );
                Err(anyhow::anyhow!(ae))
            }
        }
        Err(e) => {
            error!(
                "remove_pod pods.delete [{:?}] error: {:?}",
                &pod_to_remove, e
            );
            Err(anyhow::anyhow!(e))
        }
    }
}
