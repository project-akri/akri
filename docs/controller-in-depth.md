# Akri Controller
The Akri Controller executes on the master Node in the cluster.  It is primarily tasked with:

1. Enabling cluster access to leaf devices
1. Handling node disappearances

These tasks enable Akri to provide resources with high availability, while allowing the Kubernetes application to be agnostic about what specific Nodes or Pods are executing at any given moment.

## Enabling cluster access to resources
The first step to enable cluster access to resources (leaf devices) is, of course, finding them.  The work of discovering resources and making them known to the Kubernetes cluster is handled by the [Akri Agent](./agent-in-depth.md).  The Akri Agents ensure that Instances are created and updated to enforce capability sharing.

Once a capability has been discovered and Instances are created, it is up to the Akri Controller to provide cluster access.

To provide access to discovered resources, the Akri Controller works to ensure that the Pods and Services described in the relevant Configuration are running.  This is accomplished by listening for changes, additions, and deletions of Instances.

When an instance is created or updated, the Akri Controller needs to do several things:

1. Ensure that the protocol broker Pod based on `Configuration.brokerPodSpec` is created
1. Ensure that the broker Service based on `Configuration.instanceServiceSpec` is created
1. Ensure that the capability Service based on `Configuration.configurationServiceSpec` is created

When an instance is deleted, the Akri Controller needs to do several things:

1. Ensure that the protocol broker Pod based on `Configuration.brokerPodSpec` is removed
1. Ensure that the protocol broker Service based on `Configuration.instanceServiceSpec` is removed
1. Ensure that the capability Service based on `Configuration.configurationServiceSpec` is removed, if there are no Pods supporting the Service (note that many instances can contribute supporting Pods to a given configuration)

## Handling node disappearances
One of the conditions we need to be aware of is node disappearance.  In this case, we cannot depend on the disappeared node's Akri Agent to modify the relevant Instance.  To free up any `Configuration.capacity` that a node was using prior to disappearing, the Akri Controller watches for Node disappearance events and cleans up any lingering node references in any `Instance.nodes` and `Instance.deviceUsage`.