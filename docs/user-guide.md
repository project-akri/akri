# User Guide
To best understand the benefits of Akri and jump into using it, we recommend you start off by completing the [end to end demo](./end-to-end-demo.md). In the demo, you will see Akri discover mock video cameras and a streaming app display the footage from those cameras. It includes instructions on K8s cluster setup. If you would like to perform the demo on a cluster of Raspberry Pi 4's, see the [Raspberry Pi 4 demo](./rpi4-demo.md).

## Getting Started
To get started using Akri, you must first decide what you want to discover and whether Akri current supports a protocol that can be used to discover resources of that type. To see the list of currently supported protocols, see our [roadmap](./roadmap.md).

### Setting up your cluster
1. Before deploying Akri, you must have a Kubernetes cluster running and `kubectl` installed. All nodes must be Linux. All of the Akri component containers are currently built for amd64 or arm64v8, so all nodes must have one of these platforms.

1. Set up role-based access control (RBAC): Since Akri does not have RBAC policy set up yet, for now, grant all pods admin access.
    ```sh
    kubectl create clusterrolebinding serviceaccounts-cluster-admin --clusterrole=cluster-admin --group=system:serviceaccounts
    ```

If you are using [MicroK8s](https://microk8s.io/docs), be sure to allow privileged pods and label the master node:
1. Enable privileged pods and restart MicroK8s.
    ```sh
    echo "--allow-privileged=true" >> /var/snap/microk8s/current/args/kube-apiserver
    microk8s.stop
    microk8s.start
    ```
 1. Since MicroK8s by default does not have a node with the label `node-role.kubernetes.io/master=`, add the label to the control plane node so the controller gets scheduled.
    ```sh
    kubectl label node $HOSTNAME node-role.kubernetes.io/master= --overwrite=true
    ```

If you are using [K3s](https://k3s.io/), modify the control plane node(s) `node-role.kubernetes.io/master=true` label to remove the `true` value:
```sh
kubectl label node $HOSTNAME node-role.kubernetes.io/master= --overwrite=true
```

### Deploying Akri
1. Install Helm
    ```sh
    curl -L https://raw.githubusercontent.com/helm/helm/master/scripts/get-helm-3 | bash
    ```
1. If using MicroK8s, enable Helm.
    ```sh
    kubectl config view --raw >~/.kube/config
    chmod go-rwx ~/.kube/config
    microk8s enable helm3
    ```
1. If using K3s, point to `kubeconfig` for Helm.
    ```sh
    export KUBECONFIG=/etc/rancher/k3s/k3s.yaml
    ```
1. Install Akri Helm chart and enable the desired Configuration (in this case, ONVIF is enabled). See the [ONVIF Configuration documentation](./onvif-sample.md) to learn how to customize the Configuration. Instructions on deploying the udev Configuration can be found in [this document](./udev-sample.md).
    ```sh
    helm repo add akri-helm-charts https://deislabs.github.io/akri/
    helm install akri akri-helm-charts/akri \
        --set useLatestContainers=true \
        --set onvifVideo.enabled=true
    watch kubectl get pods,akric,akrii -o wide
    ```
    Run `kubectl get crd`, and you should see the crds listed.
    Run `kubectl get pods -o wide`, and you should see the Akri pods.
    Run `kubectl get akric`, and you should see `onvif-camera`. If IP cameras were discovered and pods spun up, the instances can be seen by running `kubectl get akrii` and further inspected by running `kubectl get akrii onvif-camera-<ID> -o yaml`
1. Delete the configuration and watch the instances, pods, and services be deleted.
    ```sh
    kubectl delete akric akri-onvif-video
    watch kubectl get pods,services,akric,akrii -o wide
    ```

### Modifying your Akri installation
See the [Modifying a Akri Installation document](./modifying-akri-installation.md) for more information on how to modify your already deployed Akri installation.