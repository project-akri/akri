use k8s_openapi::api::core::v1::{NodeSpec, NodeStatus};
use kube::{
    api::{Api, Object},
    client::APIClient,
};
use log::trace;

/// Get Kubernetes Node with a given name
///
/// Example:
///
/// ```no_run
/// use akri_shared::k8s::node;
/// use kube::client::APIClient;
/// use kube::config;
///
/// # #[tokio::main]
/// # async fn main() {
/// let label_selector = Some("environment=production,app=nginx".to_string());
/// let api_client = APIClient::new(config::incluster_config().unwrap());
/// let node = node::find_node("node-a", api_client).await.unwrap();
/// # }
/// ```
pub async fn find_node(
    name: &str,
    kube_client: APIClient,
) -> Result<Object<NodeSpec, NodeStatus>, Box<dyn std::error::Error + Send + Sync + 'static>> {
    trace!("find_node with name={:?}", &name);
    let nodes = Api::v1Node(kube_client);
    trace!("find_node PRE nodes.get(...).await?");
    let result = nodes.get(&name).await;
    trace!("find_node return");
    Ok(result?)
}
