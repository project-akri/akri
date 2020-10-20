# Akri Device Sharing
To enable multiple nodes to share a single resource, there are two vital pieces:

1. The `Configuration.capacity`
1. The `Instance.deviceUsage`

## Configuration.capacity
The configuration's capacity determines how many Nodes are allowed to schedule a workload for a given resource.  If the capacity is set to 5 and there are 10 worker nodes that can access the resource, only 5 will have Running workloads at any given moment (the remaining nodes will have workloads in a Pending state).  This provides 2 important values:

1. High availability - if a Running workload stops or fails, one of the Pending workloads will be scheduled and will start Running
1. Connection throttling - this supports resources that can only handle so many requests or connections at once

## Instance.deviceUsage
When the Akri Agent discovers a resource and creates an Instance, the deviceUsage map is initialized based on the `Configuration.capacity`.  If the capacity is 5, then the deviceUsage map will have 5 mappings, or slots.  The slots are named using a simple pattern, in this case, the initial deviceUsage might look like:

```yaml
  deviceUsage:
    my-resource-00095f-0: ""
    my-resource-00095f-1: ""
    my-resource-00095f-2: ""
    my-resource-00095f-3: ""
    my-resource-00095f-4: ""
```

Each slot is initialized to be mapped to an empty string, signifying that no Node is utilizing this slot.  When a Node utilizes a slot, it will change the mapping to include its name (i.e., `my-resource-00095f-2: "node-a"`)

During this initialization, a separate, but similar, mapping is sent to the kubelet ... for our example with 5 unutilized slots, this mapping would look like this:

```yaml
    my-resource-00095f-0: "Healthy"
    my-resource-00095f-1: "Healthy"
    my-resource-00095f-2: "Healthy"
    my-resource-00095f-3: "Healthy"
    my-resource-00095f-4: "Healthy"
```

When the kubelet attempts to schedule a workload on a specific Node, that Node's Akri Agent will be queried with a slot name (this slot name is chosen by the kubelet from the mapping list that Akri Agent sent it).  Akri Agent will query the appropriate Instance to see if that resource is still visible and if the mapping for that slot is still empty.  If both of these requirements are met, then the Akri Agent will update the `Instance.deviceUsage` map to claim the slot, and will allow the kubelet to schedule its intended workload.  After this, the `Instance.deviceUsage` may look something like this:

```yaml
  deviceUsage:
    my-resource-00095f-0: ""
    my-resource-00095f-1: ""
    my-resource-00095f-2: ""
    my-resource-00095f-3: "node-a"
    my-resource-00095f-4: ""
```

When this Instance is changed, in this case for `node-a` to claim slot `my-resource-00095f-3`, every Akri Agent that can access this instance will react by notifying the kubelet that this slot is no longer available:

```yaml
    my-resource-00095f-0: "Healthy"
    my-resource-00095f-1: "Healthy"
    my-resource-00095f-2: "Healthy"
    my-resource-00095f-3: "Unhealthy"
    my-resource-00095f-4: "Healthy"
```

These two steps will ensure that a specific slot is only used by one Node.

There is a possible race condition here.  What happens if Kubernetes tries to schedule a workload after the `Instance.deviceUsage` slot has been claimed, but before other Nodes have reported the slot as Unhealthy?

In this case, we can depend on the Instance as the truth.  If the kubelet sends a query with a slot name that is claimed by another node in `Instance.deviceUsage`, an error is returned to the kubelet and the workload will not be scheduled. Instead, the pod will stay in a `Pending` state until the Akri Controller brings it down. The Akri Agent will immediately notify the kubelet of the accurate `deviceUsage` slot availability and continue to periodically do this (as usual). Once the pod has been brought down by the Controller, if there are still some slots available, the Controller may reschedule the pod to that Node. Then, the kubelet can attempt to reserve a slot again, this time hopefully not hitting a collision. 

### Special case: workload disappearance
There is one case that is not addressed above: when a workload fails, finishes, or generally no longer exists.  In this case, the slot that the workload claimed needs to be released.

Unfortunately, the kubelet's Device-Plugin framework does not make finding this simple.  There is no deallocate or "pod failed" notification and there is no simple way to connect a slot with a workload.  However, the kubelet does let Akri Agent pass some annotations that will be attached to the workload's container.  

So, to support this slot recovery, Akri Agents add annotations identifying both the slot name and resource instance name.  These annotations allow each Akri Agent to periodically query the container runtime (through crictl, which is mounted on each akri-agent-daemonset Pod) to find all running containers.  These containers and their annotations are then used to ensure that all `Instance.deviceUsage` maps are accurate.  Any slots found without a backing container are cleared out (after a 5 minute timeout, that allows for a container to temporarily disappear).
