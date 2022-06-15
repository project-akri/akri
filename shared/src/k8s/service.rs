use super::{
    super::akri::API_NAMESPACE,
    pod::{
        AKRI_CONFIGURATION_LABEL_NAME, AKRI_INSTANCE_LABEL_NAME, APP_LABEL_ID, CONTROLLER_LABEL_ID,
    },
    OwnershipInfo, ERROR_NOT_FOUND,
};
use either::Either;
use k8s_openapi::api::core::v1::{Service, ServiceSpec};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{ObjectMeta, OwnerReference};
use kube::{
    api::{Api, DeleteParams, ListParams, ObjectList, Patch, PatchParams, PostParams},
    client::Client,
};
use log::{error, info, trace};
use std::collections::BTreeMap;

/// Get Kubernetes Services with a given selector
///
/// Example:
///
/// ```no_run
/// use akri_shared::k8s::service;
/// use kube::client::Client;
/// use kube::config;
///
/// # #[tokio::main]
/// # async fn main() {
/// let selector = "environment=production,app=nginx";
/// let api_client = Client::try_default().await.unwrap();
/// for svc in service::find_services_with_selector(&selector, api_client).await.unwrap() {
///     println!("found svc: {}", svc.metadata.name.unwrap())
/// }
/// # }
/// ```
pub async fn find_services_with_selector(
    selector: &str,
    kube_client: Client,
) -> Result<ObjectList<Service>, anyhow::Error> {
    trace!("find_services_with_selector with selector={:?}", &selector);
    let svc_client: Api<Service> = Api::all(kube_client);
    let svc_list_params = ListParams {
        label_selector: Some(selector.to_string()),
        ..Default::default()
    };
    trace!("find_services_with_selector PRE svcs.list(...).await?");
    let result = svc_client.list(&svc_list_params).await;
    trace!("find_services_with_selector return");
    Ok(result?)
}

/// Create name for Kubernetes Service.
///
/// Example:
///
/// ```no_run
/// use akri_shared::k8s::service;
///
/// let svc_name = service::create_service_app_name(
///     "capability_config",
///     "capability_instance",
///     "svc",
///     true);
/// ```
pub fn create_service_app_name(
    configuration_name: &str,
    instance_name: &str,
    svc_suffix: &str,
    node_specific_svc: bool,
) -> String {
    let normalized_instance_name = instance_name.replace('.', "-");
    if node_specific_svc {
        // If this is the node specific service, use the insrtance name which
        // contains node-specific content.
        format!("{}-{}", normalized_instance_name, svc_suffix)
    } else {
        // If this is NOT the node specific service, use the capability name.
        format!("{}-{}", configuration_name, svc_suffix)
    }
}

/// Create Kubernetes Service based on Device Capabililty Instance & Config.
///
/// Example:
///
/// ```no_run
/// use akri_shared::k8s::{
///     OwnershipInfo,
///     OwnershipType,
///     service
/// };
/// use kube::client::Client;
/// use kube::config;
/// use k8s_openapi::api::core::v1::ServiceSpec;
///
/// # #[tokio::main]
/// # async fn main() {
/// let api_client = Client::try_default().await.unwrap();
/// let svc = service::create_new_service_from_spec(
///     "svc_namespace",
///     "capability_instance",
///     "capability_config",
///     OwnershipInfo::new(
///         OwnershipType::Instance,
///         "capability_instance".to_string(),
///         "instance_uid".to_string()
///     ),
///     &ServiceSpec::default(),
///     true).unwrap();
/// # }
/// ```
pub fn create_new_service_from_spec(
    svc_namespace: &str,
    instance_name: &str,
    configuration_name: &str,
    ownership: OwnershipInfo,
    svc_spec: &ServiceSpec,
    node_specific_svc: bool,
) -> anyhow::Result<Service> {
    let app_name =
        create_service_app_name(configuration_name, instance_name, "svc", node_specific_svc);
    let mut labels: BTreeMap<String, String> = BTreeMap::new();
    labels.insert(APP_LABEL_ID.to_string(), app_name.clone());
    labels.insert(CONTROLLER_LABEL_ID.to_string(), API_NAMESPACE.to_string());
    if node_specific_svc {
        labels.insert(
            AKRI_INSTANCE_LABEL_NAME.to_string(),
            instance_name.to_string(),
        );
    } else {
        labels.insert(
            AKRI_CONFIGURATION_LABEL_NAME.to_string(),
            configuration_name.to_string(),
        );
    }

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
    if node_specific_svc {
        modified_selector.insert(
            AKRI_INSTANCE_LABEL_NAME.to_string(),
            instance_name.to_string(),
        );
    } else {
        modified_selector.insert(
            AKRI_CONFIGURATION_LABEL_NAME.to_string(),
            configuration_name.to_string(),
        );
    }
    spec.selector = Some(modified_selector);

    let new_svc = Service {
        spec: Some(spec),
        metadata: ObjectMeta {
            name: Some(app_name),
            namespace: Some(svc_namespace.to_string()),
            labels: Some(labels),
            owner_references: Some(owner_references),
            ..Default::default()
        },
        ..Default::default()
    };

    Ok(new_svc)
}

/// Update Kubernetes Service ownership references.
///
/// Example:
///
/// ```no_run
/// use akri_shared::k8s::{
///     OwnershipInfo,
///     OwnershipType,
///     service
/// };
/// use kube::client::Client;
/// use kube::config;
///
/// # #[tokio::main]
/// # async fn main() {
/// let selector = "environment=production,app=nginx";
/// let api_client = Client::try_default().await.unwrap();
/// for svc in service::find_services_with_selector(&selector, api_client).await.unwrap() {
///     let mut svc = svc;
///     service::update_ownership(
///         &mut svc,
///         OwnershipInfo::new(
///             OwnershipType::Pod,
///             "pod_name".to_string(),
///             "pod_uid".to_string(),
///         ),
///         true
///     ).unwrap();
/// }
/// # }
/// ```
pub fn update_ownership(
    svc_to_update: &mut Service,
    ownership: OwnershipInfo,
    replace_references: bool,
) -> anyhow::Result<()> {
    let ownership_ref = OwnerReference {
        api_version: ownership.get_api_version(),
        kind: ownership.get_kind(),
        controller: ownership.get_controller(),
        block_owner_deletion: ownership.get_block_owner_deletion(),
        name: ownership.get_name(),
        uid: ownership.get_uid(),
    };
    if replace_references || svc_to_update.metadata.owner_references.is_none() {
        // Replace all existing ownerReferences with specified ownership
        svc_to_update.metadata.owner_references = Some(vec![ownership_ref]);
    } else {
        // Add ownership to list IFF the UID doesn't already exist
        if !svc_to_update
            .metadata
            .owner_references
            .as_ref()
            .unwrap()
            .iter()
            .any(|x| x.uid == ownership.get_uid())
        {
            svc_to_update
                .metadata
                .owner_references
                .as_mut()
                .unwrap()
                .push(ownership_ref);
        }
    }
    Ok(())
}

#[cfg(test)]
mod svcspec_tests {
    use super::super::OwnershipType;
    use super::*;
    use env_logger;

    use k8s_openapi::api::core::v1::ServiceStatus;
    use kube::api::ObjectMeta;

    #[test]
    fn test_create_service_app_name() {
        let _ = env_logger::builder().is_test(true).try_init();

        assert_eq!(
            "node-a-suffix",
            create_service_app_name("foo", "node.a", "suffix", true)
        );
        assert_eq!(
            "foo-suffix",
            create_service_app_name("foo", "node.a", "suffix", false)
        );

        assert_eq!(
            "node-a-suffix",
            create_service_app_name("foo", "node-a", "suffix", true)
        );
        assert_eq!(
            "foo-suffix",
            create_service_app_name("foo", "node-a", "suffix", false)
        );
    }

    #[test]
    fn test_update_ownership_replace() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mut svc = Service {
            metadata: ObjectMeta::default(),
            spec: Some(ServiceSpec::default()),
            status: Some(ServiceStatus::default()),
        };

        assert!(svc.metadata.owner_references.is_none());
        update_ownership(
            &mut svc,
            OwnershipInfo {
                object_type: OwnershipType::Pod,
                object_name: "object1".to_string(),
                object_uid: "uid1".to_string(),
            },
            true,
        )
        .unwrap();
        assert_eq!(1, svc.metadata.owner_references.as_ref().unwrap().len());
        assert_eq!(
            "object1",
            &svc.metadata.owner_references.as_ref().unwrap()[0].name
        );
        assert_eq!(
            "uid1",
            &svc.metadata.owner_references.as_ref().unwrap()[0].uid
        );

        update_ownership(
            &mut svc,
            OwnershipInfo {
                object_type: OwnershipType::Pod,
                object_name: "object2".to_string(),
                object_uid: "uid2".to_string(),
            },
            true,
        )
        .unwrap();
        assert_eq!(1, svc.metadata.owner_references.as_ref().unwrap().len());
        assert_eq!(
            "object2",
            &svc.metadata.owner_references.as_ref().unwrap()[0].name
        );
        assert_eq!(
            "uid2",
            &svc.metadata.owner_references.as_ref().unwrap()[0].uid
        );
    }

    #[test]
    fn test_update_ownership_append() {
        let _ = env_logger::builder().is_test(true).try_init();

        let svc = Service {
            metadata: ObjectMeta::default(),
            spec: Some(ServiceSpec::default()),
            status: Some(ServiceStatus::default()),
        };

        assert!(svc.metadata.owner_references.is_none());
        let mut svc = svc;
        update_ownership(
            &mut svc,
            OwnershipInfo {
                object_type: OwnershipType::Pod,
                object_name: "object1".to_string(),
                object_uid: "uid1".to_string(),
            },
            false,
        )
        .unwrap();
        assert_eq!(1, svc.metadata.owner_references.as_ref().unwrap().len());
        assert_eq!(
            "object1",
            &svc.metadata.owner_references.as_ref().unwrap()[0].name
        );
        assert_eq!(
            "uid1",
            &svc.metadata.owner_references.as_ref().unwrap()[0].uid
        );

        update_ownership(
            &mut svc,
            OwnershipInfo {
                object_type: OwnershipType::Pod,
                object_name: "object2".to_string(),
                object_uid: "uid2".to_string(),
            },
            false,
        )
        .unwrap();
        assert_eq!(2, svc.metadata.owner_references.as_ref().unwrap().len());
        assert_eq!(
            "object1",
            &svc.metadata.owner_references.as_ref().unwrap()[0].name
        );
        assert_eq!(
            "uid1",
            &svc.metadata.owner_references.as_ref().unwrap()[0].uid
        );
        assert_eq!(
            "object2",
            &svc.metadata.owner_references.as_ref().unwrap()[1].name
        );
        assert_eq!(
            "uid2",
            &svc.metadata.owner_references.as_ref().unwrap()[1].uid
        );

        // Test that trying to add the same UID doesn't result in
        // duplicate
        update_ownership(
            &mut svc,
            OwnershipInfo {
                object_type: OwnershipType::Pod,
                object_name: "object2".to_string(),
                object_uid: "uid2".to_string(),
            },
            false,
        )
        .unwrap();
        assert_eq!(2, svc.metadata.owner_references.as_ref().unwrap().len());
        assert_eq!(
            "object1",
            &svc.metadata.owner_references.as_ref().unwrap()[0].name
        );
        assert_eq!(
            "uid1",
            &svc.metadata.owner_references.as_ref().unwrap()[0].uid
        );
        assert_eq!(
            "object2",
            &svc.metadata.owner_references.as_ref().unwrap()[1].name
        );
        assert_eq!(
            "uid2",
            &svc.metadata.owner_references.as_ref().unwrap()[1].uid
        );
    }

    #[test]
    fn test_svc_spec_creation() {
        let _ = env_logger::builder().is_test(true).try_init();

        let svc_namespace = "svc_namespace".to_string();
        let instance_name = "instance_name".to_string();
        let configuration_name = "configuration_name".to_string();

        let object_name = "owner_object".to_string();
        let object_uid = "owner_uid".to_string();

        for node_specific_svc in &[true, false] {
            let mut preexisting_selector = BTreeMap::new();
            preexisting_selector.insert(
                "do-not-change".to_string(),
                "this-node-selector".to_string(),
            );
            let svc_spec = ServiceSpec {
                selector: Some(preexisting_selector),
                ..Default::default()
            };

            let svc = create_new_service_from_spec(
                &svc_namespace,
                &instance_name,
                &configuration_name,
                OwnershipInfo::new(OwnershipType::Pod, object_name.clone(), object_uid.clone()),
                &svc_spec,
                *node_specific_svc,
            )
            .unwrap();

            let app_name = create_service_app_name(
                &configuration_name,
                &instance_name,
                "svc",
                *node_specific_svc,
            );

            // Validate the metadata name/namesapce
            assert_eq!(&app_name, &svc.metadata.clone().name.unwrap());
            assert_eq!(&svc_namespace, &svc.metadata.clone().namespace.unwrap());

            // Validate the labels added
            assert_eq!(
                &&app_name,
                &svc.metadata
                    .clone()
                    .labels
                    .unwrap()
                    .get(APP_LABEL_ID)
                    .unwrap()
            );
            assert_eq!(
                &&API_NAMESPACE.to_string(),
                &svc.metadata
                    .clone()
                    .labels
                    .unwrap()
                    .get(CONTROLLER_LABEL_ID)
                    .unwrap()
            );
            if *node_specific_svc {
                assert_eq!(
                    &&instance_name,
                    &svc.metadata
                        .clone()
                        .labels
                        .unwrap()
                        .get(AKRI_INSTANCE_LABEL_NAME)
                        .unwrap()
                );
            } else {
                assert_eq!(
                    &&configuration_name,
                    &svc.metadata
                        .clone()
                        .labels
                        .unwrap()
                        .get(AKRI_CONFIGURATION_LABEL_NAME)
                        .unwrap()
                );
            }

            // Validate ownerReference
            assert_eq!(
                object_name,
                svc.metadata
                    .clone()
                    .owner_references
                    .as_ref()
                    .unwrap()
                    .get(0)
                    .unwrap()
                    .name
            );
            assert_eq!(
                object_uid,
                svc.metadata
                    .clone()
                    .owner_references
                    .as_ref()
                    .unwrap()
                    .get(0)
                    .unwrap()
                    .uid
            );
            assert_eq!(
                "Pod",
                &svc.metadata
                    .clone()
                    .owner_references
                    .as_ref()
                    .unwrap()
                    .get(0)
                    .unwrap()
                    .kind
            );
            assert_eq!(
                "core/v1",
                &svc.metadata
                    .clone()
                    .owner_references
                    .unwrap()
                    .get(0)
                    .unwrap()
                    .api_version
            );
            assert!(svc
                .metadata
                .clone()
                .owner_references
                .unwrap()
                .get(0)
                .unwrap()
                .controller
                .unwrap());
            assert!(svc
                .metadata
                .clone()
                .owner_references
                .unwrap()
                .get(0)
                .unwrap()
                .block_owner_deletion
                .unwrap());

            // Validate the existing selector unchanged
            assert_eq!(
                &&"this-node-selector",
                &svc.spec
                    .as_ref()
                    .unwrap()
                    .selector
                    .as_ref()
                    .unwrap()
                    .get("do-not-change")
                    .unwrap()
            );
            // Validate the selector added
            assert_eq!(
                &&API_NAMESPACE.to_string(),
                &svc.spec
                    .as_ref()
                    .unwrap()
                    .selector
                    .as_ref()
                    .unwrap()
                    .get(CONTROLLER_LABEL_ID)
                    .unwrap()
            );
            if *node_specific_svc {
                assert_eq!(
                    &&instance_name,
                    &svc.spec
                        .as_ref()
                        .unwrap()
                        .selector
                        .as_ref()
                        .unwrap()
                        .get(AKRI_INSTANCE_LABEL_NAME)
                        .unwrap()
                );
            } else {
                assert_eq!(
                    &&configuration_name,
                    &svc.spec
                        .as_ref()
                        .unwrap()
                        .selector
                        .as_ref()
                        .unwrap()
                        .get(AKRI_CONFIGURATION_LABEL_NAME)
                        .unwrap()
                );
            }
        }
    }
}

/// Create Kubernetes Service
///
/// Example:
///
/// ```no_run
/// use akri_shared::k8s::service;
/// use kube::client::Client;
/// use kube::config;
/// use k8s_openapi::api::core::v1::Service;
///
/// # #[tokio::main]
/// # async fn main() {
/// let api_client = Client::try_default().await.unwrap();
/// service::create_service(&Service::default(), "svc_namespace", api_client).await.unwrap();
/// # }
/// ```
pub async fn create_service(
    svc_to_create: &Service,
    namespace: &str,
    kube_client: Client,
) -> Result<(), anyhow::Error> {
    trace!("create_service enter");
    let services: Api<Service> = Api::namespaced(kube_client, namespace);
    info!("create_service svcs.create(...).await?:");
    match services.create(&PostParams::default(), svc_to_create).await {
        Ok(created_svc) => {
            info!(
                "create_service services.create return: {:?}",
                created_svc.metadata.name
            );
            Ok(())
        }
        Err(kube::Error::Api(ae)) => {
            error!(
                "create_service services.create [{:?}] returned kube error: {:?}",
                serde_json::to_string(&svc_to_create),
                ae
            );
            Ok(())
        }
        Err(e) => {
            error!(
                "create_service services.create [{:?}] error: {:?}",
                serde_json::to_string(&svc_to_create),
                e
            );
            Err(anyhow::anyhow!(e))
        }
    }
}

/// Remove Kubernetes Service
///
/// Example:
///
/// ```no_run
/// use akri_shared::k8s::service;
/// use kube::client::Client;
/// use kube::config;
///
/// # #[tokio::main]
/// # async fn main() {
/// let api_client = Client::try_default().await.unwrap();
/// service::remove_service("svc_to_remove", "svc_namespace", api_client).await.unwrap();
/// # }
/// ```
pub async fn remove_service(
    svc_to_remove: &str,
    namespace: &str,
    kube_client: Client,
) -> Result<(), anyhow::Error> {
    trace!("remove_service enter");
    let svcs: Api<Service> = Api::namespaced(kube_client, namespace);
    info!("remove_service svcs.create(...).await?:");
    match svcs.delete(svc_to_remove, &DeleteParams::default()).await {
        Ok(deleted_svc) => match deleted_svc {
            Either::Left(spec) => {
                info!(
                    "remove_service svcs.delete return: {:?}",
                    &spec.metadata.name
                );
                Ok(())
            }
            Either::Right(status) => {
                info!("remove_service svcs.delete return: {:?}", &status.status);
                Ok(())
            }
        },
        Err(kube::Error::Api(ae)) => {
            if ae.code == ERROR_NOT_FOUND {
                trace!("remove_service - service already deleted");
                Ok(())
            } else {
                error!(
                    "remove_service svcs.delete [{:?}] returned kube error: {:?}",
                    &svc_to_remove, ae
                );
                Err(anyhow::anyhow!(ae))
            }
        }
        Err(e) => {
            error!(
                "remove_service svcs.delete [{:?}] error: {:?}",
                &svc_to_remove, e
            );
            Err(anyhow::anyhow!(e))
        }
    }
}

/// Update Kubernetes Service
///
/// Example:
///
/// ```no_run
/// use akri_shared::k8s::service;
/// use kube::client::Client;
/// use kube::config;
///
/// # #[tokio::main]
/// # async fn main() {
/// let selector = "environment=production,app=nginx";
/// let api_client = Client::try_default().await.unwrap();
/// for svc in service::find_services_with_selector(&selector, api_client).await.unwrap() {
///     let svc_name = &svc.metadata.name.clone().unwrap();
///     let svc_namespace = &svc.metadata.namespace.as_ref().unwrap().clone();
///     let loop_api_client = Client::try_default().await.unwrap();
///     let updated_svc = service::update_service(
///         &svc,
///         &svc_name,
///         &svc_namespace,
///         loop_api_client).await.unwrap();
/// }
/// # }
/// ```
pub async fn update_service(
    svc_to_update: &Service,
    name: &str,
    namespace: &str,
    kube_client: Client,
) -> Result<(), anyhow::Error> {
    trace!(
        "update_service enter name:{} namespace: {}",
        &name,
        &namespace
    );
    let svcs: Api<Service> = Api::namespaced(kube_client, namespace);

    info!("remove_service svcs.patch(...).await?:");
    match svcs
        .patch(name, &PatchParams::default(), &Patch::Merge(&svc_to_update))
        .await
    {
        Ok(_service_modified) => {
            log::trace!("update_service return");
            Ok(())
        }
        Err(kube::Error::Api(ae)) => {
            log::trace!(
                "update_service kube_client.request returned kube error: {:?}",
                ae
            );
            Err(anyhow::anyhow!(ae))
        }
        Err(e) => {
            log::trace!("update_service kube_client.request error: {:?}", e);
            Err(anyhow::anyhow!(e))
        }
    }
}
