use super::{constants::SLOT_RECONCILIATION_CHECK_DELAY_SECS, crictl_containers};
use akri_shared::akri::instance::device_usage::NodeUsage;
use akri_shared::{akri::instance::InstanceSpec, k8s::KubeInterface};
use async_trait::async_trait;
use k8s_openapi::api::core::v1::PodStatus;
#[cfg(test)]
use mockall::{automock, predicate::*};
use std::{
    collections::{HashMap, HashSet},
    str::FromStr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::process::Command;

type SlotQueryResult =
    Result<HashMap<String, NodeUsage>, Box<dyn std::error::Error + Send + Sync + 'static>>;

#[cfg_attr(test, automock)]
#[async_trait]
pub trait SlotQuery {
    async fn get_node_slots(&self) -> SlotQueryResult;
}

/// Discovers which of an instance's usage slots are actively used by containers on this node
pub struct CriCtlSlotQuery {
    pub crictl_path: String,
    pub runtime_endpoint: String,
    pub image_endpoint: String,
}

#[async_trait]
impl SlotQuery for CriCtlSlotQuery {
    /// Calls crictl to query container runtime in search of active containers and extracts their usage slots.
    async fn get_node_slots(&self) -> SlotQueryResult {
        match Command::new(&self.crictl_path)
            .args([
                "--runtime-endpoint",
                &self.runtime_endpoint,
                "--image-endpoint",
                &self.image_endpoint,
                "ps",
                "-v",
                "--output",
                "json",
            ])
            .output()
            .await
        {
            Ok(output) => {
                if output.status.success() {
                    trace!("get_node_slots - crictl called successfully");
                    let output_string = String::from_utf8_lossy(&output.stdout);
                    Ok(crictl_containers::get_container_slot_usage(&output_string))
                } else {
                    let output_string = String::from_utf8_lossy(&output.stderr);
                    Err(None.ok_or(format!(
                        "get_node_slots - Failed to call crictl: {:?}",
                        output_string
                    ))?)
                }
            }
            Err(e) => {
                trace!("get_node_slots - Command failed to call crictl: {:?}", e);
                Err(e.into())
            }
        }
    }
}

/// Makes sure Instance's `device_usage` accurately reflects actual usage.
pub struct DevicePluginSlotReconciler {
    pub removal_slot_map: Arc<Mutex<HashMap<String, Instant>>>,
}

impl DevicePluginSlotReconciler {
    pub async fn reconcile(
        &self,
        node_name: &str,
        slot_grace_period: Duration,
        slot_query: &impl SlotQuery,
        kube_interface: &impl KubeInterface,
    ) {
        trace!(
            "reconcile - thread iteration start [{:?}]",
            self.removal_slot_map
        );

        let node_slot_usage = match slot_query.get_node_slots().await {
            Ok(usage) => usage,
            Err(e) => {
                trace!("reconcile - get_node_slots failed: {:?}", e);
                // If an error occurs in the crictl call, return early
                // to avoid treating this error like crictl found no
                // active containers.  Currently, reconcile is a best
                // effort approach.
                return;
            }
        };
        trace!(
            "reconcile - slots currently in use on this node: {:?}",
            node_slot_usage
        );

        // Any slot found in use should be scrubbed from our list
        {
            let mut removal_slot_map_guard = self.removal_slot_map.lock().unwrap();
            node_slot_usage.iter().for_each(|(slot, _)| {
                trace!("reconcile - remove slot from tracked slots: {:?}", slot);
                removal_slot_map_guard.remove(slot);
            });
        }
        trace!(
            "reconcile - removal_slot_map after removing node_slot_usage: {:?}",
            self.removal_slot_map
        );

        let instances = match kube_interface.get_instances().await {
            Ok(instances) => instances,
            Err(e) => {
                trace!("reconcile - Failed to get instances: {:?}", e);
                return;
            }
        };

        let pods = match kube_interface
            .find_pods_with_field(&format!("{}={}", "spec.nodeName", &node_name,))
            .await
        {
            Ok(pods) => {
                trace!("reconcile - found {} pods on this node", pods.items.len());
                pods
            }
            Err(e) => {
                trace!("reconcile - error finding pending pods: {}", e);
                return;
            }
        };

        // Check to see if there are any Pods on this Node that have
        // Containers that are not ready. If there are any, we should
        // wait for the Containers to be ready before cleaning any
        // Instance device_usage
        let any_unready_pods = pods.items.iter().any(|pod| {
            pod.status
                .as_ref()
                .unwrap_or(&PodStatus::default())
                .conditions
                .as_ref()
                .unwrap_or(&Vec::new())
                .iter()
                .any(|condition| {
                    condition.type_ == "ContainersReady"
                        && condition.status != "True"
                        && condition.reason != Some("PodCompleted".to_string())
                })
        });
        if any_unready_pods {
            trace!("reconcile - Pods with unready Containers exist on this node, we can't clean the slots yet");
            return;
        }

        for instance in instances {
            // Check Instance against list of slots that are being used by this node's
            // current pods.  If we find any missing, we should update the Instance for
            // the actual slot usage.
            let slots_missing_this_node_name = instance
                .spec
                .device_usage
                .iter()
                .filter_map(|(k, v)| {
                    let same_node_name = match NodeUsage::from_str(v) {
                        Ok(node_usage) => node_usage.is_same_node(node_name),
                        Err(_) => false,
                    };
                    if !same_node_name {
                        // We need to add node_name to this slot IF
                        //     the slot is not labeled with node_name AND
                        //     there is a container using that slot on this node
                        node_slot_usage
                            .get_key_value(k)
                            .map(|(slot, node_usage)| (slot.to_string(), node_usage.clone()))
                    } else {
                        None
                    }
                })
                .collect::<HashMap<String, NodeUsage>>();

            // Check Instance to find slots that are registered to this node, but
            // there is no actual pod using the slot.  We should update the Instance
            // to clear the false usage.
            //
            // For slots that need to be cleaned, we should wait for a "grace
            // period" prior to updating the Instance.
            let slots_to_clean = instance
                .spec
                .device_usage
                .iter()
                .filter_map(|(k, v)| {
                    let same_node_name = match NodeUsage::from_str(v) {
                        Ok(usage) => usage.is_same_node(node_name),
                        Err(_) => false,
                    };
                    if same_node_name && !node_slot_usage.contains_key(k) {
                        // We need to clean this slot IF
                        //     this slot is handled by this node AND
                        //     there are no containers using that slot on this node
                        Some(k.to_string())
                    } else {
                        None
                    }
                })
                .filter(|slot_string| {
                    let mut local_slot_map = self.removal_slot_map.lock().unwrap();
                    if let Some(time) = local_slot_map.get(slot_string) {
                        let now = Instant::now();
                        match now.checked_duration_since(*time) {
                            Some(duration) => {
                                if duration > slot_grace_period {
                                    trace!("reconcile - slot expired: [{:?}]", duration);
                                    true // slot has been unoccupied beyond the grace period
                                } else {
                                    false // still in grace period
                                }
                            }
                            None => {
                                false // still in grace period
                            }
                        }
                    } else {
                        trace!("reconcile - slot added to list: [Now]");
                        local_slot_map.insert(slot_string.to_string(), Instant::now());
                        false // do not remove this node just yet
                    }
                })
                .collect::<HashSet<String>>();
            trace!(
                "reconcile - these slots have no pods according to crictl AND have expired: {:?}",
                &slots_to_clean
            );

            if !slots_to_clean.is_empty() || !slots_missing_this_node_name.is_empty() {
                trace!(
                    "reconcile - update Instance slots_to_clean: {:?}  slots_missing_this_node_name: {:?}",
                    slots_to_clean,
                    slots_missing_this_node_name
                );
                let modified_device_usage = instance
                    .spec
                    .device_usage
                    .iter()
                    .map(|(slot, usage)| {
                        (
                            slot.to_string(),
                            if slots_missing_this_node_name.contains_key(slot) {
                                // Restore usage because there have been
                                // cases where a Pod is running (which corresponds
                                // to an Allocate call, but the Instance slot is empty.
                                slots_missing_this_node_name.get(slot).unwrap().to_string()
                            } else if slots_to_clean.contains(slot) {
                                // Set usage to free because there is no
                                // Deallocate message from kubelet for us to know
                                // when a slot is no longer in use
                                NodeUsage::default().to_string()
                            } else {
                                // This slot remains unchanged.
                                usage.into()
                            },
                        )
                    })
                    .collect::<HashMap<String, String>>();
                let modified_instance = InstanceSpec {
                    configuration_name: instance.spec.configuration_name.clone(),
                    broker_properties: instance.spec.broker_properties.clone(),
                    shared: instance.spec.shared,
                    device_usage: modified_device_usage,
                    nodes: instance.spec.nodes.clone(),
                };
                trace!("reconcile - update Instance from: {:?}", &instance.spec);
                trace!("reconcile - update Instance   to: {:?}", &modified_instance);
                match kube_interface
                    .update_instance(
                        &modified_instance,
                        &instance.metadata.name.unwrap(),
                        &instance.metadata.namespace.unwrap(),
                    )
                    .await
                {
                    Ok(()) => {
                        slots_to_clean.iter().for_each(|slot| {
                            trace!("reconcile - remove {} from removal_slot_map", slot);
                            self.removal_slot_map.lock().unwrap().remove(slot);
                        });
                    }
                    Err(e) => {
                        // If update fails, let the next iteration update the Instance.  We
                        // may want to revisit this decision and add some retry logic
                        // here.
                        trace!("reconcile - update Instance failed: {:?}", e);
                    }
                }
            }
        }

        trace!("reconcile - thread iteration end");
    }
}

/// This periodically checks to make sure that all Instances' device_usage
/// accurately reflects the actual usage.
///
/// The Kubernetes Device-Plugin implementation has no notifications for
/// when a Pod disappears (which should, in turn, free up a slot).  Because
/// of this, if a Pod disappears, there will be a slot that Akri (and the
/// Kubernetes scheduler) falsely thinks is in use.
///
/// To work around this, we have done 2 things:
///   1. Each of Agent's device plugins add slot information to the Annotations
///      section of the Allocate response.
///   2. periodic_slot_reconciliation will periodically call crictl to query the
///      container runtime in search of active Containers that have our slot
///      Annotations.  This function will make sure that our Instance device_usage
///      accurately reflects the actual usage.
///
/// It has rarely been seen, perhaps due to connectivity issues, that active
/// Containers with our Annotation are no longer in our Instance.  This is a bug that
/// we are aware of, but haven't found yet.  To address this, until a fix is found,
/// we will also make sure that any Container that exists with our Annotation will
/// be shown in our Instance device_usage.
pub async fn periodic_slot_reconciliation(
    slot_grace_period: std::time::Duration,
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    trace!("periodic_slot_reconciliation - start");
    let kube_interface = akri_shared::k8s::KubeImpl::new().await?;
    let node_name = std::env::var("AGENT_NODE_NAME").unwrap();
    let crictl_path = std::env::var("HOST_CRICTL_PATH").unwrap();
    let runtime_endpoint = std::env::var("HOST_RUNTIME_ENDPOINT").unwrap();
    let image_endpoint = std::env::var("HOST_IMAGE_ENDPOINT").unwrap();

    let reconciler = DevicePluginSlotReconciler {
        removal_slot_map: Arc::new(std::sync::Mutex::new(HashMap::new())),
    };
    let slot_query = CriCtlSlotQuery {
        crictl_path,
        runtime_endpoint,
        image_endpoint,
    };

    loop {
        trace!("periodic_slot_reconciliation - iteration pre sleep");
        tokio::time::sleep(std::time::Duration::from_secs(
            SLOT_RECONCILIATION_CHECK_DELAY_SECS,
        ))
        .await;

        trace!("periodic_slot_reconciliation - iteration call reconiler.reconcile");
        reconciler
            .reconcile(&node_name, slot_grace_period, &slot_query, &kube_interface)
            .await;

        trace!("periodic_slot_reconciliation - iteration end");
    }
}

#[cfg(test)]
mod reconcile_tests {
    use super::*;
    use akri_shared::akri::instance::device_usage::DeviceUsageKind;
    use akri_shared::{akri::instance::InstanceList, k8s::MockKubeInterface, os::file};
    use k8s_openapi::api::core::v1::Pod;
    use kube::api::ObjectList;

    fn configure_get_node_slots(
        mock: &mut MockSlotQuery,
        result: HashMap<String, NodeUsage>,
        error: bool,
    ) {
        mock.expect_get_node_slots().times(1).returning(move || {
            if !error {
                Ok(result.clone())
            } else {
                Err(None.ok_or("failure")?)
            }
        });
    }

    fn configure_get_instances(mock: &mut MockKubeInterface, result_file: &'static str) {
        mock.expect_get_instances().times(1).returning(move || {
            let instance_list_json = file::read_file_to_string(result_file);
            let instance_list: InstanceList = serde_json::from_str(&instance_list_json).unwrap();
            Ok(instance_list)
        });
    }

    fn configure_find_pods_with_field(
        mock: &mut MockKubeInterface,
        selector: &'static str,
        result_file: &'static str,
    ) {
        mock.expect_find_pods_with_field()
            .times(1)
            .withf(move |s| s == selector)
            .returning(move |_| {
                let pods_json = file::read_file_to_string(result_file);
                let pods: ObjectList<Pod> = serde_json::from_str(&pods_json).unwrap();
                Ok(pods)
            });
    }

    struct NodeSlots {
        node_slots: HashMap<String, NodeUsage>,
        node_slots_error: bool,
    }

    struct UpdateInstance {
        expected_slot_1_node: &'static str,
        expected_slot_5_node: &'static str,
    }

    async fn configure_scnenario(
        node_slots: NodeSlots,
        instances_result_file: &'static str,
        update_instance: Option<UpdateInstance>,
        grace_period: Duration,
        reconciler: &DevicePluginSlotReconciler,
    ) {
        let mut slot_query = MockSlotQuery::new();
        // slot_query to identify one slot used by this node
        configure_get_node_slots(
            &mut slot_query,
            node_slots.node_slots,
            node_slots.node_slots_error,
        );

        let mut kube_interface = MockKubeInterface::new();
        if !node_slots.node_slots_error {
            // kube_interface to find Instance with node-a using slots:
            //    config-a-359973-1 & config-a-359973-3
            configure_get_instances(&mut kube_interface, instances_result_file);
            // kube_interface to find no pods with unready containers
            configure_find_pods_with_field(
                &mut kube_interface,
                "spec.nodeName=node-a",
                "../test/json/running-pod-list-for-config-a-shared.json",
            );
            if let Some(update_instance_) = update_instance {
                trace!(
                    "expect_update_instance - slot1: {}, slot5: {}",
                    update_instance_.expected_slot_1_node,
                    update_instance_.expected_slot_5_node
                );
                // kube_interface to update Instance
                kube_interface
                    .expect_update_instance()
                    .times(1)
                    .withf(move |instance, name, namespace| {
                        name == "config-a-359973"
                            && namespace == "config-a-namespace"
                            && instance.nodes.len() == 3
                            && instance.nodes.contains(&"node-a".to_string())
                            && instance.nodes.contains(&"node-b".to_string())
                            && instance.nodes.contains(&"node-c".to_string())
                            && instance.device_usage["config-a-359973-0"] == "node-b"
                            && instance.device_usage["config-a-359973-1"]
                                == update_instance_.expected_slot_1_node
                            && instance.device_usage["config-a-359973-2"] == "node-b"
                            && instance.device_usage["config-a-359973-3"] == "node-a"
                            && instance.device_usage["config-a-359973-4"] == "node-c"
                            && instance.device_usage["config-a-359973-5"]
                                == update_instance_.expected_slot_5_node
                    })
                    .returning(move |_, _, _| Ok(()));
            }
        }

        reconciler
            .reconcile("node-a", grace_period, &slot_query, &kube_interface)
            .await;
    }

    #[tokio::test]
    async fn test_reconcile_no_slots_to_reconcile() {
        let _ = env_logger::builder().is_test(true).try_init();

        let reconciler = DevicePluginSlotReconciler {
            removal_slot_map: Arc::new(Mutex::new(HashMap::new())),
        };
        configure_scnenario(
            NodeSlots {
                node_slots: HashMap::new(),
                node_slots_error: false,
            },
            "../test/json/shared-instance-list.json",
            None,
            Duration::from_secs(10),
            &reconciler,
        )
        .await;
    }

    #[tokio::test]
    async fn test_reconcile_get_slots_error() {
        let _ = env_logger::builder().is_test(true).try_init();

        let reconciler = DevicePluginSlotReconciler {
            removal_slot_map: Arc::new(Mutex::new(HashMap::new())),
        };
        configure_scnenario(
            NodeSlots {
                node_slots: HashMap::new(),
                node_slots_error: true,
            },
            "",
            None,
            Duration::from_secs(10),
            &reconciler,
        )
        .await;
    }

    #[tokio::test]
    async fn test_reconcile_slots_to_add() {
        let _ = env_logger::builder().is_test(true).try_init();

        let reconciler = DevicePluginSlotReconciler {
            removal_slot_map: Arc::new(Mutex::new(HashMap::new())),
        };

        let grace_period = Duration::from_millis(100);
        let mut node_slots = HashMap::new();
        node_slots.insert(
            "config-a-359973-3".to_string(),
            NodeUsage::create(&DeviceUsageKind::Instance, "node-a").unwrap(),
        );
        node_slots.insert(
            "config-a-359973-5".to_string(),
            NodeUsage::create(&DeviceUsageKind::Instance, "node-a").unwrap(),
        );
        configure_scnenario(
            // slot_query to identify one slot used by this node
            NodeSlots {
                node_slots,
                node_slots_error: false,
            },
            // kube_interface to find Instance with node-a using slots:
            //    config-a-359973-1 & config-a-359973-3
            "../test/json/shared-instance-list-slots.json",
            Some(UpdateInstance {
                expected_slot_1_node: "node-a",
                expected_slot_5_node: "node-a",
            }),
            grace_period,
            &reconciler,
        )
        .await;

        // Validate that the slot has been added to the list of "to be removed slots"
        assert!(reconciler.removal_slot_map.lock().unwrap().len() == 1);
        assert!(reconciler
            .removal_slot_map
            .lock()
            .unwrap()
            .contains_key("config-a-359973-1"));
    }

    #[tokio::test]
    async fn test_reconcile_slots_to_delete() {
        let _ = env_logger::builder().is_test(true).try_init();

        let reconciler = DevicePluginSlotReconciler {
            removal_slot_map: Arc::new(Mutex::new(HashMap::new())),
        };

        let grace_period = Duration::from_millis(100);
        let mut node_slots = HashMap::new();
        node_slots.insert(
            "config-a-359973-3".to_string(),
            NodeUsage::create(&DeviceUsageKind::Instance, "node-a").unwrap(),
        );
        configure_scnenario(
            // slot_query to identify one slot used by this node
            NodeSlots {
                node_slots: node_slots.clone(),
                node_slots_error: false,
            },
            // kube_interface to find Instance with node-a using slots:
            //    config-a-359973-1 & config-a-359973-3
            "../test/json/shared-instance-list-slots.json",
            None,
            grace_period,
            &reconciler,
        )
        .await;

        // Validate that the slot has been added to the list of "to be removed slots"
        assert!(reconciler.removal_slot_map.lock().unwrap().len() == 1);
        assert!(reconciler
            .removal_slot_map
            .lock()
            .unwrap()
            .contains_key("config-a-359973-1"));

        // Wait for more than the grace period ... it short, so, just wait twice :)
        std::thread::sleep(grace_period);
        std::thread::sleep(grace_period);

        configure_scnenario(
            // slot_query to identify one slot used by this node
            NodeSlots {
                node_slots: node_slots.clone(),
                node_slots_error: false,
            },
            // kube_interface to find Instance with node-a using slots:
            //    config-a-359973-1 & config-a-359973-3
            "../test/json/shared-instance-list-slots.json",
            Some(UpdateInstance {
                expected_slot_1_node: "",
                expected_slot_5_node: "",
            }),
            grace_period,
            &reconciler,
        )
        .await;

        // Validate that the slot has been added to the list of "to be removed slots"
        assert!(reconciler.removal_slot_map.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_reconcile_slots_to_delete_and_add() {
        let _ = env_logger::builder().is_test(true).try_init();

        let reconciler = DevicePluginSlotReconciler {
            removal_slot_map: Arc::new(Mutex::new(HashMap::new())),
        };

        let grace_period = Duration::from_millis(100);
        let mut node_slots = HashMap::new();
        node_slots.insert(
            "config-a-359973-3".to_string(),
            NodeUsage::create(&DeviceUsageKind::Instance, "node-a").unwrap(),
        );
        configure_scnenario(
            // slot_query to identify one slot used by this node
            NodeSlots {
                node_slots,
                node_slots_error: false,
            },
            // kube_interface to find Instance with node-a using slots:
            //    config-a-359973-1 & config-a-359973-3
            "../test/json/shared-instance-list-slots.json",
            None,
            grace_period,
            &reconciler,
        )
        .await;

        // Validate that the slot has been added to the list of "to be removed slots"
        assert!(reconciler.removal_slot_map.lock().unwrap().len() == 1);
        assert!(reconciler
            .removal_slot_map
            .lock()
            .unwrap()
            .contains_key("config-a-359973-1"));

        // Wait for more than the grace period ... it short, so, just wait twice :)
        std::thread::sleep(grace_period);
        std::thread::sleep(grace_period);

        let mut node_slots_added = HashMap::new();
        node_slots_added.insert(
            "config-a-359973-3".to_string(),
            NodeUsage::create(&DeviceUsageKind::Instance, "node-a").unwrap(),
        );
        node_slots_added.insert(
            "config-a-359973-5".to_string(),
            NodeUsage::create(&DeviceUsageKind::Instance, "node-a").unwrap(),
        );
        configure_scnenario(
            // slot_query to identify one slot used by this node
            NodeSlots {
                node_slots: node_slots_added,
                node_slots_error: false,
            },
            // kube_interface to find Instance with node-a using slots:
            //    config-a-359973-1 & config-a-359973-3
            "../test/json/shared-instance-list-slots.json",
            Some(UpdateInstance {
                expected_slot_1_node: "",
                expected_slot_5_node: "node-a",
            }),
            grace_period,
            &reconciler,
        )
        .await;

        // Validate that the slot has been added to the list of "to be removed slots"
        assert!(reconciler.removal_slot_map.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_reconcile_slots_to_delete_only_temporarily() {
        let _ = env_logger::builder().is_test(true).try_init();

        let reconciler = DevicePluginSlotReconciler {
            removal_slot_map: Arc::new(Mutex::new(HashMap::new())),
        };

        let grace_period = Duration::from_millis(100);
        let mut node_slots = HashMap::new();
        node_slots.insert(
            "config-a-359973-3".to_string(),
            NodeUsage::create(&DeviceUsageKind::Instance, "node-a").unwrap(),
        );
        configure_scnenario(
            // slot_query to identify one slot used by this node
            NodeSlots {
                node_slots,
                node_slots_error: false,
            },
            // kube_interface to find Instance with node-a using slots:
            //    config-a-359973-1 & config-a-359973-3
            "../test/json/shared-instance-list-slots.json",
            None,
            grace_period,
            &reconciler,
        )
        .await;

        // Validate that the slot has been added to the list of "to be removed slots"
        assert!(reconciler.removal_slot_map.lock().unwrap().len() == 1);
        assert!(reconciler
            .removal_slot_map
            .lock()
            .unwrap()
            .contains_key("config-a-359973-1"));

        // Wait for more than the grace period ... it short, so, just wait twice :)
        std::thread::sleep(grace_period);
        std::thread::sleep(grace_period);

        let mut node_slots_added = HashMap::new();
        node_slots_added.insert(
            "config-a-359973-1".to_string(),
            NodeUsage::create(&DeviceUsageKind::Instance, "node-a").unwrap(),
        );
        node_slots_added.insert(
            "config-a-359973-3".to_string(),
            NodeUsage::create(&DeviceUsageKind::Instance, "node-a").unwrap(),
        );
        configure_scnenario(
            // slot_query to identify two slots used by this node
            NodeSlots {
                node_slots: node_slots_added,
                node_slots_error: false,
            },
            // kube_interface to find Instance with node-a using slots:
            //    config-a-359973-1 & config-a-359973-3
            "../test/json/shared-instance-list-slots.json",
            None,
            grace_period,
            &reconciler,
        )
        .await;

        // Validate that the slot has been added to the list of "to be removed slots"
        assert!(reconciler.removal_slot_map.lock().unwrap().is_empty());
    }
}
