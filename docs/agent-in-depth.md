# Akri Agent
The Akri Agent executes on all worker Nodes in the cluster.  It is primarily tasked with:

1. Handling resource availability changes
1. Enabling resource sharing

These two tasks enable Akri to find configured resources (leaf devices), expose them to the Kubernetes cluster for workload scheduling, and allow resources to be shared by multiple Nodes.

## Handling resource availability changes
The first step in handling resource availability is determining what resources (leaf devices) to look for.  This is accomplished by finding existing Configurations and watching for changes to them.

Once the Akri Agent understands what resources to look for (via `Configuration.protocol`), it will find any resources that are visible.

For each resource that is found:

1. An Instance is created and uploaded to etcd
1. A connection with the kubelet is established according to the Kubernetes Device Plugin framework.  This connection is used to convey availability changes to the kubelet. The kubelet will, in turn, expose these availability changes to the Kubernetes scheduler.

Each protocol will periodically reassess what resources are visible and update both the Instance and the kubelet with the current availability.

This process allows Akri to dynamically represent resources that appear and disappear.

## Enabling resource sharing
To enable resource sharing, the Akri Agent creates and updates the `Instance.deviceUsage` map and communicates with kubelet.  The `Instance.deviceUsage` map is used to coordinate between Nodes.  The kubelet communication allows Akri Agent to communicate any resource availability changes to the Kubernetes scheduler.

For more detailed information, see the [in-depth resource sharing doc](./resource-sharing-in-depth.md).
