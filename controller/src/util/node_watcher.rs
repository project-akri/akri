//! This is used to handle Nodes disappearing.
//!
//! When a Node disapears, make sure that any Instance that
//! references the Node is cleaned.  This means that the
//! Instance.nodes property no longer contains the node and
//! that the Instance.deviceUsage property no longer contains
//! slots that are occupied by the node.
use crate::util::{
    controller_ctx::{ControllerContext, NodeState},
    ControllerError, Result,
};
use akri_shared::k8s::api::Api;

use akri_shared::akri::instance::{device_usage::NodeUsage, Instance};
use anyhow::Context;
use futures::StreamExt;
use k8s_openapi::api::core::v1::{Node, NodeStatus};
use kube::{
    api::{
        ListParams, NotUsed, Object, ObjectList, ObjectMeta, Patch, PatchParams, ResourceExt,
        TypeMeta,
    },
    runtime::{
        controller::{Action, Controller},
        finalizer::{finalizer, Event},
        reflector::Lookup,
        watcher::Config,
    },
};
use log::{error, info, trace};
use std::str::FromStr;
use std::{collections::HashMap, sync::Arc};

pub static NODE_FINALIZER: &str = "nodes.kube.rs";

/// Initialize the instance controller
/// TODO: consider passing state that is shared among controllers such as a metrics exporter
pub async fn run(ctx: Arc<ControllerContext>) {
    let api = ctx.client.all().as_inner();
    if let Err(e) = api.list(&ListParams::default().limit(1)).await {
        error!("Nodes are not queryable; {e:?}");
        std::process::exit(1);
    }
    Controller::new(api, Config::default().any_semantic())
        .shutdown_on_signal()
        .run(reconcile, error_policy, ctx)
        // TODO: needs update for tokio?
        .filter_map(|x| async move { std::result::Result::ok(x) })
        .for_each(|_| futures::future::ready(()))
        .await;
}

fn error_policy(_node: Arc<Node>, error: &ControllerError, _ctx: Arc<ControllerContext>) -> Action {
    log::warn!("reconcile failed: {:?}", error);
    Action::requeue(std::time::Duration::from_secs(5 * 60))
}

/// This function is the main Reconcile function for Node resources
/// This will get called every time an Node is added, deleted, or changed, it will also be called for every existing Node on startup.
///
/// Nodes are constantly updated.  Cleanup  work for our services only
/// needs to be called once.
///
/// To achieve this, store each Node's state as either Known (Node has
/// been seen, but not Running), Running (Node has been seen as Running),
/// and InstanceCleaned (previously Running Node has been seen as not
/// Running).
///
/// When a Node is in the Known state, it is not Running.  If it has
/// never been seen as Running, it is likely being created and there is
/// no need to clean any Instance.
///
/// Once a Node moves through the Running state into a non Running
/// state, it becomes important to clean Instances referencing the
/// non-Running Node.
pub async fn reconcile(node: Arc<Node>, ctx: Arc<ControllerContext>) -> Result<Action> {
    trace!("Reconciling node {}", node.name_any());
    finalizer(
        &ctx.client.clone().all().as_inner(),
        NODE_FINALIZER,
        node,
        |event| reconcile_inner(event, ctx.clone()),
    )
    .await
    // .map_err(|_e| anyhow!("todo"))
    .map_err(|e| ControllerError::FinalizerError(Box::new(e)))
}

async fn reconcile_inner(event: Event<Node>, ctx: Arc<ControllerContext>) -> Result<Action> {
    match event {
        Event::Apply(node) => {
            let node_name = node.name_unchecked();
            info!("handle_node - Added or modified: {}", node_name);
            if is_node_ready(&node) {
                ctx.known_nodes
                    .write()
                    .await
                    .insert(node_name, NodeState::Running);
            } else {
                let mut guard = ctx.known_nodes.write().await;
                if let std::collections::hash_map::Entry::Vacant(e) = guard.entry(node_name) {
                    e.insert(NodeState::Known);
                } else {
                    // Node Modified
                    drop(guard);
                    handle_node_disappearance(&node, ctx.clone()).await?;
                }
            }
            Ok(Action::await_change())
        }
        Event::Cleanup(node) => {
            info!("handle_node - Deleted: {:?}", &node.name_unchecked());
            handle_node_disappearance(&node, ctx.clone()).await?;
            ctx.known_nodes.write().await.remove(&node.name_unchecked());
            Ok(Action::await_change())
        }
    }
}

/// This should be called for Nodes that are either !Ready or Deleted.
/// This function will clean up any Instances that reference a Node that
/// was previously Running.
async fn handle_node_disappearance(node: &Node, ctx: Arc<ControllerContext>) -> anyhow::Result<()> {
    let node_name = node.name_unchecked();
    trace!(
        "handle_node_disappearance - enter: {:?}",
        &node.metadata.name
    );
    let last_known_state = ctx
        .known_nodes
        .read()
        .await
        .get(&node_name)
        .unwrap_or(&NodeState::Running)
        .clone();
    trace!(
        "handle_node_disappearance - last_known_state: {:?}",
        &last_known_state
    );

    // If the node was running and no longer is, clear the node from
    // each instance's nodes list and deviceUsage map.
    if last_known_state == NodeState::Running {
        let api = ctx.client.all();
        let instances: ObjectList<Instance> = api.list(&ListParams::default()).await?;
        trace!(
            "handle_node_disappearance - found {:?} instances",
            instances.items.len()
        );
        for instance in instances.items {
            let instance_name = instance.name_unchecked();
            try_remove_nodes_from_instance(&node_name, &instance_name, &instance, api.as_ref())
                .await?;
            api.remove_finalizer(&instance, &node_name).await?;
        }
        ctx.known_nodes
            .write()
            .await
            .insert(node_name.to_string(), NodeState::InstancesCleaned);
    }
    Ok(())
}

/// This determines if a node is in the Ready state.
fn is_node_ready(k8s_node: &Node) -> bool {
    trace!("is_node_ready - for node {:?}", k8s_node.metadata.name);
    k8s_node
        .status
        .as_ref()
        .unwrap_or(&NodeStatus::default())
        .conditions
        .as_ref()
        .unwrap_or(&Vec::new())
        .last()
        .map_or(false, |condition| {
            condition.type_ == "Ready" && condition.status == "True"
        })
}

/// This attempts to remove nodes from the nodes list and deviceUsage
/// map in an Instance.  An attempt is made to update
/// the instance in etcd, any failure is returned.
async fn try_remove_nodes_from_instance(
    vanished_node_name: &str,
    instance_name: &str,
    instance: &Instance,
    api: &dyn Api<Instance>,
) -> Result<(), anyhow::Error> {
    trace!(
        "try_remove_nodes_from_instance - vanished_node_name: {:?}",
        &vanished_node_name
    );
    let modified_nodes = instance
        .spec
        .nodes
        .iter()
        .filter(|node| &vanished_node_name != node)
        .map(|node| node.into())
        .collect::<Vec<String>>();
    // Remove nodes from instance.deviceusage
    let modified_device_usage = instance
        .spec
        .device_usage
        .iter()
        .map(|(slot, usage)| match NodeUsage::from_str(usage) {
            Ok(node_usage) if node_usage.is_same_node(vanished_node_name) => {
                (slot.to_owned(), NodeUsage::default().to_string())
            }
            Ok(_) => (slot.to_owned(), usage.into()),
            Err(_) => (slot.to_owned(), usage.into()),
        })
        .collect::<HashMap<String, String>>();
    let mut modified_spec = instance.spec.clone();
    modified_spec.nodes = modified_nodes;
    modified_spec.device_usage = modified_device_usage;
    let patch = Patch::Merge(
        serde_json::to_value(Object {
            types: Some(TypeMeta {
                api_version: Instance::api_version(&()).to_string(),
                kind: Instance::kind(&()).to_string(),
            }),
            status: None::<NotUsed>,
            spec: modified_spec,
            metadata: ObjectMeta {
                name: Some(instance_name.to_string()),
                ..Default::default()
            },
        })
        .context("Could not create instance patch")?,
    );
    api.raw_patch(instance_name, &patch, &PatchParams::default())
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::shared_test_utils::mock_client::MockControllerKubeClient;
    use super::*;
    use akri_shared::{akri::instance::InstanceSpec, k8s::api::MockApi, os::file};

    fn instances_list(
        instance_name: &str,
        instance_namespace: &str,
    ) -> kube::Result<ObjectList<Instance>> {
        let list = serde_json::json!({
            "apiVersion": "v1",
            "kind": "List",
            "metadata": {
                "resourceVersion": "",
                "selfLink": ""
            },
            "items": [
                {
                    "apiVersion": "akri.sh/v0",
                    "kind": "Instance",
                    "metadata": {
                        "name": instance_name,
                        "namespace": instance_namespace,
                        "uid": "abcdegfh-ijkl-mnop-qrst-uvwxyz012345"
                    },
                    "spec": {
                        "configurationName": "config-a",
                        "capacity": 5,
                        "cdiName": "akri.sh/config-a=359973",
                        "deviceUsage": {
                            format!("{instance_name}-0"): "node-b",
                            format!("{instance_name}-1"): "node-a",
                            format!("{instance_name}-2"): "node-b",
                            format!("{instance_name}-3"): "node-a",
                            format!("{instance_name}-4"): "node-c",
                            format!("{instance_name}-5"): ""
                        },
                        "nodes": [ "node-a", "node-b", "node-c" ],
                        "shared": true
                    }
                }
            ]
        });
        Ok(serde_json::from_value(list).unwrap())
    }

    #[tokio::test]
    async fn test_reconcile_node_apply_ready() {
        let _ = env_logger::builder().is_test(true).try_init();
        let node_json = file::read_file_to_string("../test/json/node-a.json");
        let node: Node = serde_json::from_str(&node_json).unwrap();
        let node_name = node.name_unchecked();
        let mut mock = MockControllerKubeClient::default();
        mock.node
            .expect_all()
            .return_once(|| Box::new(MockApi::new()));
        let ctx = Arc::new(ControllerContext::new(Arc::new(mock), "test"));
        reconcile_inner(Event::Apply(Arc::new(node)), ctx.clone())
            .await
            .unwrap();

        assert_eq!(
            &NodeState::Running,
            ctx.known_nodes.read().await.get(&node_name).unwrap()
        );
    }

    #[tokio::test]
    async fn test_reconcile_node_apply_unready_unknown() {
        let _ = env_logger::builder().is_test(true).try_init();
        let node_json = file::read_file_to_string("../test/json/node-a-not-ready.json");
        let node: Node = serde_json::from_str(&node_json).unwrap();
        let node_name = node.name_unchecked();
        let mut mock = MockControllerKubeClient::default();
        mock.node
            .expect_all()
            .return_once(|| Box::new(MockApi::new()));
        let ctx = Arc::new(ControllerContext::new(Arc::new(mock), "test"));
        reconcile_inner(Event::Apply(Arc::new(node)), ctx.clone())
            .await
            .unwrap();

        assert_eq!(
            &NodeState::Known,
            ctx.known_nodes.read().await.get(&node_name).unwrap()
        );
    }
    // If a known node is modified and is still not ready, it should remain in the known state
    #[tokio::test]
    async fn test_reconcile_node_apply_unready_known() {
        let _ = env_logger::builder().is_test(true).try_init();
        let node_json = file::read_file_to_string("../test/json/node-a-not-ready.json");
        let node: Node = serde_json::from_str(&node_json).unwrap();
        let node_name = node.name_unchecked();
        let mut mock = MockControllerKubeClient::default();
        mock.node
            .expect_all()
            .return_once(|| Box::new(MockApi::new()));
        let ctx = Arc::new(ControllerContext::new(Arc::new(mock), "test"));
        ctx.known_nodes
            .write()
            .await
            .insert(node_name.clone(), NodeState::Known);
        reconcile_inner(Event::Apply(Arc::new(node)), ctx.clone())
            .await
            .unwrap();

        assert_eq!(
            &NodeState::Known,
            ctx.known_nodes.read().await.get(&node_name).unwrap()
        );
    }

    // If previously running node is modified and is not ready, it should remove the node from the instances' node lists
    #[tokio::test]
    async fn test_reconcile_node_apply_unready_previously_running() {
        let _ = env_logger::builder().is_test(true).try_init();
        let node_json = file::read_file_to_string("../test/json/node-a-not-ready.json");
        let node: Node = serde_json::from_str(&node_json).unwrap();
        let node_name = node.name_unchecked();
        let mut mock = MockControllerKubeClient::default();
        mock.node
            .expect_all()
            .return_once(|| Box::new(MockApi::new()));
        let mut instance_api_mock: MockApi<Instance> = MockApi::new();
        let instance_name = "config-a-359973";
        instance_api_mock
            .expect_list()
            .return_once(|_| instances_list(instance_name, "unused"));
        instance_api_mock
            .expect_raw_patch()
            .return_once(|_, _, _| Ok(Instance::new("unused", InstanceSpec::default())))
            .withf(|_, patch, _| match patch {
                Patch::Merge(v) => {
                    let instance: Instance = serde_json::from_value(v.clone()).unwrap();
                    !instance.spec.nodes.contains(&"node-a".to_owned())
                }
                _ => false,
            });
        instance_api_mock
            .expect_remove_finalizer()
            .returning(|_, _| Ok(()));
        mock.instance
            .expect_all()
            .return_once(move || Box::new(instance_api_mock));
        let ctx = Arc::new(ControllerContext::new(Arc::new(mock), "test"));
        ctx.known_nodes
            .write()
            .await
            .insert(node_name.clone(), NodeState::Running);
        reconcile_inner(Event::Apply(Arc::new(node)), ctx.clone())
            .await
            .unwrap();
        assert_eq!(
            &NodeState::InstancesCleaned,
            ctx.known_nodes.read().await.get(&node_name).unwrap()
        );
    }

    // If previously running node enters the cleanup state, it should remove the node from the instances' node lists
    // and ensure that the node is removed from the known_nodes
    #[tokio::test]
    async fn test_reconcile_node_cleanup() {
        let _ = env_logger::builder().is_test(true).try_init();
        let node_json = file::read_file_to_string("../test/json/node-a-not-ready.json");
        let node: Node = serde_json::from_str(&node_json).unwrap();
        let node_name = node.name_unchecked();
        let mut mock = MockControllerKubeClient::default();
        mock.node
            .expect_all()
            .return_once(|| Box::new(MockApi::new()));
        let mut instance_api_mock: MockApi<Instance> = MockApi::new();
        let instance_name = "config-a-359973";
        instance_api_mock
            .expect_list()
            .return_once(|_| instances_list(instance_name, "unused"));
        instance_api_mock
            .expect_raw_patch()
            .return_once(|_, _, _| Ok(Instance::new("unused", InstanceSpec::default())))
            .withf(|_, patch, _| match patch {
                Patch::Merge(v) => {
                    let instance: Instance = serde_json::from_value(v.clone()).unwrap();
                    !instance.spec.nodes.contains(&"node-a".to_owned())
                }
                _ => false,
            });
        instance_api_mock
            .expect_remove_finalizer()
            .returning(|_, _| Ok(()));
        mock.instance
            .expect_all()
            .return_once(move || Box::new(instance_api_mock));
        let ctx = Arc::new(ControllerContext::new(Arc::new(mock), "test"));
        ctx.known_nodes
            .write()
            .await
            .insert(node_name.clone(), NodeState::Running);
        reconcile_inner(Event::Cleanup(Arc::new(node)), ctx.clone())
            .await
            .unwrap();
        assert!(ctx.known_nodes.read().await.get(&node_name).is_none());
    }

    // If unknown node is deleted, it should remove the node from the instances' node lists
    #[tokio::test]
    async fn test_reconcile_node_cleanup_unknown() {
        let _ = env_logger::builder().is_test(true).try_init();
        let node_json = file::read_file_to_string("../test/json/node-a-not-ready.json");
        let node: Node = serde_json::from_str(&node_json).unwrap();
        let node_name = node.name_unchecked();
        let mut mock = MockControllerKubeClient::default();
        mock.node
            .expect_all()
            .return_once(|| Box::new(MockApi::new()));
        let mut instance_api_mock: MockApi<Instance> = MockApi::new();
        let instance_name = "config-a-359973";
        instance_api_mock
            .expect_list()
            .return_once(|_| instances_list(instance_name, "unused"));
        instance_api_mock
            .expect_raw_patch()
            .return_once(|_, _, _| Ok(Instance::new("unused", InstanceSpec::default())))
            .withf(|_, patch, _| match patch {
                Patch::Merge(v) => {
                    let instance: Instance = serde_json::from_value(v.clone()).unwrap();
                    !instance.spec.nodes.contains(&"node-a".to_owned())
                }
                _ => false,
            });
        instance_api_mock
            .expect_remove_finalizer()
            .returning(|_, _| Ok(()));
        mock.instance
            .expect_all()
            .return_once(move || Box::new(instance_api_mock));
        let ctx = Arc::new(ControllerContext::new(Arc::new(mock), "test"));
        reconcile_inner(Event::Cleanup(Arc::new(node)), ctx.clone())
            .await
            .unwrap();
        assert!(ctx.known_nodes.read().await.get(&node_name).is_none());
    }
}
