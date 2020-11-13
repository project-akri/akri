/// For unshared devices, Healthy means the device is discoverable.
/// For shared devices, Healthy means the device is either unused or used by this node.
pub const HEALTHY: &str = "Healthy";

/// For unshared devices, Unhealthy means the device is not discoverable.
/// For shared devices, UnHealthy means that the device shared and used already by another node.
pub const UNHEALTHY: &str = "Unhealthy";

/// Current version of the API supported by kubelet.
pub const K8S_DEVICE_PLUGIN_VERSION: &str = "v1beta1";

/// DevicePluginPath is the folder the kubelet expects to find Device-Plugin sockets. Only privileged pods have access to this path.
#[cfg(not(test))]
pub const DEVICE_PLUGIN_PATH: &str = "/var/lib/kubelet/device-plugins";
/// Path for testing `DevicePluginService`
#[cfg(test)]
pub const DEVICE_PLUGIN_PATH: &str = "dummy";

/// Path of the Kubelet registry socket
pub const KUBELET_SOCKET: &str = "/var/lib/kubelet/device-plugins/kubelet.sock";

/// Maximum length of time `list_and_watch` will sleep before sending kubelet another list of virtual devices
pub const LIST_AND_WATCH_SLEEP_SECS: u64 = 60;

/// Length of time to sleep between instance discovery checks
pub const DISCOVERY_DELAY_SECS: u64 = 10;

/// Length of time a shared instance can be offline before it's `DevicePluginService` is shutdown.
pub const SHARED_INSTANCE_OFFLINE_GRACE_PERIOD_SECS: u64 = 300;

/// Length of time to sleep between slot reconciliation checks
pub const SLOT_RECONCILIATION_CHECK_DELAY_SECS: u64 = 10;

/// Length of time a slot can be unused before slot reconciliation relaims it
pub const SLOT_RECONCILIATION_SLOT_GRACE_PERIOD_SECS: u64 = 300;
