# Akri Agent
The Akri Agent executes on all worker Nodes in the cluster.  It is primarily tasked with:

1. Handling resource availability changes
1. Enabling resource sharing

These two tasks enable Akri to find configured resources (leaf devices), expose them to the Kubernetes cluster for workload scheduling, and allow resources to be shared by multiple Nodes.

## Handling resource availability changes
The first step in handling resource availability is determining what resources (leaf devices) to look for.  This is accomplished by finding existing Configurations and watching for changes to them.

Once the Akri Agent understands what resources to look for (via `Configuration.discovery_handler`), it will [find any resources that are visible](##resource-discovery).

For each resource that is found:

1. An Instance is created and uploaded to etcd
1. A connection with the kubelet is established according to the Kubernetes Device Plugin framework.  This connection is used to convey availability changes to the kubelet. The kubelet will, in turn, expose these availability changes to the Kubernetes scheduler.

Each protocol will periodically reassess what resources are visible and update both the Instance and the kubelet with the current availability.

This process allows Akri to dynamically represent resources that appear and disappear.

## Enabling resource sharing
To enable resource sharing, the Akri Agent creates and updates the `Instance.deviceUsage` map and communicates with kubelet.  The `Instance.deviceUsage` map is used to coordinate between Nodes.  The kubelet communication allows Akri Agent to communicate any resource availability changes to the Kubernetes scheduler.

For more detailed information, see the [in-depth resource sharing doc](./resource-sharing-in-depth.md).

## Resource discovery
The Agent discovers resources via Discovery Handlers (DHs). A Discovery Handler is anything that implements the
`DiscoveryHandler` service defined in [`discovery.proto`](../discovery-utils/proto/discovery.proto). In order to be
utilized, a DH must register with the Agent, which hosts the `Registration` service defined in
[`discovery.proto`](../discovery-utils/proto/discovery.proto). The Agent maintains a list of registered DHs and their
connectivity statuses, which is either `Waiting`, `Active`, or `Offline(Instant)`. When registered, a DH's status is
`Waiting`. Once a Configuration requesting resources discovered by a DH is applied to the Akri-enabled cluster, the
Agent will create a connection with the DH requested in the Configuration and set the status of the DH to `Active`. If
the Agent is unable to connect or loses a connection with a DH, its status is set to `Offline(Instant)`. The `Instant`
marks the time at which the DH became unresponsive. If the DH has been offline for more than 5 minutes, it is removed
from the Agent's list of registered Discovery Handlers. If a Configuration is deleted, the Agent drops the connection it
made with all DHs for that Configuration and marks the DHs' statuses as `Waiting`. Note, while probably not commonplace,
the Agent allows for multiple DHs to be registered for the same protocol. IE: you could have two udev DHs running on a
node on different sockets. 

The Agent's registration service defaults to running on the socket `/var/lib/akri/agent-registration.sock` but can be
Configured with Helm. While Discovery Handlers must register with this service over UDS, the Discovery Handler's service
can run over UDS or an IP based endpoint.

Supported Rust DHs each have a [library](../discovery-handlers) and a [binary
implementation](../discovery-handler-modules). This allows them to either be run within the Agent binary or in their own
Pod.

Reference the [Discovery Handler development document](./discovery-handler-development.md) to learn how to implement a
Discovery Handler. 
