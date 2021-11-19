use k8s_openapi::api::core::v1::Node;
use kube::{api::Api, client::Client};
use log::trace;

/// Get Kubernetes Node with a given name
///
/// Example:
///
/// ```no_run
/// use akri_shared::k8s::node;
/// use kube::client::Client;
/// use kube::config;
///
/// # #[tokio::main]
/// # async fn main() {
/// let label_selector = Some("environment=production,app=nginx".to_string());
/// let api_client = Client::try_default().await.unwrap();
/// let node = node::find_node("node-a", api_client).await.unwrap();
/// # }
/// ```
pub async fn find_node(name: &str, kube_client: Client) -> Result<Node, anyhow::Error> {
    trace!("find_node with name={}", name);
    let nodes: Api<Node> = Api::all(kube_client);
    trace!("find_node PRE nodes.get(...).await?");
    let result = nodes.get(name).await;
    trace!("find_node return");
    Ok(result?)
}
