use super::instance_action::InstanceAction;
use chrono::Utc;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::Time;

/// Pod action types
///
/// Pod actions describe the types of actions the controller can
/// take for broker Pods.
///
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PodAction {
    /// The broker Pod must be added
    Add,
    /// The broker Pod must be removed
    Remove,
    /// The broker Pod must be removed and added
    RemoveAndAdd,
    /// No action should be taken for the broker Pod
    NoAction,
}

/// This is used to determine what action to take for
/// a broker Pod.
///
/// The action to take is based on several factors:
/// 1. what the InstanceAction is (Add, Delete, Modify)
/// 1. what phase the Pod is in (Running, Pending, etc)
/// 1. when the Pod started
/// 1. the relevant grace time
///
pub struct PodActionInfo {
    pub pending_grace_time_in_minutes: i64,
    pub ended_grace_time_in_minutes: i64,
    pub phase: String,
    pub instance_action: InstanceAction,
    pub status_start_time: Option<Time>,
    pub unknown_node: bool,
    pub trace_node_name: String,
}

impl PodActionInfo {
    /// This will determine what action to take on the broker Pod
    ///
    ///   | --> (Unknown) ===> PodAction::Remove
    ///   | --> (Known)
    ///            | --> <Phase == Running>
    ///                     | --> <InstanceAction == Remove> ===> PodAction::Remove
    ///                     | --> <InstanceAction != Remove> ===> PodAction::NoAction
    ///            | --> <Phase == Other>
    ///                     | --> <Phase == Starting>
    ///                               | --> <InstanceAction == Remove> ===> PodAction::Remove
    ///                               | --> <InstanceAction != Remove> ===> PodAction::NoAction
    ///                     | --> <Phase == NonRunning>
    ///                               | --> (No PodStartTime) ===> PodAction::NoAction
    ///                               | --> (PodStartTime within grace period) ===> PodAction::NoAction
    ///                               | --> (PodStartTime outside grace period) ===> PodAction::RemoveAndAdd
    ///
    pub fn select_pod_action(&self) -> anyhow::Result<PodAction> {
        log::trace!(
            "select_pod_action phase={:?} action={:?} unknown_node={:?}",
            &self.phase,
            self.instance_action,
            self.unknown_node
        );
        self.choice_for_pod_action()
    }

    /// This will determine what to do with a non-Running Pod based on how long the Pod has existed
    fn time_choice_for_non_running_pods(
        &self,
        grace_period_in_minutes: i64,
    ) -> anyhow::Result<PodAction> {
        log::trace!("time_choice_for_non_running_pods");
        //
        // For Non-Running pods (with our controller's selector), apply NoAction if the pod has existed for
        // less than grace_time_in_minutes minutes ... otherwise, apply RemoveAndAdd to reset the pod.
        //
        let give_it_more_time: bool;
        if let Some(start_time) = &self.status_start_time {
            // If this pod has a start_time in its status, calculate when the grace period would end
            log::trace!(
                "time_choice_for_non_running_pods - checking for time after start_time ({:?})",
                start_time
            );
            let time_limit = &start_time
                .0
                .checked_add_signed(chrono::Duration::minutes(grace_period_in_minutes))
                .ok_or_else(|| anyhow::anyhow!("check_add_signed failed"))?;
            let now = Utc::now();
            log::trace!(
                "time_choice_for_non_running_pods - need more time? now:({:?}) ({:?})",
                now,
                *time_limit
            );
            // If "now" is less than the grace period, the pod deserves more time
            give_it_more_time = now < *time_limit;
            log::trace!(
                "time_choice_for_non_running_pods - give_it_more_time: ({:?})",
                give_it_more_time
            );
        } else {
            // If the pod has no start_time, give it more time
            log::trace!(
                "time_choice_for_non_running_pods - no start time found ... give it more time? ({:?})",
                &self.trace_node_name
            );
            give_it_more_time = true;
            log::trace!(
                "time_choice_for_non_running_pods - give_it_more_time: ({:?})",
                give_it_more_time
            );
        }

        if give_it_more_time {
            log::trace!(
                "time_choice_for_non_running_pods - Pending Pod (tracked) ... PodAction::NoAction ({:?})",
                &self.trace_node_name
            );
            Ok(PodAction::NoAction)
        } else {
            log::trace!(
                "time_choice_for_non_running_pods - Pending Pod (tracked) ... PodAction::RemoveAndAdd ({:?})",
                &self.trace_node_name
            );
            Ok(PodAction::RemoveAndAdd)
        }
    }

    /// This will determine what to do with a Running Pod
    fn choice_for_running_pods(&self) -> anyhow::Result<PodAction> {
        log::trace!(
            "choice_for_running_pods action={:?} trace_node_name={:?}",
            self.instance_action,
            &self.trace_node_name
        );
        match self.instance_action {
            InstanceAction::Remove => {
                //
                // For all pods (with our controller's selector) that are RUNNING on a node that is described in the
                // Instance (unknown_node=false), when that Instance is removed, we will REMVOE the pods.
                //
                log::trace!(
                    "choice_for_running_pods - Running Pod (tracked) ... PodAction::Remove ({:?})",
                    &self.trace_node_name
                );
                Ok(PodAction::Remove)
            }
            _ => {
                //
                // For all pods (with our controller's selector) that are RUNNING on a node that is described in the
                // Instance (unknown_node=false), when that Instance is !removed (added|updated),
                // we will perform NO-ACTION on the pods.
                //
                log::trace!(
                    "choice_for_running_pods - Running Pod (tracked) ... PodAction::NoAction ({:?})",
                    &self.trace_node_name
                );
                Ok(PodAction::NoAction)
            }
        }
    }

    /// This will determine what to do with a non-Running Pod
    fn choice_for_non_running_pods(
        &self,
        grace_period_in_minutes: i64,
    ) -> anyhow::Result<PodAction> {
        log::trace!(
            "choice_for_non_running_pods action={:?} trace_node_name={:?}",
            self.instance_action,
            &self.trace_node_name
        );
        match self.instance_action {
            InstanceAction::Remove => {
                //
                // For Non-Running pods (with our controller's selector), if the Instance is removed, we will
                // REMOVE the pod.
                //
                log::trace!(
                    "choice_for_non_running_pods - Pending Pod (tracked) ... PodAction::Remove ({:?})",
                    self.trace_node_name
                );
                Ok(PodAction::Remove)
            }
            _ => {
                //
                // For Non-Running pods (with our controller's selector), if the Instance is !removed (added|updated),
                // we will look at the start_time to determine what to do.
                //
                self.time_choice_for_non_running_pods(grace_period_in_minutes)
            }
        }
    }

    /// This will determine what to do with a Pod running on a known Node
    fn choice_for_pods_on_known_nodes(&self) -> anyhow::Result<PodAction> {
        log::trace!(
            "choice_for_pods_on_known_nodes phase={:?} action={:?} trace_node_name={:?}",
            &self.phase,
            self.instance_action,
            &self.trace_node_name
        );
        match self.phase.as_str() {
            "Running" | "ContainerCreating" | "PodInitializing" => {
                // Handle
                //    Kubelete states
                //         <Waiting> ContainerCreating
                //         <Waiting> PodInitializing
                //         <Running>

                //
                // For all pods (with our controller's selector) that are RUNNING ...
                //
                self.choice_for_running_pods()
            }
            _ => {
                // Handle
                //    Kubelete states
                //         <Terminated>

                //    Kubelet pod admit
                //         UnexpectedAdmissionError
                //         InvalidNodeInfo
                //         UnknownReason
                //         OutOf*
                //         UnexpectedPredicateFailureType
                //         <PredicateName>

                //    PodPhase
                //         Pending
                //         Succeeded
                //         Failed
                //         Unknown

                if self.phase.as_str() == "Pending" {
                    //
                    // For pods that are Pending (with our controller's selector) ...
                    //
                    self.choice_for_non_running_pods(self.pending_grace_time_in_minutes)
                } else {
                    //
                    // For pods that are not running (with our controller's selector) ...
                    //
                    self.choice_for_non_running_pods(self.ended_grace_time_in_minutes)
                }
            }
        }
    }

    /// This will determine what to do with a Pod
    fn choice_for_pod_action(&self) -> anyhow::Result<PodAction> {
        log::trace!(
            "choice_for_pod_action phase={:?} action={:?} unknown_node={:?} trace_node_name={:?}",
            &self.phase,
            self.instance_action,
            self.unknown_node,
            &self.trace_node_name
        );
        if self.unknown_node {
            //
            // For pods (with our controller's selector) that are not described by the Instance, we
            // will REMOVE the pods.
            //
            log::trace!(
                "choice_for_pod_action - Running Pod (untracked) ... PodAction::Remove ({:?})",
                &self.trace_node_name
            );
            Ok(PodAction::Remove)
        } else {
            //
            // For pods (with our controller's selector) that are described by the Instance ...
            //
            self.choice_for_pods_on_known_nodes()
        }
    }
}

#[cfg(test)]
mod controller_tests {
    use super::*;

    #[test]
    fn test_select_pod_action_for_unknown_nodes() {
        let _ = env_logger::builder().is_test(true).try_init();

        let unexpired_k8s_time = Time(Utc::now());
        let unexpired_start_time = Some(unexpired_k8s_time);
        let expired_k8s_time = Time(
            Utc::now()
                .checked_sub_signed(chrono::Duration::minutes(10))
                .unwrap(),
        );
        let expired_start_time = Some(expired_k8s_time);

        [None, unexpired_start_time, expired_start_time]
            .iter()
            .for_each(|start_time| {
                [
                    ("Running", PodAction::Remove),
                    ("Pending", PodAction::Remove),
                    ("UnexpectedAdmissionError", PodAction::Remove),
                    ("ContainerCreating", PodAction::Remove),
                    ("PodInitializing", PodAction::Remove),
                    ("blah-blah-unknown", PodAction::Remove),
                ]
                .iter()
                .for_each(|map_tuple| {
                    [
                        InstanceAction::Add,
                        InstanceAction::Remove,
                        InstanceAction::Update,
                    ]
                    .iter()
                    .for_each(|instance_action| {
                        println!(
                            "Testing phase={}, current action={:?} expected action={:?}",
                            map_tuple.0, instance_action, map_tuple.1
                        );
                        let pod_action_info = PodActionInfo {
                            pending_grace_time_in_minutes: 1,
                            ended_grace_time_in_minutes: 1,
                            phase: map_tuple.0.to_string(),
                            instance_action: instance_action.clone(),
                            status_start_time: start_time.clone(),
                            unknown_node: true,
                            trace_node_name: "foo".to_string(),
                        };
                        assert_eq!(map_tuple.1, pod_action_info.select_pod_action().unwrap());
                    });
                });
            });
    }

    #[test]
    fn test_select_pod_action_for_known_nodes_with_instance_add_with_no_start_time() {
        let _ = env_logger::builder().is_test(true).try_init();

        // for any known pod with NO start time (see None below), we should do nothing
        let start_time = None;
        [
            ("Running", PodAction::NoAction),
            ("Pending", PodAction::NoAction),
            ("UnexpectedAdmissionError", PodAction::NoAction),
            ("ContainerCreating", PodAction::NoAction),
            ("PodInitializing", PodAction::NoAction),
            ("blah-blah-unknown", PodAction::NoAction),
        ]
        .iter()
        .for_each(|map_tuple| {
            [InstanceAction::Add, InstanceAction::Update]
                .iter()
                .for_each(|instance_action| {
                    println!(
                        "Testing phase={}, current action={:?} expected action={:?}",
                        map_tuple.0, instance_action, map_tuple.1
                    );
                    let pod_action_info = PodActionInfo {
                        pending_grace_time_in_minutes: 1,
                        ended_grace_time_in_minutes: 1,
                        phase: map_tuple.0.to_string(),
                        instance_action: instance_action.clone(),
                        status_start_time: start_time.clone(),
                        unknown_node: false,
                        trace_node_name: "foo".to_string(),
                    };
                    assert_eq!(map_tuple.1, pod_action_info.select_pod_action().unwrap());
                });
        });
    }

    #[test]
    fn test_select_pod_action_for_known_nodes_with_instance_add_with_expired_start_time() {
        let _ = env_logger::builder().is_test(true).try_init();

        // for any known pod with EXPIRED start time, we should remove it
        println!("now={:?}", Utc::now());
        let expired_time = Utc::now()
            .checked_sub_signed(chrono::Duration::minutes(10))
            .unwrap();
        let k8s_time = Time(expired_time);
        let start_time = Some(k8s_time);
        println!("start_time={:?}", &start_time);
        [
            ("Running", PodAction::NoAction),
            ("Pending", PodAction::RemoveAndAdd),
            ("UnexpectedAdmissionError", PodAction::RemoveAndAdd),
            ("ContainerCreating", PodAction::NoAction),
            ("PodInitializing", PodAction::NoAction),
            ("blah-blah-unknown", PodAction::RemoveAndAdd),
        ]
        .iter()
        .for_each(|map_tuple| {
            [InstanceAction::Add, InstanceAction::Update]
                .iter()
                .for_each(|instance_action| {
                    println!(
                        "Testing phase={}, current action={:?} expected action={:?}",
                        map_tuple.0, instance_action, map_tuple.1
                    );
                    let pod_action_info1 = PodActionInfo {
                        pending_grace_time_in_minutes: 1,
                        ended_grace_time_in_minutes: 1,
                        phase: map_tuple.0.to_string(),
                        instance_action: instance_action.clone(),
                        status_start_time: start_time.clone(),
                        unknown_node: false,
                        trace_node_name: "foo".to_string(),
                    };
                    assert_eq!(map_tuple.1, pod_action_info1.select_pod_action().unwrap());
                });
        });
    }

    #[test]
    fn test_select_pod_action_for_known_nodes_with_instance_add_with_unexpired_start_time() {
        let _ = env_logger::builder().is_test(true).try_init();

        // for any known pod with UNEXPIRED start time, we should remove it
        let k8s_time = Time(Utc::now());
        let start_time = Some(k8s_time);
        [
            ("Running", PodAction::NoAction),
            ("Pending", PodAction::NoAction),
            ("UnexpectedAdmissionError", PodAction::NoAction),
            ("ContainerCreating", PodAction::NoAction),
            ("PodInitializing", PodAction::NoAction),
            ("blah-blah-unknown", PodAction::NoAction),
        ]
        .iter()
        .for_each(|map_tuple| {
            [InstanceAction::Add, InstanceAction::Update]
                .iter()
                .for_each(|instance_action| {
                    println!(
                        "Testing phase={}, current action={:?} expected action={:?}",
                        map_tuple.0, instance_action, map_tuple.1
                    );
                    let pod_action_info = PodActionInfo {
                        pending_grace_time_in_minutes: 1,
                        ended_grace_time_in_minutes: 1,
                        phase: map_tuple.0.to_string(),
                        instance_action: instance_action.clone(),
                        status_start_time: start_time.clone(),
                        unknown_node: false,
                        trace_node_name: "foo".to_string(),
                    };
                    assert_eq!(map_tuple.1, pod_action_info.select_pod_action().unwrap());
                });
        });
    }

    #[test]
    fn test_select_pod_action_for_known_nodes_with_instance_delete_with_unexpired_start_time() {
        let _ = env_logger::builder().is_test(true).try_init();

        // for any known pod with UNEXPIRED start time, we should remove it
        let k8s_time = Time(Utc::now());
        let start_time = Some(k8s_time);
        [
            ("Running", PodAction::Remove),
            ("Pending", PodAction::Remove),
            ("UnexpectedAdmissionError", PodAction::Remove),
            ("ContainerCreating", PodAction::Remove),
            ("PodInitializing", PodAction::Remove),
            ("blah-blah-unknown", PodAction::Remove),
        ]
        .iter()
        .for_each(|map_tuple| {
            [InstanceAction::Remove].iter().for_each(|instance_action| {
                println!(
                    "Testing phase={}, current action={:?} expected action={:?}",
                    map_tuple.0, instance_action, map_tuple.1
                );
                let pod_action_info = PodActionInfo {
                    pending_grace_time_in_minutes: 1,
                    ended_grace_time_in_minutes: 1,
                    phase: map_tuple.0.to_string(),
                    instance_action: instance_action.clone(),
                    status_start_time: start_time.clone(),
                    unknown_node: false,
                    trace_node_name: "foo".to_string(),
                };
                assert_eq!(map_tuple.1, pod_action_info.select_pod_action().unwrap());
            });
        });
    }

    #[test]
    fn test_select_pod_action_with_expired_start_time_for_ended_only() {
        let _ = env_logger::builder().is_test(true).try_init();

        // for any known pod with EXPIRED start time, we should remove it
        println!("now={:?}", Utc::now());
        let expired_time = Utc::now()
            .checked_sub_signed(chrono::Duration::minutes(3))
            .unwrap();
        let k8s_time = Time(expired_time);
        let start_time = Some(k8s_time);
        println!("start_time={:?}", &start_time);
        [
            ("Running", PodAction::NoAction),
            ("Pending", PodAction::NoAction),
            ("UnexpectedAdmissionError", PodAction::RemoveAndAdd),
            ("ContainerCreating", PodAction::NoAction),
            ("PodInitializing", PodAction::NoAction),
            ("blah-blah-unknown", PodAction::RemoveAndAdd),
        ]
        .iter()
        .for_each(|map_tuple| {
            [InstanceAction::Add, InstanceAction::Update]
                .iter()
                .for_each(|instance_action| {
                    println!(
                        "Testing phase={}, current action={:?} expected action={:?}",
                        map_tuple.0, instance_action, map_tuple.1
                    );
                    // scenario, we are asking to Add a known pod that is in map_tuple.0 state ... result should be map_tuple.1
                    let pod_action_info1 = PodActionInfo {
                        pending_grace_time_in_minutes: 5,
                        ended_grace_time_in_minutes: 1,
                        phase: map_tuple.0.to_string(),
                        instance_action: instance_action.clone(),
                        status_start_time: start_time.clone(),
                        unknown_node: false,
                        trace_node_name: "foo".to_string(),
                    };
                    assert_eq!(map_tuple.1, pod_action_info1.select_pod_action().unwrap());
                });
        });
    }
}
