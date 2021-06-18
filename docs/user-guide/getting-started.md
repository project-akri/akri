# Getting Started

To best understand the benefits of Akri and jump into using it, we recommend you start off by completing the [end to end demo](../demos/camera-mocked.md). In the demo, you will see Akri discover mock video cameras and a streaming app display the footage from those cameras. It includes instructions on K8s cluster setup.

## The Journey Begins

To get started using Akri, you must first decide what you want to discover and whether Akri currently supports a Discovery Handler that can be used to discover resources of that type. Akri discovers devices via Discovery Handlers, which are often protocol implementations that understand filter information passed via an Akri Configuration. To see the list of currently supported Discovery Handlers, see our [roadmap](https://github.com/deislabs/akri/projects/3).

### Understanding Akri Helm charts

Akri is most easily deployed with Helm charts. Helm charts provide convenient packaging and configuration.

Starting in v0.0.36, an **akri-dev** Helm chart will be published for each build version. Each Akri build is verified with end-to-end tests on Kubernetes, K3s, and MicroK8s. These builds may be less stable than our Releases. You can deploy these versions of Akri with this command \(note: **akri-dev**\):

```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri-dev
```

Starting in Release v0.0.44, an **akri** Helm chart will be published for each [Release](https://github.com/deislabs/akri/releases). Releases will generally reflect milestones and will have more rigorous testing. You can deploy Release versions of Akri with this command \(note: **akri**\):

```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri
```

To use the latest containers of the Akri components, add `--set useLatestContainers=true` when installing Akri like so:

```bash
helm install akri akri-helm-charts/akri \
   --set useLatestContainers=true
```

Before v0.4.0, all of Akri's Discovery Handlers were embedded in the Agent. As more Discovery Handlers are added to Akri, this will become unsustainable and cause the Agent to have a larger footprint than oftentimes necessary \(if only one of the many Discovery Handlers is being leveraged\). Starting in v0.4.0, Akri is starting the transition to mainly supporting an Agent image without any embedded Discovery Handlers, which will be the image used by Akri's Helm chart by default. The required Discovery Handlers can be deployed as DaemonSets by setting `<discovery handler name>.discovery.enabled=true` when installing Akri, as explained in the [user flow](getting-started.md#installing-akri-flow). To instead use the previous strategy of an Agent image with embedded udev, OPC UA, and ONVIF Discovery Handlers, set `agent.full=true`.

To see which version of the **akri** and **akri-dev** Helm charts are stored locally, run `helm inspect chart akri-helm-charts/akri` and `helm inspect chart akri-helm-charts/akri-dev`, respectively.

To grab the latest Akri Helm charts, run `helm repo update`.

### Setting up your cluster

Before deploying Akri, you must have a Kubernetes cluster \(v1.16 or higher\) running with `kubectl` and `Helm` installed. Reference our [cluster setup docu](cluster-setup.md)[entation](cluster-setup.md) to set up a cluster or adapt your currently existing cluster. Akri currently supports Linux Nodes on amd64, arm64v8, or arm32v7.

### Installing Akri Flow

Akri is installed using its Helm Chart, which contains settings for deploying the Akri Agents, Controller, Discovery Handlers, and Configurations. All these can be installed in one command, in several different Helm installations, or via consecutive `helm upgrades`. This section will focus on the latter strategy, helping you construct your Akri installation command, assuming you have already decided what you want Akri to discover.

Akri's Helm chart deploys the Akri Controller and Agent by default, so you only need to specify which Discovery Handlers and Configurations need to be deployed in your command. Akri discovers devices via Discovery Handlers, which are often protocol implementations. Akri currently supports three Discovery Handlers \(udev, OPC UA and ONVIF\); however, custom discovery handlers can be created and deployed as explained in Akri's [Discovery Handler development document](../development/handler-development.md). Akri is told what to discover via Akri Configurations, which specify the name of the Discovery Handler that should be used, any discovery details \(such as filters\) that need to be passed to the Discovery Handler, and optionally any broker Pods and services that should be created upon discovery. For example, the ONVIF Discovery Handler can receive requests to include or exclude cameras with certain IP addresses.

Let's walk through building an Akri installation command:

1. Get Akri's Helm repo

   ```bash
    helm repo add akri-helm-charts https://deislabs.github.io/akri/
   ```

2. Install Akri's Controller and Agent, specifying the crictl configuration from [the cluster setup steps](cluster-setup.md) in not using vanilla Kubernetes:

   ```bash
     helm install akri akri-helm-charts/akri \
        $AKRI_HELM_CRICTL_CONFIGURATION
   ```

   > Note: To use Akri's latest dev releases, specify `akri-helm-charts/akri`

3. Upgrade the installation to deploy the Discovery Handler you wish to use. Discovery Handlers are deployed as DaemonSets like the Agent when `<discovery handler name>.discovery.enabled` is set.

   ```bash
    helm upgrade akri akri-helm-charts/akri \
        --set <discovery handler name>.discovery.enabled=true
   ```

   > Note: To install a full Agent with embedded udev, OPC UA, and ONVIF Discovery Handlers, set `agent.full=true` instead of enabling the Discovery Handlers. Note, this we restart the Agent Pods.
   >
   > ```bash
   > helm upgrade akri akri-helm-charts/akri \
   >    --set agent.full=true
   > ```

4. Upgrade the installation to apply a Configuration, which requests discovery of certain devices by a Discovery Handler. A Configuration is applied by setting `<discovery handler name>.configuration.enabled`. While some Configurations may not require any discovery details to be set, oftentimes setting details is preferable for narrowing the Discovery Handlers' search. These are set under `<discovery handler name>.configuration.discoveryDetails`. For example, udev rules are passed to the udev Discovery Handler to specify which devices in the Linux device file system it should search for by setting `udev.configuration.discoveryDetails.udevRules`. Akri can be instructed to automatically deploy workloads called "brokers" to each discovered device by setting a broker Pod image in a Configuration via `--set <protocol>.configuration.brokerPod.image.repository=<your broker image>`. Learn more about creating brokers in the [broker development document](../development/broker-development.md).

   ```bash
    helm upgrade akri akri-helm-charts/akri \
        --set <discovery handler name>.discovery.enabled=true \
        --set <discovery handler name>.configuration.enabled=true \
        # set any discovery details in the Configuration
        # specify any broker images in the Configuration
   ```

Installation could have been done in one step rather than a series of upgrades:

```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri \
    --set <discovery handler name>.discovery.enabled=true \
    --set <discovery handler name>.configuration.enabled=true \
    # set any discovery details in the Configuration
    # specify any broker images in the Configuration
```

As a real example, Akri's Controller, Agents, udev Discovery Handlers, and a udev Configuration that specifies the discovery of only USB video devices and an nginx broker image are installed like so:

```bash
helm install akri akri-helm-charts/akri \
    --set udev.discovery.enabled=true \
    --set udev.configuration.enabled=true \
    --set udev.configuration.discoveryDetails.udevRules[0]='KERNEL=="video[0-9]*"' \
    --set udev.configuration.brokerPod.image.repository=nginx
```

> Note: set `<discovery handler name>.brokerPod.image.tag` to specify an image tag \(defaults to `latest`\).

This installation can be expanded to install multiple Discovery Handlers and/or Configurations. See the documentation on [udev](../discovery-handlers/udev.md), [OPC UA](../discovery-handlers/opc-ua.md), and [ONVIF](../discovery-handlers/onvif.md) Configurations to learn more about setting the discovery details passed to their Discovery Handlers and more.

See [modifying an Akri Installation](customizing-an-akri-installation.md) to learn about how to use Akri's Helm chart to install additional Configurations and Discovery Handlers.

### Inspecting an Akri Installation

* Run `kubectl get crd`, and you should see Akri's two CRDs listed. 
* Run `kubectl get pods -o wide`, and you should see the Akri Controller, Agent, and \(if specified\) broker pods. 
* Run `kubectl get akric`, and you should see the Configuration for the protocol you specified.  
* If devices were discovered, the instances can be seen by running `kubectl get akrii` and further inspected by running `kubectl get akrii <discovery handler name>-<ID> -o yaml`.
* List all that Akri has automatically created and deployed, namely the Akri Controller, Agents, Configurations, Instances \(which are the Akri custom resource that represents each device\), and if specified, broker Pods, a service for each broker Pod, and a service for all brokers.

  ```bash
    watch microk8s kubectl get pods,akric,akrii,services -o wide
  ```

    For K3s and vanilla Kubernetes

  ```bash
    watch kubectl get pods,akric,akrii,services -o wide
  ```

  **Deleting Akri Configurations**

  To tell Akri to stop discovering devices, simply delete the Configuration that initiated the discovery. Watch as all instances that represent the discovered devices are deleted.

  ```bash
  kubectl delete akric akri-<discovery handler name>
  kubectl get akrii
  ```

### Deleting Akri

1. If you are done using Akri, it can be uninstalled via Helm.

   ```bash
    helm delete akri
   ```

2. Delete Akri's CRDs.

   ```bash
    kubectl delete crd instances.akri.sh
    kubectl delete crd configurations.akri.sh
   ```

### Customizing where the Controller runs

By default the Controller can be deployed to any control plane or worker node. This can be changed by adding extra settings when installing Akri below. If you don't want the Controller to ever be scheduled to control plane nodes, add `--set controller.allowOnControlPlane=false` to your install command below. Conversely, if you only want the Controller to run on control plane nodes, add `--set controller.onlyOnControlPlane=true`. This will guarantee the Controller only runs on nodes with the label \(key, value\) of \(`node-role.kubernetes.io/master`, ""\), which is the default label for the control plane node for Kubernetes.

However, control plane nodes on MicroK8s and K3s may not have this exact label by default, so you can add it by running `kubectl label node ${HOSTNAME,,} node-role.kubernetes.io/master=--overwrite=true`. Or alternatively, in K3s, you can keep the default label value on the master and set `controller.nodeSelectors."node-role\.kubernetes\.io/master"=true`.

