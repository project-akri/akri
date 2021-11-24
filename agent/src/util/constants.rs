/// For unshared devices, Healthy means the device is discoverable. For shared devices, Healthy means the device is
/// either unused or used by this node.
pub const HEALTHY: &str = "Healthy";

/// For unshared devices, Unhealthy means the device is not discoverable. For shared devices, Unhealthy means that the
/// device shared and used already by another node.
pub const UNHEALTHY: &str = "Unhealthy";

/// Current version of the API supported by kubelet.
pub const K8S_DEVICE_PLUGIN_VERSION: &str = "v1beta1";

/// DevicePluginPath is the folder the kubelet expects to find Device-Plugin sockets.
pub const DEVICE_PLUGIN_PATH: &str = "/var/lib/kubelet/device-plugins";

/// Path of the Kubelet registry socket
pub const KUBELET_SOCKET: &str = "/var/lib/kubelet/device-plugins/kubelet.sock";

/// Maximum length of time `list_and_watch` will sleep before sending kubelet another list of virtual devices
pub const LIST_AND_WATCH_SLEEP_SECS: u64 = 60;

/// Length of time a shared instance can be offline before it's `DevicePluginService` is shutdown.
pub const SHARED_INSTANCE_OFFLINE_GRACE_PERIOD_SECS: u64 = 300;

/// Length of time to sleep between slot reconciliation checks
pub const SLOT_RECONCILIATION_CHECK_DELAY_SECS: u64 = 10;

/// Length of time a slot can be unused before slot reconciliation reclaims it
pub const SLOT_RECONCILIATION_SLOT_GRACE_PERIOD_SECS: u64 = 300;

/// Label of environment variable that, when set, enables the embedded debug echo discovery handler
#[cfg(any(test, feature = "agent-full"))]
pub const ENABLE_DEBUG_ECHO_LABEL: &str = "ENABLE_DEBUG_ECHO";

/// Capacity of channel over which `DevicePluginService::list_and_watch` sends updates to kubelet about "virtual" device
/// health of an instance. The kubelet Device Plugin manager should receive each message instantly; however, providing
/// some buffer in case.
pub const KUBELET_UPDATE_CHANNEL_CAPACITY: usize = 4;

/// Capacity of channel over which the Agent Registration updates `DiscoveryOperators` when new `DiscoveryHandlers`
/// register. Tokio does not provide an unbounded broadcast channel in order to prevent the channel from growing
/// infinitely due to a "slow receiver". It is hard to determine an appropriate channel size, since the number of
/// `DiscoveryOperator` receivers (equivalent to number of applied Akri Configurations) and the frequency of sends
/// (equivalent to the number of registering `DiscoveryHandlers`) are unpredictable. Therefore, a large size is chosen
/// out of caution.
pub const NEW_DISCOVERY_HANDLER_CHANNEL_CAPACITY: usize = 15;

/// Capacity of channel over which the `DevicePluginService::list_and_watch` receives messages to
/// `ListAndWatchMessageKind::Continue` (prematurely send updates to kubelet) or `ListAndWatchMessageKind::End`
/// (terminate itself). `list_and_watch` receives messages asynchronously from `DevicePluginService.allocate`,
/// `DiscoveryOperator.update_connectivity_status`, and `handle_config_delete`. Messages are sent as a response to a
/// variety of events, such as an Instance going offline/online, a Configuration being deleted, or a slot being
/// requested via allocate that is already taken, making it hard to determine the appropriate size of the channel. If a
/// new message is put in the channel after capacity is already met, the oldest message is dropped, dropping a
/// `ListAndWatchMessageKind::End` would likely be unrecoverable. Tokio does not provide an unbounded broadcast channel
/// in order to prevent the channel from growing infinitely due to a "slow receiver", so a large channel size is chosen
/// out of caution.
pub const LIST_AND_WATCH_MESSAGE_CHANNEL_CAPACITY: usize = 15;

/// Capacity of channel over which a `DevicePluginService` receives a shutdown signal. This is either sent by
/// `DevicePluginBuilder::register` or `DevicePluginService::list_and_watch`. Capacity is set to meet worst case
/// scenario in which they both send messages at the same time.
pub const DEVICE_PLUGIN_SERVER_ENDER_CHANNEL_CAPACITY: usize = 2;

/// Capacity of channel over which a `DiscoveryOperator` is notified to stop discovery for its Configuration. This
/// signals it to tell each of its subtasks to stop discovery. Message is only sent once, upon Configuration deletion.
pub const DISCOVERY_OPERATOR_STOP_DISCOVERY_CHANNEL_CAPACITY: usize = 1;

/// Capacity of channel over which a DiscoveryOperator signals that it has stopped discovery and a Configuration's
/// Instances and associated `DevicePluginServices` can safely be deleted/terminated. There is only one sender
/// (`DiscoveryOperator`) who only sends a message once.
pub const DISCOVERY_OPERATOR_FINISHED_DISCOVERY_CHANNEL_CAPACITY: usize = 1;

/// Capacity of channel over which `DiscoveryOperator` is notified to stop (trying to make) a connection with a
/// `DiscoveryHandler`. Sent once by the Agent Registration service when a `DiscoveryHandler` re-registers with a different
/// registration request (edge case).
pub const CLOSE_DISCOVERY_HANDLER_CONNECTION_CHANNEL_CAPACITY: usize = 1;
