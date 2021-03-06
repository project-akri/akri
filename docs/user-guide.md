# User Guide
To best understand the benefits of Akri and jump into using it, we recommend you start off by completing the [end to end
demo](./end-to-end-demo.md). In the demo, you will see Akri discover mock video cameras and a streaming app display the
footage from those cameras. It includes instructions on K8s cluster setup. If you would like to perform the demo on a
cluster of Raspberry Pi 4's, see the [Raspberry Pi 4 demo](./end-to-end-demo-rpi4.md).

## Getting Started
To get started using Akri, you must first decide what you want to discover and whether Akri currently supports a protocol
that can be used to discover resources of that type. To see the list of currently supported protocols, see our
[roadmap](./roadmap.md).

### Understanding Akri Helm charts
Akri is most easily deployed with Helm charts.  Helm charts provide convenient packaging and configuration.

Starting in v0.0.36, an **akri-dev** Helm chart will be published for each build version.  Each Akri build is verified
with end-to-end tests on Kubernetes, K3s, and MicroK8s.  These builds may be less stable than our Releases.  You can
deploy these versions of Akri with this command (note: **akri-dev**):
```sh
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri-dev
```

Starting in Release v0.0.44, an **akri** Helm chart will be published for each
[Release](https://github.com/deislabs/akri/releases).  Releases will generally reflect milestones and will have more
rigorous testing.  You can deploy Release versions of Akri with this command (note: **akri**):
```sh
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri
```

To use the latest containers of the Akri components, add `--set useLatestContainers=true` when installing Akri like so:
```sh
helm install akri akri-helm-charts/akri \
   --set useLatestContainers=true 
```

To see which version of the **akri** and **akri-dev** Helm charts are stored locally, run  `helm inspect chart akri-helm-charts/akri` and `helm inspect chart akri-helm-charts/akri-dev`, respectively.

To grab the latest Akri Helm charts, run `helm repo update`.

### Setting up your cluster
1. Before deploying Akri, you must have a Kubernetes (v1.16 or higher) cluster running and `kubectl` installed. All
   nodes must be Linux. All of the Akri component containers are currently built for amd64, arm64v8, or arm32v7, so all nodes must
   have one of these platforms.

### Deploying Akri
1. Install Helm
    ```sh
    curl -L https://raw.githubusercontent.com/helm/helm/master/scripts/get-helm-3 | bash
    ```
1. Provide runtime-specific configuration to enable Akri and Helm

    1. If using **K3s**, point to `kubeconfig` for Helm, install crictl, and configure Akri to use K3s' CRI socket.
        ```sh
        # Install crictl locally (note: there are no known version limitations, any crictl version is expected to work). 
        # This step is not necessary if using a K3s version below 1.19, in which case K3s' embedded crictl can be used.
        VERSION="v1.17.0"
        curl -L https://github.com/kubernetes-sigs/cri-tools/releases/download/$VERSION/crictl-${VERSION}-linux-amd64.tar.gz --output crictl-${VERSION}-linux-amd64.tar.gz
        sudo tar zxvf crictl-$VERSION-linux-amd64.tar.gz -C /usr/local/bin
        rm -f crictl-$VERSION-linux-amd64.tar.gz

        # Helm uses $KUBECONFIG to find the Kubernetes configuration
        export KUBECONFIG=/etc/rancher/k3s/k3s.yaml

        # Configure Akri to use K3s' embedded crictl and CRI socket
        export AKRI_HELM_CRICTL_CONFIGURATION="--set agent.host.crictl=/usr/local/bin/crictl --set agent.host.dockerShimSock=/run/k3s/containerd/containerd.sock"
        ```
    1. If using **MicroK8s**, enable CoreDNS, RBAC (optional), and Helm. If your broker Pods must run privileged, enable
       privileged Pods. Also, install crictl, and configure Akri to use MicroK8s' CRI socket.
        ```sh
        # Enable CoreDNS, RBAC and Helm
        microk8s enable dns rbac helm3

        # Optionally enable privileged pods (if your broker Pods must run privileged) and restart MicroK8s.
        echo "--allow-privileged=true" >> /var/snap/microk8s/current/args/kube-apiserver
        sudo microk8s stop && microk8s start

        # Install crictl locally (note: there are no known version
        # limitations, any crictl version is expected to work)
        VERSION="v1.17.0"
        curl -L https://github.com/kubernetes-sigs/cri-tools/releases/download/$VERSION/crictl-${VERSION}-linux-amd64.tar.gz --output crictl-${VERSION}-linux-amd64.tar.gz
        sudo tar zxvf crictl-$VERSION-linux-amd64.tar.gz -C /usr/local/bin
        rm -f crictl-$VERSION-linux-amd64.tar.gz

        # Configure Akri to use MicroK8s' CRI socket
        export AKRI_HELM_CRICTL_CONFIGURATION="--set agent.host.crictl=/usr/local/bin/crictl --set agent.host.dockerShimSock=/var/snap/microk8s/common/run/containerd.sock"
        ```
        If you don't have existing kubectl and helm installations, you can add aliases. If you do not want to set an
        alias, add microk8s in front of all kubectl and helm commands.
        ```sh
        alias kubectl='microk8s kubectl'
        alias helm='microk8s helm3'
        ```
    1. If using **Kubernetes**, Helm and crictl do not require additional configuration.

1. When installing the Akri Helm chart, you can specify what Configuration to apply by specifying the protocol
   that will be used in the Configuration. This is done in the setting `--set <protocol>.enabled=true` below. Here,
   `<protocol>` could be `udev`, `onvif`, or `opcua`. Helm will automatically apply the default Configuration for that protocol to
   the cluster. You can set values in the Helm install command to customize the Configuration. To explore the values you
   can set, see our documentation on customizing the provided [ONVIF](./onvif-configuration.md),
   [udev](./udev-configuration.md), and [OPC UA](./opcua-configuration.md) Configuration templates.

    The Helm settings can also be used to customize where the Akri Controller runs. By default the Controller can be
    deployed to any control plane or worker node. These settings can be changed by adding extra settings when installing
    Akri below. If you don't want the Controller to ever be scheduled to control plane nodes, add `--set
    controller.allowOnControlPlane=false` to your install command below. Conversely, if you only want the Controller to
    run on control plane nodes, add `--set controller.onlyOnControlPlane=true`. This will guarantee the Controller only
    runs on nodes with the label (key, value) of (`node-role.kubernetes.io/master`, ""), which is the default label for
    the control plane node for Kubernetes.
    
    However, control plane nodes on MicroK8s and K3s may not have this exact label by
    default, so you can add it by running `kubectl label node ${HOSTNAME,,} node-role.kubernetes.io/master=
    --overwrite=true`. Or alternatively, in K3s, you can keep the default label value on the master and add `--set controller.nodeSelectors."node-role\.kubernetes\.io/master"=true` to the install command below.

    Run the following to fetch the Akri Helm chart, install Akri, and apply the default configuration for `<protocol>`,
    optionally specifying the image for the broker pod that should be deployed to utilize each discovered device.
    ```sh
    helm repo add akri-helm-charts https://deislabs.github.io/akri/
    helm install akri akri-helm-charts/akri \
        $AKRI_HELM_CRICTL_CONFIGURATION \
        --set <protocol>.enabled=true \
        # --set <protocol>.brokerPod.image.repository=<your broker image> \
        # apply any additional settings here
    ```
    > Note: set `<protocol>.brokerPod.image.tag` to specify an image tag (defaults to `latest`).

    Run `kubectl get crd`, and you should see Akri's two CRDs listed. Run `kubectl get pods -o wide`, and you should see
    the Akri Controller pod, Agent pods, and broker pods if a broker was specified. Run `kubectl get akric`, and you
    should see the Configuration for the protocol you specified.  If devices were discovered, the instances can be seen
    by running `kubectl get akrii` and further inspected by running `kubectl get akrii <protocol>-<ID> -o yaml`.
1. Delete the configuration and watch the instances, pods, and services (if you specified a broker image) be deleted.
    ```sh
    kubectl delete akric akri-<protocol>
    watch kubectl get pods,services,akric,akrii -o wide
    ```

### Modifying your Akri installation or deploying a custom Akri Configuration
See the [Customizing an Akri Installation document](./customizing-akri-installation.md) for more information on how to modify
your already deployed Akri installation or to specify a custom Akri Configuration.
