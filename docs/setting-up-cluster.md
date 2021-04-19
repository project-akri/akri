# Setting up your cluster
Before deploying Akri, you must have a Kubernetes cluster (v1.16 or higher) running with `kubectl` and `Helm` installed. Akri is Kubernetes native, so it should run on most Kubernetes distributions. All of our end-to-end tests run on vanilla Kubernetes, K3s, and MicroK8s clusters. This documentation will walk through how to set up a cluster using one of those three distributions.

>Note: All nodes must be Linux on amd64, arm64v8, or arm32v7.

## Set up a standard Kubernetes cluster
1. Reference [Kubernetes documentation](https://kubernetes.io/docs/tasks/tools/) for instructions on how to install Kubernetes. 
1. Install Helm for deploying Akri.
    ```sh
    sudo apt install -y curl
    curl -L https://raw.githubusercontent.com/helm/helm/master/scripts/get-helm-3 | bash
    ```

> Note: To enable workloads on a single-node cluster, remove the master taint.
> ```sh
> kubectl taint nodes --all node-role.kubernetes.io/master-
> ```

## Set up a K3s cluster
1. Install [K3s](https://k3s.io/)
    ```sh
      curl -sfL https://get.k3s.io | sh -
    ```
    
    >Note: Optionally specify a version with the `INSTALL_K3S_VERSION` env var as follows: `curl -sfL https://get.k3s.io | INSTALL_K3S_VERSION=v1.18.9+k3s1 sh -`
1. Grant admin privilege to access kube config.
    ```sh
    sudo addgroup k3s-admin
    sudo adduser $USER k3s-admin
    sudo usermod -a -G k3s-admin $USER
    sudo chgrp k3s-admin /etc/rancher/k3s/k3s.yaml
    sudo chmod g+r /etc/rancher/k3s/k3s.yaml
    su - $USER
    ```
1. Check K3s status.
    ```sh
    kubectl get node
    ```
1. Install Helm.
    ```sh
    export KUBECONFIG=/etc/rancher/k3s/k3s.yaml
    sudo apt install -y curl
    curl -L https://raw.githubusercontent.com/helm/helm/master/scripts/get-helm-3 | bash
    ```
1. Akri depends on crictl to track some Pod information. If using K3s versions 1.19 or greater, install crictl locally (note: there are no known version limitations, any crictl version is expected to work). Previous K3s versions come when crictl embedded.
    ```sh
        VERSION="v1.17.0"
        curl -L https://github.com/kubernetes-sigs/cri-tools/releases/download/$VERSION/crictl-${VERSION}-linux-amd64.tar.gz --output crictl-${VERSION}-linux-amd64.tar.gz
        sudo tar zxvf crictl-$VERSION-linux-amd64.tar.gz -C /usr/local/bin
        rm -f crictl-$VERSION-linux-amd64.tar.gz
    ```
1. Configure Akri to use the crictl path and K3s containerd socket. This `AKRI_HELM_CRICTL_CONFIGURATION` environment variable should be added to all Akri Helm installations. 
    ```sh
    export AKRI_HELM_CRICTL_CONFIGURATION="--set agent.host.crictl=/usr/local/bin/crictl --set agent.host.dockerShimSock=/run/k3s/containerd/containerd.sock"
    ```
1. Add nodes to your cluster by running the K3s installation script with the `K3S_URL` and `K3S_TOKEN` environment variables. See [K3s installation documentation](https://rancher.com/docs/k3s/latest/en/quick-start/#install-script) for more details.

## Set up a MicroK8s cluster
1. Install [MicroK8s](https://microk8s.io/docs).
    ```sh
    sudo snap install microk8s --classic --channel=1.19/stable
    ```
1. Grant admin privilege for running MicroK8s commands.
    ```sh
    sudo usermod -a -G microk8s $USER
    sudo chown -f -R $USER ~/.kube
    su - $USER
    ```
1. Check MicroK8s status.
    ```sh
    microk8s status --wait-ready
    ```
1. Enable CoreDNS, Helm and RBAC for MicroK8s.
    ```sh
    microk8s enable dns helm3 rbac
    ```
1. If you don't have an existing `kubectl` and `helm` installations, add aliases. If you do not want to set an alias, add `microk8s` in front of all `kubectl` and `helm` commands.
    ```sh
    alias kubectl='microk8s kubectl'
    alias helm='microk8s helm3'
    ```
1. By default, MicroK8s does not allow Pods to run in a privileged context. None of Akri's components run privileged; however, if your custom broker Pods do in order to access devices for example, enable privileged Pods like so:
    ```sh
    echo "--allow-privileged=true" >> /var/snap/microk8s/current/args/kube-apiserver
    microk8s.stop
    microk8s.start
    ```
1. Akri depends on crictl to track some Pod information. MicroK8s does not install crictl locally, so crictl must be installed and the Akri Helm chart needs to be configured with the crictl path and MicroK8s containerd socket.
    ```sh
    # Note that we aren't aware of any version restrictions
    VERSION="v1.17.0"
    curl -L https://github.com/kubernetes-sigs/cri-tools/releases/download/$VERSION/crictl-${VERSION}-linux-amd64.tar.gz --output crictl-${VERSION}-linux-amd64.tar.gz
    sudo tar zxvf crictl-$VERSION-linux-amd64.tar.gz -C /usr/local/bin
    rm -f crictl-$VERSION-linux-amd64.tar.gz

    export AKRI_HELM_CRICTL_CONFIGURATION="--set agent.host.crictl=/usr/local/bin/crictl --set agent.host.dockerShimSock=/var/snap/microk8s/common/run/containerd.sock"
    ```
1. To add additional nodes to the cluster, reference [MicroK8's documentation](https://microk8s.io/docs/clustering).
