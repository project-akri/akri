# Architecture
This document will describe the four components of Akri: the Configuration CRD, Instance CRD, Agent, and Controller. The word "resource" is used to describe what is being searched for and ultimately utilized. Resources offer services. For example, they can be USB or IP cameras, which serve video frames, or GPUs, which provide computation. They can be attached locally attached, embedded, or remotely accessible to worker nodes, such as USB devices, GPUs, and IP cameras, respectively.

### Custom Resource Definitions
There are two Akri CRDs:
1. Configuration
2. Instance

#### Akri Configuration CRD
The configuration of Akri is enabled by the Configuration CRD.  Akri users will create Configurations to describe what resources should be discovered and what pod should be deployed on the nodes that discover a resource. Take a look at the [Akri Configuration CRD](../deployment/helm/crds/akri-configuration-crd.yaml). It specifies what components all Configurations must have, including the following:

* the desired discovery protocol used for finding resources, i.e. ONVIF or udev.
* a capacity (spec.capacity) that defines the maximum number of nodes that may schedule workloads on this resource.
* a PodSpec (spec.brokerPodSpec) that defines the "broker" pod that will be scheduled to each of these reported resources.
* a ServiceSpec (spec.instanceServiceSpec) that defines the service that provides a single stable endpoint to access each individual resource's set of broker pods.
* a ServiceSpec (spec.configurationServiceSpec) that defines the service that provides a single stable endpoint to access the set of all brokers for all resources associated with the Configuration.

Akri has already provided two Configurations, one for discovering IP cameras using the ONVIF protocol and the other for discovering USB cameras via udev. Let's look at an [example ONVIF Configuration yaml](../test/yaml/akri-onvif-video.yaml). You can see it specifies the protocol ONVIF, an image for the broker pod, a capacity of 5, and two Kubernetes services. In this case, the broker pod is a sample frame server we have provided. To get only the frames from a specific camera, a user could point an application at the Instance service, while the Configuration service provides the frames from all the cameras.The ONVIF Configuration can be customized using Helm. When installing the ONVIF Configuration to your Akri enabled cluster, you can specify [the values](../deployment/helm/values.yaml) you want to be inserted into the [ONVIF Configuration template](../deployment/helm/templates/onvif-video.yaml). Learn more about [deploying the ONVIF sample here](./onvif-sample.md). 

#### Akri Instance CRD
Each Instance represents an individual resource that is visible to the cluster. So, if there are 5 IP cameras visible to the cluster, there will be 5 Instances. Akri coordination and resource sharing is enabled by the Instance CRD. These instances store internal Akri state and are not intended to be edited by users. For a more in-depth understanding on how resource sharing is accomplished, see [Resource Sharing In-depth](./resource-sharing-in-depth.md).

### Agent
The Akri Agent is an implementation of the [Kubernetes Device-Plugin](https://kubernetes.io/docs/concepts/extend-kubernetes/compute-storage-net/device-plugins/).

The basic flow of the Akri Agent is:
1. Watch for Configuration changes to determine what resources to search for
1. Monitor resource availability (as edge devices may come and go) to determine what resources to advertise
1. Inform Kubernetes of resource health/availability as it changes

This basic flow combined with the state stored in the Instance allows multiple nodes to share a resource while respecting the limitations defined by Configuration.capacity.

For a more in-depth understanding, see [Agent In-depth](./agent-in-depth.md).

### Controller
The Akri controller serves two purposes:
1. Handle (create and/or delete) the Pods & Services that enable resource availability
1. Ensure that Instances are aligned to the cluster state at any given moment

To achieve these goals, the basic flow of the controller is:
1. Watch for Instance changes to determine what Pods and Services should exist
1. Watch for Nodes that are contained in Instances that no longer exist

This basic flow allows the Akri controller to ensure that protocol brokers and Kubernetes Services are running on all nodes exposing desired resources while respecting the limitations defined by `Configuration.capacity`.

For a more in-depth understanding, see [Controller In-depth](./controller-in-depth.md).
