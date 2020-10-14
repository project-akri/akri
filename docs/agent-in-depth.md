# Akri Agent
The Akri Agent executes on all worker Nodes in the cluster.  It is primarily tasked with:

1. Handling capability availabiity changes
1. Enabling capability sharing

These two tasks enable Akri to find configured capabilities, expose them to the Kubernetes cluster for workload scheduling, and allow capabilities to be shared by multiple Nodes.

## Handling capability availabiity changes
The first step in handling capability availability is determining what capabilities to look for.  This is accomplished by finding existing Configurations and watching for changes to them.

Once the Akri Agent understands what capabilities to look for (via `Configuration.protocol`), it will find any capabilities that are visible.

For each capability that is found:

1. An Instance is created and uploaded to etcd
1. A connection with Kubelet is established according to the Kubernetes Device Plugin framework.  This connection is used to convey availability changes to Kubelet.  Kubelet will, in turn, expose these availability changes to the Kubernetes scheduler.

Each protocol will periodically reassess what capabilities are visible and update both the Instance and Kubelet with the current availability.

This process allows Akri to dynamically represent capabilities that appear and disappear.

## Enabling capability sharing
To enable capability sharing, the Akri Agent creates and updates the `Instance.deviceUsage` map and communicates with Kubelet.  The `DeviceCapabiltiyInstance.deviceUsage` map is used to coordinate between Nodes.  The Kubelet communication allows Akri Agent to communicate any capability availabilty changes to the Kubernetes scheduler.

For more detailed information, see the [in-depth capability sharing doc](./capability-sharing-in-depth.md).
