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

/// Length of time a Pod can be pending before we give up and retry
pub const PENDING_POD_GRACE_PERIOD_MINUTES: i64 = 5;
/// Length of time a Pod can be in an error state before we retry
pub const FAILED_POD_GRACE_PERIOD_MINUTES: i64 = 0;

/// Get Kubernetes Jobs with a given label or field selector
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
    trace!("find_jobs_with_selector PRE jobs.list(...).await?");
    let result = jobs.list(&job_list_params).await;
    trace!("find_jobs_with_selector return");
    Ok(result?)
}

/// Create Kubernetes Job based on Instance & Config.
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
    let pod_labels = labels.clone();
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
    modified_job_spec.template.metadata = Some(ObjectMeta {
        labels: Some(pod_labels),
        ..Default::default()
    });
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

/// Get Instance for a given name and namespace
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
/// let job = job::find_job(
///     "job-1",
///     "default",
///     api_client).await.unwrap();
/// # }
/// ```
pub async fn find_job(name: &str, namespace: &str, kube_client: Client) -> anyhow::Result<Job> {
    log::trace!("find_job enter");
    let client: Api<Job> = Api::namespaced(kube_client, namespace);

    log::trace!("find_job getting job with name {}", name);

    client.get(name).await.map_err(anyhow::Error::from)
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
    info!("create_job jobs.create(...).await?:");
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
    info!("remove_job jobs.delete(...).await?:");
    let dps = DeleteParams {
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

/// Delete a collection of Jobs with the given selectors
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
/// job::delete_jobs_with_selector(label_selector, None, "default", api_client).await.unwrap();
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
/// job::delete_jobs_with_selector(None, field_selector, "default", api_client).await.unwrap();
/// # }
/// ```
pub async fn delete_jobs_with_selector(
    label_selector: Option<String>,
    field_selector: Option<String>,
    namespace: &str,
    kube_client: Client,
) -> Result<(), anyhow::Error> {
    trace!("remove_job enter");
    let jobs: Api<Job> = Api::namespaced(kube_client, namespace);
    let lps = ListParams {
        label_selector,
        field_selector,
        ..Default::default()
    };
    info!("remove_job jobs.delete(...).await?:");
    match jobs
        .delete_collection(&DeleteParams::default(), &lps)
        .await?
    {
        either::Left(list) => {
            let names: Vec<_> = list.iter().map(kube::ResourceExt::name).collect();
            trace!("Deleting collection of pods: {:?}", names);
        }
        either::Right(status) => {
            trace!("Deleted collection of pods: status={:?}", status);
        }
    }
    Ok(())
}
