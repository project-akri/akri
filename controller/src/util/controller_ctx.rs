use std::collections::HashMap;
use std::sync::Arc;

use akri_shared::akri::configuration::Configuration;
use akri_shared::akri::instance::Instance;
use akri_shared::k8s::api::IntoApi;

use k8s_openapi::api::batch::v1::Job;
use k8s_openapi::api::core::v1::{Node, Pod, Service};

use tokio::sync::RwLock;

/// Pod states that BrokerPodWatcher is interested in
///
/// PodState describes the various states that the controller can
/// react to for Pods.
#[derive(Clone, Debug, PartialEq)]
pub enum PodState {
    /// Pod is in Pending state and no action is needed.
    Pending,
    /// Pod is in Running state and needs to ensure that
    /// instance and configuration services are running
    Running,
    /// Pod is in Failed/Completed/Succeeded state and
    /// needs to remove any instance and configuration
    /// services that are not supported by other Running
    /// Pods.  Also, at this point, if an Instance still
    /// exists, instance_action::handle_instance_change
    /// needs to be called to ensure that Pods are
    /// restarted
    Ended,
    /// Pod is in Deleted state and needs to remove any
    /// instance and configuration services that are not
    /// supported by other Running Pods. Also, at this
    /// point, if an Instance still exists, and the Pod is
    /// owned by the Instance,
    /// instance_action::handle_instance_change needs to be
    /// called to ensure that Pods are restarted. Akri
    /// places an Instance OwnerReference on all the Pods it
    /// deploys. This declares that the Instance owns that
    /// Pod and Akri's Controller explicitly manages its
    /// deployment. However, if the Pod is not owned by the
    /// Instance, Akri should not assume retry logic and
    /// should cease action. The owning object (ie Job) will
    /// handle retries as necessary.
    Deleted,
}

/// Node states that NodeWatcher is interested in
///
/// NodeState describes the various states that the controller can
/// react to for Nodes.
#[derive(Clone, Debug, PartialEq)]
pub enum NodeState {
    /// Node has been seen, but not Running yet
    Known,
    /// Node has been seen Running
    Running,
    /// A previously Running Node has been seen as not Running
    /// and the Instances have been cleaned of references to that
    /// vanished Node
    InstancesCleaned,
}

pub trait ControllerKubeClient:
    IntoApi<Instance>
    + IntoApi<Configuration>
    + IntoApi<Pod>
    + IntoApi<Job>
    + IntoApi<Service>
    + IntoApi<Node>
{
}

impl<
        T: IntoApi<Instance>
            + IntoApi<Configuration>
            + IntoApi<Pod>
            + IntoApi<Job>
            + IntoApi<Service>
            + IntoApi<Node>,
    > ControllerKubeClient for T
{
}

pub struct ControllerContext {
    /// Kubernetes client
    pub client: Arc<dyn ControllerKubeClient>,
    pub known_pods: Arc<RwLock<HashMap<String, PodState>>>,
    pub known_nodes: Arc<RwLock<HashMap<String, NodeState>>>,
}

impl ControllerContext {
    pub fn new(client: Arc<dyn ControllerKubeClient>) -> Self {
        ControllerContext {
            client,
            known_pods: Arc::new(RwLock::new(HashMap::new())),
            known_nodes: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}
