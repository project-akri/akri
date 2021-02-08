use akri_shared::{
    akri::{
        instance::{Instance, KubeAkriInstance},
        retry::{random_delay, MAX_INSTANCE_UPDATE_TRIES},
    },
    k8s,
    k8s::KubeInterface,
};
use futures::StreamExt;
use k8s_openapi::api::core::v1::{NodeSpec, NodeStatus};
use kube::api::{Api, Informer, Object, WatchEvent};
use log::trace;
use std::collections::HashMap;

type NodeObject = Object<NodeSpec, NodeStatus>;

/// Node states that NodeWatcher is interested in
///
/// NodeState describes the various states that the controller can
/// react to for Nodes.
#[derive(Clone, Debug, PartialEq)]
enum NodeState {
    /// Node has been seen, but not Running yet
    Known,
    /// Node has been seen Running
    Running,
    /// A previously Running Node has been seen as not Running
    /// and the Instances have been cleaned of references to that
    /// vanished Node
    InstancesCleaned,
}

/// This is used to handle Nodes disappearing.
///
/// When a Node disapears, make sure that any Instance that
/// references the Node is cleaned.  This means that the
/// Instance.nodes property no longer contains the node and
/// that the Instance.deviceUsage property no longer contains
/// slots that are occupied by the node.
pub struct NodeWatcher {
    known_nodes: HashMap<String, NodeState>,
}

impl NodeWatcher {
    /// Create new instance of BrokerPodWatcher
    pub fn new() -> Self {
        NodeWatcher {
            known_nodes: HashMap::new(),
        }
    }

    /// This watches for Node events
    pub async fn watch(
        &mut self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        trace!("watch - enter");
        let kube_interface = k8s::create_kube_interface();
        let resource = Api::v1Node(kube_interface.get_kube_client());
        let inf = Informer::new(resource.clone()).init().await?;

        loop {
            let mut nodes = inf.poll().await?.boxed();

            // Currently, this does not handle None except to break the
            // while.
            while let Some(event) = nodes.next().await {
                self.handle_node(event?, &kube_interface).await?;
            }
        }
    }

    /// This takes an event off the Node stream and if a Node is no longer
    /// available, it calls handle_node_disappearance.
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
    async fn handle_node(
        &mut self,
        event: WatchEvent<NodeObject>,
        kube_interface: &impl KubeInterface,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        trace!("handle_node - enter");
        match event {
            WatchEvent::Added(node) => {
                trace!("handle_node - Added: {:?}", &node.metadata.name);
                if self.is_node_ready(&node) {
                    self.known_nodes
                        .insert(node.metadata.name, NodeState::Running);
                } else {
                    self.known_nodes
                        .insert(node.metadata.name, NodeState::Known);
                }
            }
            WatchEvent::Modified(node) => {
                trace!("handle_node - Modified: {:?}", &node.metadata.name);
                if self.is_node_ready(&node) {
                    self.known_nodes
                        .insert(node.metadata.name.clone(), NodeState::Running);
                } else {
                    self.call_handle_node_disappearance_if_needed(&node, kube_interface)
                        .await?;
                }
            }
            WatchEvent::Deleted(node) => {
                trace!("handle_node - Deleted: {:?}", &node.metadata.name);
                self.call_handle_node_disappearance_if_needed(&node, kube_interface)
                    .await?;
            }
            WatchEvent::Error(e) => {
                trace!("handle_node - error for Node: {}", e);
            }
        };
        Ok(())
    }

    /// This should be called for Nodes that are either !Ready or Deleted.
    /// This function ensures that handle_node_disappearance is called
    /// only once for any Node as it disappears.
    async fn call_handle_node_disappearance_if_needed(
        &mut self,
        node: &NodeObject,
        kube_interface: &impl KubeInterface,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        trace!(
            "call_handle_node_disappearance_if_needed - enter: {:?}",
            &node.metadata.name
        );
        let last_known_state = self
            .known_nodes
            .get(&node.metadata.name)
            .unwrap_or(&NodeState::Running);
        trace!(
            "call_handle_node_disappearance_if_needed - last_known_state: {:?}",
            &last_known_state
        );
        // Nodes are updated roughly once a minute ... try to only call
        // handle_node_disappearance once for a node that disappears.
        //
        // Also, there is no need to call handle_node_disappearance if a
        // Node has never been in the Running state.
        if last_known_state == &NodeState::Running {
            trace!(
                "call_handle_node_disappearance_if_needed - call handle_node_disappearance: {:?}",
                &node.metadata.name
            );
            self.handle_node_disappearance(&node.metadata.name, kube_interface)
                .await?;
            self.known_nodes
                .insert(node.metadata.name.clone(), NodeState::InstancesCleaned);
        }
        Ok(())
    }

    /// This determines if a node is in the Ready state.
    fn is_node_ready(&self, k8s_node: &NodeObject) -> bool {
        k8s_node
            .status
            .as_ref()
            .unwrap_or(&NodeStatus::default())
            .conditions
            .as_ref()
            .unwrap_or(&Vec::new())
            .iter()
            .filter_map(|condition| {
                if condition.type_ == "Ready" {
                    Some(condition.status == "True")
                } else {
                    None
                }
            })
            .collect::<Vec<bool>>()
            .last()
            .unwrap_or(&false)
            == &true
    }

    /// This handles when a node disappears by clearing nodes from
    /// the nodes list and deviceUsage map and then trying 5 times to
    /// update the Instance.
    async fn handle_node_disappearance(
        &self,
        vanished_node_name: &str,
        kube_interface: &impl KubeInterface,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        trace!(
            "handle_node_disappearance - enter vanished_node_name={:?}",
            vanished_node_name,
        );

        let instances = kube_interface.get_instances().await?;
        trace!(
            "handle_node_disappearance - found {:?} instances",
            instances.items.len()
        );
        for instance in instances.items {
            let instance_name = instance.metadata.name.clone();
            let instance_namespace = instance.metadata.namespace.as_ref().ok_or(format!(
                "Namespace not found for instance: {}",
                instance_name
            ))?;

            trace!(
                "handle_node_disappearance - make sure node is not referenced here: {:?}",
                &instance_name
            );

            // Try up to MAX_INSTANCE_UPDATE_TRIES times to update/create/get instance
            for x in 0..MAX_INSTANCE_UPDATE_TRIES {
                match if x == 0 {
                    self.try_remove_nodes_from_instance(
                        &vanished_node_name,
                        &instance_name,
                        &instance_namespace,
                        &instance,
                        kube_interface,
                    )
                    .await
                } else {
                    let retry_instance = kube_interface
                        .find_instance(&instance_name, &instance_namespace)
                        .await?;
                    self.try_remove_nodes_from_instance(
                        &vanished_node_name,
                        &instance_name,
                        &instance_namespace,
                        &retry_instance,
                        kube_interface,
                    )
                    .await
                } {
                    Ok(_) => break,
                    Err(e) => {
                        if x == (MAX_INSTANCE_UPDATE_TRIES - 1) {
                            return Err(e);
                        }
                        random_delay().await;
                    }
                }
            }
        }

        trace!("handle_node_disappearance - exit");
        Ok(())
    }

    /// This attempts to remove nodes from the nodes list and deviceUsage
    /// map in an Instance.  An attempt is made to update
    /// the instance in etcd, any failure is returned.
    async fn try_remove_nodes_from_instance(
        &self,
        vanished_node_name: &str,
        instance_name: &str,
        instance_namespace: &str,
        instance: &KubeAkriInstance,
        kube_interface: &impl KubeInterface,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
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
            .map(|(slot, node)| {
                (
                    slot.to_string(),
                    if vanished_node_name == node {
                        "".into()
                    } else {
                        node.into()
                    },
                )
            })
            .collect::<HashMap<String, String>>();

        // Save the instance
        let modified_instance = Instance {
            configuration_name: instance.spec.configuration_name.clone(),
            metadata: instance.spec.metadata.clone(),
            rbac: instance.spec.rbac.clone(),
            shared: instance.spec.shared,
            device_usage: modified_device_usage,
            nodes: modified_nodes,
        };

        trace!(
            "handle_node_disappearance - kube_interface.update_instance name: {}, namespace: {}, {:?}",
            &instance_name,
            &instance_namespace,
            &modified_instance
        );

        kube_interface
            .update_instance(&modified_instance, &instance_name, &instance_namespace)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::super::shared_test_utils::config_for_tests;
    use super::*;
    use akri_shared::{akri::instance::KubeAkriInstanceList, k8s::MockKubeInterface, os::file};

    #[derive(Clone)]
    struct UpdateInstance {
        instance_to_update: Instance,
        instance_name: &'static str,
        instance_namespace: &'static str,
    }

    #[derive(Clone)]
    struct HandleNodeDisappearance {
        get_instances_result_file: &'static str,
        get_instances_result_listify: bool,
        update_instance: Option<UpdateInstance>,
    }

    fn configure_for_handle_node_disappearance(
        mock: &mut MockKubeInterface,
        work: &HandleNodeDisappearance,
    ) {
        config_for_tests::configure_get_instances(
            mock,
            work.get_instances_result_file,
            work.get_instances_result_listify,
        );

        if let Some(update_instance) = &work.update_instance {
            config_for_tests::configure_update_instance(
                mock,
                update_instance.instance_to_update.clone(),
                update_instance.instance_name,
                update_instance.instance_namespace,
                false,
            );
        }
    }

    #[tokio::test]
    async fn test_handle_node_added_unready() {
        let _ = env_logger::builder().is_test(true).try_init();
        let node_json = file::read_file_to_string("../test/json/node-a-not-ready.json");
        let node: NodeObject = serde_json::from_str(&node_json).unwrap();
        let mut node_watcher = NodeWatcher::new();
        node_watcher
            .handle_node(WatchEvent::Added(node), &MockKubeInterface::new())
            .await
            .unwrap();

        assert_eq!(1, node_watcher.known_nodes.len());

        assert_eq!(
            &NodeState::Known,
            node_watcher.known_nodes.get(&"node-a".to_string()).unwrap()
        )
    }

    #[tokio::test]
    async fn test_handle_node_added_ready() {
        let _ = env_logger::builder().is_test(true).try_init();

        let node_json = file::read_file_to_string("../test/json/node-a.json");
        let node: NodeObject = serde_json::from_str(&node_json).unwrap();
        let mut node_watcher = NodeWatcher::new();
        node_watcher
            .handle_node(WatchEvent::Added(node), &MockKubeInterface::new())
            .await
            .unwrap();

        assert_eq!(1, node_watcher.known_nodes.len());

        assert_eq!(
            &NodeState::Running,
            node_watcher.known_nodes.get(&"node-a".to_string()).unwrap()
        )
    }

    #[tokio::test]
    async fn test_handle_node_modified_unready_unknown() {
        let _ = env_logger::builder().is_test(true).try_init();

        let node_json = file::read_file_to_string("../test/json/node-b-not-ready.json");
        let node: NodeObject = serde_json::from_str(&node_json).unwrap();
        let mut node_watcher = NodeWatcher::new();

        let instance_file = "../test/json/shared-instance-update.json";
        let instance_json = file::read_file_to_string(instance_file);
        let kube_object_instance: KubeAkriInstance = serde_json::from_str(&instance_json).unwrap();
        let mut instance = kube_object_instance.spec;
        instance.nodes.clear();
        instance
            .device_usage
            .insert("config-a-359973-2".to_string(), "".to_string());

        let mut mock = MockKubeInterface::new();
        configure_for_handle_node_disappearance(
            &mut mock,
            &HandleNodeDisappearance {
                get_instances_result_file: "../test/json/shared-instance-update.json",
                get_instances_result_listify: true,
                update_instance: Some(UpdateInstance {
                    instance_to_update: instance,
                    instance_name: "config-a-359973",
                    instance_namespace: "config-a-namespace",
                }),
            },
        );

        node_watcher
            .handle_node(WatchEvent::Modified(node), &mock)
            .await
            .unwrap();

        assert_eq!(1, node_watcher.known_nodes.len());

        assert_eq!(
            &NodeState::InstancesCleaned,
            node_watcher.known_nodes.get(&"node-b".to_string()).unwrap()
        )
    }

    #[tokio::test]
    async fn test_handle_node_modified_ready_unknown() {
        let _ = env_logger::builder().is_test(true).try_init();

        let node_json = file::read_file_to_string("../test/json/node-b.json");
        let node: NodeObject = serde_json::from_str(&node_json).unwrap();
        let mut node_watcher = NodeWatcher::new();

        let mock = MockKubeInterface::new();
        node_watcher
            .handle_node(WatchEvent::Modified(node), &mock)
            .await
            .unwrap();

        assert_eq!(1, node_watcher.known_nodes.len());

        assert_eq!(
            &NodeState::Running,
            node_watcher.known_nodes.get(&"node-b".to_string()).unwrap()
        )
    }

    #[tokio::test]
    async fn test_handle_node_deleted_unready_unknown() {
        let _ = env_logger::builder().is_test(true).try_init();

        let node_json = file::read_file_to_string("../test/json/node-b-not-ready.json");
        let node: NodeObject = serde_json::from_str(&node_json).unwrap();
        let mut node_watcher = NodeWatcher::new();

        let instance_file = "../test/json/shared-instance-update.json";
        let instance_json = file::read_file_to_string(instance_file);
        let kube_object_instance: KubeAkriInstance = serde_json::from_str(&instance_json).unwrap();
        let mut instance = kube_object_instance.spec;
        instance.nodes.clear();
        instance
            .device_usage
            .insert("config-a-359973-2".to_string(), "".to_string());

        let mut mock = MockKubeInterface::new();
        configure_for_handle_node_disappearance(
            &mut mock,
            &HandleNodeDisappearance {
                get_instances_result_file: "../test/json/shared-instance-update.json",
                get_instances_result_listify: true,
                update_instance: Some(UpdateInstance {
                    instance_to_update: instance,
                    instance_name: "config-a-359973",
                    instance_namespace: "config-a-namespace",
                }),
            },
        );

        node_watcher
            .handle_node(WatchEvent::Deleted(node), &mock)
            .await
            .unwrap();

        assert_eq!(1, node_watcher.known_nodes.len());

        assert_eq!(
            &NodeState::InstancesCleaned,
            node_watcher.known_nodes.get(&"node-b".to_string()).unwrap()
        )
    }

    const LIST_PREFIX: &str = r#"
{
    "apiVersion": "v1",
    "items": ["#;
    const LIST_SUFFIX: &str = r#"
    ],
    "kind": "List",
    "metadata": {
        "resourceVersion": "",
        "selfLink": ""
    }
}"#;
    fn listify_node(node_json: &String) -> String {
        format!("{}\n{}\n{}", LIST_PREFIX, node_json, LIST_SUFFIX)
    }

    #[tokio::test]
    async fn test_handle_node_disappearance_update_failure_retries() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mut mock = MockKubeInterface::new();
        mock.expect_get_instances().times(1).returning(move || {
            let instance_file = "../test/json/shared-instance-update.json";
            let instance_json = file::read_file_to_string(instance_file);
            let instance_list_json = listify_node(&instance_json);
            let list: KubeAkriInstanceList = serde_json::from_str(&instance_list_json).unwrap();
            Ok(list)
        });
        mock.expect_update_instance()
            .times(MAX_INSTANCE_UPDATE_TRIES as usize)
            .withf(move |_instance, n, ns| n == "config-a-359973" && ns == "config-a-namespace")
            .returning(move |_, _, _| Err(None.ok_or("failure")?));
        mock.expect_find_instance()
            .times((MAX_INSTANCE_UPDATE_TRIES - 1) as usize)
            .withf(move |n, ns| n == "config-a-359973" && ns == "config-a-namespace")
            .returning(move |_, _| {
                let instance_file = "../test/json/shared-instance-update.json";
                let instance_json = file::read_file_to_string(instance_file);
                let instance: KubeAkriInstance = serde_json::from_str(&instance_json).unwrap();
                Ok(instance)
            });

        let node_watcher = NodeWatcher::new();
        assert!(node_watcher
            .handle_node_disappearance(&"foo-a", &mock,)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn test_try_remove_nodes_from_instance() {
        let _ = env_logger::builder().is_test(true).try_init();

        let instance_file = "../test/json/shared-instance-update.json";
        let instance_json = file::read_file_to_string(instance_file);
        let kube_object_instance: KubeAkriInstance = serde_json::from_str(&instance_json).unwrap();

        let mut mock = MockKubeInterface::new();
        mock.expect_update_instance()
            .times(1)
            .withf(move |ins, n, ns| {
                n == "config-a"
                    && ns == "config-a-namespace"
                    && !ins.nodes.contains(&"node-b".to_string())
                    && ins
                        .device_usage
                        .iter()
                        .filter_map(|(_slot, value)| {
                            if value == &"node-b".to_string() {
                                Some(value.to_string())
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<String>>()
                        .first()
                        .is_none()
            })
            .returning(move |_, _, _| Ok(()));

        let node_watcher = NodeWatcher::new();
        assert!(node_watcher
            .try_remove_nodes_from_instance(
                "node-b",
                "config-a",
                "config-a-namespace",
                &kube_object_instance,
                &mock,
            )
            .await
            .is_ok());
    }

    #[test]
    fn test_is_node_ready_ready() {
        let _ = env_logger::builder().is_test(true).try_init();

        let tests = [
            ("../test/json/node-a.json", true),
            ("../test/json/node-a-not-ready.json", false),
            ("../test/json/node-a-no-conditions.json", false),
            ("../test/json/node-a-no-ready-condition.json", false),
        ];

        for (node_file, result) in tests.iter() {
            trace!(
                "Testing {} should reflect node is ready={}",
                node_file,
                result
            );

            let node_json = file::read_file_to_string(node_file);
            let kube_object_node: Object<NodeSpec, NodeStatus> =
                serde_json::from_str(&node_json).unwrap();

            let node_watcher = NodeWatcher::new();
            assert_eq!(
                result.clone(),
                node_watcher.is_node_ready(&kube_object_node)
            );
        }
    }
}
