use super::super::akri::{instance::Instance, API_NAMESPACE};
use super::{
    pod::modify_pod_spec,
    pod::{
        AKRI_CONFIGURATION_LABEL_NAME, AKRI_INSTANCE_LABEL_NAME, APP_LABEL_ID, CONTROLLER_LABEL_ID,
    },
    OwnershipInfo, ERROR_CONFLICT, ERROR_NOT_FOUND,
};
use either::Either;
use k8s_openapi::api::batch::v1::{Job, JobSpec};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{ObjectMeta, OwnerReference};
use kube::{
    api::{Api, DeleteParams, ListParams, ObjectList, PostParams, PropagationPolicy},
    client::Client,
};
use log::{error, info, trace};
use std::collections::BTreeMap;

/// Find Kubernetes Jobs with a given label or field selector
///
/// Example:
///
/// ```no_run
/// use akri_shared::k8s::job;
/// use kube::client::Client;
/// use kube::config;
///
/// # #[tokio::main]
/// # async fn main() {
/// let label_selector = Some("environment=production,app=nginx".to_string());
/// let api_client = Client::try_default().await.unwrap();
/// for job in job::find_jobs_with_selector(label_selector, None, api_client).await.unwrap() {
///     println!("found job: {}", job.metadata.name.unwrap())
/// }
/// # }
/// ```
///
/// ```no_run
/// use akri_shared::k8s::job;
/// use kube::client::Client;
/// use kube::config;
///
/// # #[tokio::main]
/// # async fn main() {
/// let field_selector = Some("spec.nodeName=node-a".to_string());
/// let api_client = Client::try_default().await.unwrap();
/// for job in job::find_jobs_with_selector(None, field_selector, api_client).await.unwrap() {
///     println!("found job: {}", job.metadata.name.unwrap())
/// }
/// # }
/// ```
pub async fn find_jobs_with_selector(
    label_selector: Option<String>,
    field_selector: Option<String>,
    kube_client: Client,
) -> Result<ObjectList<Job>, anyhow::Error> {
    trace!(
        "find_jobs_with_selector with label_selector={:?} field_selector={:?}",
        &label_selector,
        &field_selector
    );
    let jobs: Api<Job> = Api::all(kube_client);
    let job_list_params = ListParams {
        label_selector,
        field_selector,
        ..Default::default()
    };
    let result = jobs.list(&job_list_params).await;
    trace!("find_jobs_with_selector return");
    Ok(result?)
}

/// Create Kubernetes Job with given Instance and OwnershipInfo
///
/// Example:
///
/// ```no_run
/// use akri_shared::k8s::{
///     OwnershipInfo,
///     OwnershipType,
///     job
/// };
/// use akri_shared::akri::instance::{Instance, InstanceSpec};
/// use kube::client::Client;
/// use kube::config;
/// use k8s_openapi::api::batch::v1::JobSpec;
///
/// # #[tokio::main]
/// # async fn main() {
/// let api_client = Client::try_default().await.unwrap();
/// let instance_spec = InstanceSpec {
///     configuration_name: "configuration_name".to_string(),
///     shared: true,
///     nodes: Vec::new(),
///     device_usage: std::collections::HashMap::new(),
///     broker_properties: std::collections::HashMap::new()
/// };    
/// let instance = Instance::new("instance_name", instance_spec);
/// let job = job::create_new_job_from_spec(
///     &instance,
///     OwnershipInfo::new(
///         OwnershipType::Instance,
///         "instance_name".to_string(),
///         "instance_uid".to_string()
///     ),
///     "akri.sh/configuration_name",
///     &JobSpec::default(),"app_name").unwrap();
/// # }
/// ```
pub fn create_new_job_from_spec(
    instance: &Instance,
    ownership: OwnershipInfo,
    resource_limit_name: &str,
    job_spec: &JobSpec,
    app_name: &str,
) -> anyhow::Result<Job> {
    trace!("create_new_job_from_spec enter");
    // TODO: Consider optionally enabling podAntiAffinity in this function
    // (using an instance name label) to ensure only one Job runs on each Node per instance.
    let instance_name = instance.metadata.name.as_ref().unwrap();
    let mut labels: BTreeMap<String, String> = BTreeMap::new();
    labels.insert(
        AKRI_CONFIGURATION_LABEL_NAME.to_string(),
        instance.spec.configuration_name.to_string(),
    );
    labels.insert(
        AKRI_INSTANCE_LABEL_NAME.to_string(),
        instance_name.to_string(),
    );
    let mut pod_labels = labels.clone();
    labels.insert(APP_LABEL_ID.to_string(), app_name.to_string());
    labels.insert(CONTROLLER_LABEL_ID.to_string(), API_NAMESPACE.to_string());

    let owner_references: Vec<OwnerReference> = vec![OwnerReference {
        api_version: ownership.get_api_version(),
        kind: ownership.get_kind(),
        controller: ownership.get_controller(),
        block_owner_deletion: ownership.get_block_owner_deletion(),
        name: ownership.get_name(),
        uid: ownership.get_uid(),
    }];

    let mut modified_job_spec = job_spec.clone();
    let mut pod_spec = modified_job_spec.template.spec.clone().unwrap();
    modify_pod_spec(&mut pod_spec, resource_limit_name, None);
    modified_job_spec
        .template
        .metadata
        .get_or_insert(ObjectMeta {
            ..Default::default()
        })
        .labels
        .get_or_insert(BTreeMap::new())
        .append(&mut pod_labels);
    modified_job_spec.template.spec = Some(pod_spec);
    let result = Job {
        spec: Some(modified_job_spec),
        metadata: ObjectMeta {
            name: Some(app_name.to_string()),
            namespace: Some(instance.metadata.namespace.as_ref().unwrap().to_string()),
            labels: Some(labels),
            owner_references: Some(owner_references),
            ..Default::default()
        },
        ..Default::default()
    };

    trace!("create_new_job_from_spec return");
    Ok(result)
}

/// Create Kubernetes Job
///
/// Example:
///
/// ```no_run
/// use akri_shared::k8s::job;
/// use kube::client::Client;
/// use kube::config;
/// use k8s_openapi::api::batch::v1::Job;
///
/// # #[tokio::main]
/// # async fn main() {
/// let api_client = Client::try_default().await.unwrap();
/// job::create_job(&Job::default(), "job_namespace", api_client).await.unwrap();
/// # }
/// ```
pub async fn create_job(
    job_to_create: &Job,
    namespace: &str,
    kube_client: Client,
) -> Result<(), anyhow::Error> {
    trace!("create_job enter");
    let jobs: Api<Job> = Api::namespaced(kube_client, namespace);
    match jobs.create(&PostParams::default(), job_to_create).await {
        Ok(created_job) => {
            info!(
                "create_job jobs.create return: {:?}",
                created_job.metadata.name
            );
            Ok(())
        }
        Err(kube::Error::Api(ae)) => {
            if ae.code == ERROR_CONFLICT {
                trace!("create_job - job already exists");
                Ok(())
            } else {
                error!(
                    "create_job jobs.create [{:?}] returned kube error: {:?}",
                    serde_json::to_string(&job_to_create),
                    ae
                );
                Err(anyhow::anyhow!(ae))
            }
        }
        Err(e) => {
            error!(
                "create_job jobs.create [{:?}] error: {:?}",
                serde_json::to_string(&job_to_create),
                e
            );
            Err(anyhow::anyhow!(e))
        }
    }
}

/// Remove Kubernetes Job
///
/// Example:
///
/// ```no_run
/// use akri_shared::k8s::job;
/// use kube::client::Client;
/// use kube::config;
///
/// # #[tokio::main]
/// # async fn main() {
/// let api_client = Client::try_default().await.unwrap();
/// job::remove_job("job_to_remove", "job_namespace", api_client).await.unwrap();
/// # }
/// ```
pub async fn remove_job(
    job_to_remove: &str,
    namespace: &str,
    kube_client: Client,
) -> Result<(), anyhow::Error> {
    trace!("remove_job enter");
    let jobs: Api<Job> = Api::namespaced(kube_client, namespace);
    let dps = DeleteParams {
        dry_run: false,
        propagation_policy: Some(PropagationPolicy::Background),
        ..Default::default()
    };
    match jobs.delete(job_to_remove, &dps).await {
        Ok(deleted_job) => match deleted_job {
            Either::Left(spec) => {
                info!("remove_job jobs.delete return: {:?}", &spec.metadata.name);
                Ok(())
            }
            Either::Right(status) => {
                info!("remove_job jobs.delete return: {:?}", &status.status);
                Ok(())
            }
        },
        Err(kube::Error::Api(ae)) => {
            if ae.code == ERROR_NOT_FOUND {
                trace!("remove_job - job already removed");
                Ok(())
            } else {
                error!(
                    "remove_job jobs.delete [{:?}] returned kube error: {:?}",
                    &job_to_remove, ae
                );
                Err(anyhow::anyhow!(ae))
            }
        }
        Err(e) => {
            error!(
                "remove_job jobs.delete [{:?}] error: {:?}",
                &job_to_remove, e
            );
            Err(anyhow::anyhow!(e))
        }
    }
}

#[cfg(test)]
mod broker_jobspec_tests {
    use super::super::super::{akri::API_VERSION, os::file};
    use super::super::{OwnershipType, RESOURCE_REQUIREMENTS_KEY};
    use super::*;
    use k8s_openapi::api::core::v1::{Container, PodSpec, PodTemplateSpec, ResourceRequirements};
    use k8s_openapi::apimachinery::pkg::api::resource::Quantity;

    type ResourceQuantityType = BTreeMap<String, Quantity>;

    #[test]
    fn test_create_new_job_from_spec() {
        let mut placeholder_limits1: ResourceQuantityType = BTreeMap::new();
        placeholder_limits1.insert(RESOURCE_REQUIREMENTS_KEY.to_string(), Default::default());
        placeholder_limits1.insert("do-not-change-this".to_string(), Default::default());
        let placeholder_requests1 = placeholder_limits1.clone();
        let c = Container {
            image: Some("image1".to_string()),
            resources: Some(ResourceRequirements {
                limits: Some(placeholder_limits1),
                requests: Some(placeholder_requests1),
            }),
            ..Default::default()
        };
        // More extensive PodSpec testing of `modify_pod_spec` covered in `pod.rs` module tests
        let pod_spec = PodSpec {
            containers: vec![c],
            ..Default::default()
        };

        let mut preexisting_labels = BTreeMap::new();
        preexisting_labels.insert("app".to_string(), "management".to_string());
        let mut preexisting_annotations = BTreeMap::new();
        preexisting_annotations.insert("version".to_string(), "1.0".to_string());
        let job_spec = JobSpec {
            parallelism: Some(3),
            backoff_limit: Some(2),
            template: PodTemplateSpec {
                metadata: Some(ObjectMeta {
                    labels: Some(preexisting_labels),
                    annotations: Some(preexisting_annotations),
                    ..Default::default()
                }),
                spec: Some(pod_spec),
            },
            ..Default::default()
        };
        let app_name = "job-name";
        let instance_json = file::read_file_to_string("../test/json/local-instance.json");
        let instance: Instance = serde_json::from_str(&instance_json).unwrap();
        let instance_name = instance.metadata.name.as_ref().unwrap();
        let instance_uid = instance.metadata.uid.as_ref().unwrap();
        let job = create_new_job_from_spec(
            &instance,
            OwnershipInfo::new(
                OwnershipType::Instance,
                instance_name.to_string(),
                instance_uid.to_string(),
            ),
            instance_name,
            &job_spec,
            app_name,
        )
        .unwrap();

        // Validate that uses instance namespace
        assert_eq!(
            &instance.metadata.namespace.as_ref().unwrap(),
            &job.metadata.namespace.as_ref().unwrap()
        );

        // Validate that Akri labels are added to Job
        assert_eq!(
            app_name,
            job.metadata
                .labels
                .as_ref()
                .unwrap()
                .get(APP_LABEL_ID)
                .unwrap()
        );
        assert_eq!(
            &API_NAMESPACE,
            job.metadata
                .labels
                .as_ref()
                .unwrap()
                .get(CONTROLLER_LABEL_ID)
                .unwrap()
        );
        assert_eq!(
            &instance.spec.configuration_name,
            job.metadata
                .labels
                .as_ref()
                .unwrap()
                .get(AKRI_CONFIGURATION_LABEL_NAME)
                .unwrap()
        );
        assert_eq!(
            instance_name,
            job.metadata
                .labels
                .as_ref()
                .unwrap()
                .get(AKRI_INSTANCE_LABEL_NAME)
                .unwrap()
        );

        // Validate that pre-existing fields persist in Job
        assert_eq!(3, job.spec.as_ref().unwrap().parallelism.unwrap());
        assert_eq!(2, job.spec.as_ref().unwrap().backoff_limit.unwrap());

        // Validate that Configuration and Instance labels added to Pod
        assert_eq!(
            &instance.spec.configuration_name,
            job.spec
                .as_ref()
                .unwrap()
                .template
                .metadata
                .as_ref()
                .unwrap()
                .labels
                .as_ref()
                .unwrap()
                .get(AKRI_CONFIGURATION_LABEL_NAME)
                .unwrap()
        );
        assert_eq!(
            instance_name,
            job.spec
                .as_ref()
                .unwrap()
                .template
                .metadata
                .as_ref()
                .unwrap()
                .labels
                .as_ref()
                .unwrap()
                .get(AKRI_INSTANCE_LABEL_NAME)
                .unwrap()
        );

        // Validate that pre-existing metadata persist in Pod
        assert_eq!(
            "1.0",
            job.spec
                .as_ref()
                .unwrap()
                .template
                .metadata
                .as_ref()
                .unwrap()
                .annotations
                .as_ref()
                .unwrap()
                .get("version")
                .unwrap()
        );

        // Validate OwnerReferences
        assert_eq!(
            instance_name,
            &job.metadata
                .owner_references
                .as_ref()
                .unwrap()
                .get(0)
                .unwrap()
                .name
        );
        assert_eq!(
            instance_uid,
            &job.metadata
                .owner_references
                .as_ref()
                .unwrap()
                .get(0)
                .unwrap()
                .uid
        );
        assert_eq!(
            "Instance",
            job.metadata
                .owner_references
                .as_ref()
                .unwrap()
                .get(0)
                .unwrap()
                .kind
        );
        assert_eq!(
            format!("{}/{}", API_NAMESPACE, API_VERSION),
            job.metadata
                .owner_references
                .as_ref()
                .unwrap()
                .get(0)
                .unwrap()
                .api_version
        );
        assert!(job
            .metadata
            .owner_references
            .as_ref()
            .unwrap()
            .get(0)
            .unwrap()
            .controller
            .unwrap());
        assert!(job
            .metadata
            .owner_references
            .as_ref()
            .unwrap()
            .get(0)
            .unwrap()
            .block_owner_deletion
            .unwrap());
    }
}
