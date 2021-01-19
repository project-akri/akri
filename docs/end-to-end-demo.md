# End-to-End Demo
In this guide, we will walk through using Akri to discover mock USB cameras attached to nodes in a Kubernetes cluster. You'll see how Akri automatically deploys workloads to pull frames from the cameras. We will then deploy a streaming application that will point to services automatically created by Akri to access the video frames from the workloads.

The following will be covered in this demo:
1. Setting up mock udev video devices
1. Setting up a cluster
1. Installing Akri via Helm with settings to create your Akri udev Configuration
1. Inspecting Akri
1. Deploying a streaming application
1. Cleanup
1. Going beyond the demo

## Setting up mock udev video devices
1. Acquire an Ubuntu 20.04 LTS, 18.04 LTS or 16.04 LTS environment to run the
   commands. If you would like to deploy the demo to a cloud-based VM, see the
   instructions for [DigitalOcean](end-to-end-demo-do.md) or [Google Compute
   Engine](end-to-end-demo-gce.md) (and you can skip the rest of the steps in
   this document).
1. To setup fake usb video devices, install the v4l2loopback kernel module and its prerequisites. Learn more about v4l2 loopback [here](https://github.com/umlaeute/v4l2loopback)
    ```sh
    sudo apt update
    sudo apt -y install linux-modules-extra-$(uname -r)
    sudo apt -y install dkms
    curl http://deb.debian.org/debian/pool/main/v/v4l2loopback/v4l2loopback-dkms_0.12.5-1_all.deb -o v4l2loopback-dkms_0.12.5-1_all.deb
    sudo dpkg -i v4l2loopback-dkms_0.12.5-1_all.deb
    ```
    > **Note** When running on Ubuntu 20.04 LTS, 18.04 LTS or 16.04 LTS, do NOT install 
    > v4l2loopback  through `sudo apt install -y v4l2loopback-dkms`, you will get an older version (0.12.3). 
    > 0.12.5-1 is required for gstreamer to work properly.


    > **Note**: If not able to install the debian package of v4l2loopback due to using a different
    > Linux kernel, you can clone the repo, build the module, and setup the module dependencies 
    > like so:
    > ```sh
    > git clone https://github.com/umlaeute/v4l2loopback.git
    > cd v4l2loopback
    > make & sudo make install
    > sudo make install-utils
    > sudo depmod -a  
    > ```
    
1. "Plug-in" two cameras by inserting the kernel module. To create different number video devices modify the `video_nr` argument. 
    ```sh
    sudo modprobe v4l2loopback exclusive_caps=1 video_nr=1,2
    ```
1. Confirm that two video device nodes (video1 and video2) have been created.
    ```sh
    ls /dev/video*
    ```
1. Install the necessary Gstreamer packages.
    ```sh
    sudo apt-get install -y \
        libgstreamer1.0-0 gstreamer1.0-tools gstreamer1.0-plugins-base \
        gstreamer1.0-plugins-good gstreamer1.0-libav
    ```
1. Now that our cameras are set up, lets use Gstreamer to pass fake video streams through them.
    ```sh
    sudo gst-launch-1.0 -v videotestsrc pattern=ball ! "video/x-raw,width=640,height=480,framerate=10/1" ! avenc_mjpeg ! v4l2sink device=/dev/video1 > camera-logs/ball.log 2>&1 &
    sudo gst-launch-1.0 -v videotestsrc pattern=smpte horizontal-speed=1 ! "video/x-raw,width=640,height=480,framerate=10/1" ! avenc_mjpeg ! v4l2sink device=/dev/video2 > camera-logs/smpte.log 2>&1 &
    ```
    > **Note**: If this generates an error, be sure that there are no existing video streams targeting the video device nodes by running the following and then re-running the previous command:
    > ```sh
    > if pgrep gst-launch-1.0 > /dev/null; then
    >   sudo pkill -9 gst-launch-1.0
    > fi
    > ```

## Setting up a cluster

**Note:** Feel free to deploy on any Kubernetes distribution. Here, find instructions for K3s and MicroK8s. Select and
carry out one or the other (or adapt to your distribution), then continue on with the rest of the steps. 

### Option 1: Set up single node cluster using K3s
1. Install [K3s](https://k3s.io/) v1.18.9+k3s1.
    ```sh
    curl -sfL https://get.k3s.io | INSTALL_K3S_VERSION=v1.18.9+k3s1 sh -
    ```
1. Grant admin privilege to access kubeconfig.
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
1. K3s uses its own embedded crictl, so we need to configure the Akri Helm chart with the k3s crictl path and socket.
    ```sh
    export AKRI_HELM_CRICTL_CONFIGURATION="--set agent.host.crictl=/usr/local/bin/crictl --set agent.host.dockerShimSock=/run/k3s/containerd/containerd.sock"
    ```

### Option 2: Set up single node cluster using MicroK8s
1. Install [MicroK8s](https://microk8s.io/docs).
    ```sh
    sudo snap install microk8s --classic --channel=1.18/stable
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
1. For the sake of this demo, the udev video broker pods run privileged to easily grant them access to video devices, so
   enable privileged pods and restart MicroK8s. More explicit device access could have been configured by setting the
   appropriate [security context](udev-configuration.md#setting-the-broker-pod-security-context) in the broker PodSpec
   in the Configuration.
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

## Installing Akri
You tell Akri what you want to find with an Akri Configuration, which is one of Akri's Kubernetes custom resources. The Akri Configuration is simply a `yaml` file that you apply to your cluster. Within it, you specify three things: 
1. a discovery protocol
2. any additional device filtering
3. an image for a Pod (that we call a "broker") that you want to be automatically deployed to utilize each discovered device

For this demo, we will specify (1) Akri's udev discovery protocol, which is used to discover devices in the Linux device file system. Akri's udev discovery protocol supports (2) filtering by udev rules. We want to find all video devices in the Linux device file system, which can be specified by the udev rule `KERNEL=="video[0-9]*"`. Say we wanted to be more specific and only discover devices made by Great Vendor, we could adjust our rule to be `KERNEL=="video[0-9]*"\, ENV{ID_VENDOR}=="Great Vendor"`. For (3) a broker Pod image, we will use a sample container that Akri has provided that pulls frames from the cameras and serves them over gRPC. 

Instead of having to build a Configuration from scratch, Akri has provided [Helm templates](../deployment/helm/templates) for each supported discovery protocol. Lets customize the generic [udev Helm template](../deployment/helm/templates/udev.yaml) with our three specifications above. We can also set the name for the Configuration to be `akri-udev-video`. Also, if using MicroK8s or K3s, configure the crictl path and socket using the `AKRI_HELM_CRICTL_CONFIGURATION` variable created when setting up your cluster. 

1. Add the Akri Helm chart and run the install command, setting Helm values as described above.
    ```sh
    helm repo add akri-helm-charts https://deislabs.github.io/akri/
    helm install akri akri-helm-charts/akri \
        $AKRI_HELM_CRICTL_CONFIGURATION \
        --set useLatestContainers=true \
        --set udev.enabled=true \
        --set udev.name=akri-udev-video \
        --set udev.udevRules[0]='KERNEL=="video[0-9]*"' \
        --set udev.brokerPod.image.repository="ghcr.io/deislabs/akri/udev-video-broker:latest-dev"
    ```

## Inspecting Akri
After installing Akri, since the /dev/video1 and /dev/video2 devices are running on this node, the Akri Agent will discover them and create an Instance for each camera. 

1. List all that Akri has automatically created and deployed, namely the Akri Configuration we created when installing Akri, two Instances (which are the Akri custom resource that represents each device), two broker Pods (one for each camera), a service for each broker Pod, and a service for all brokers.

    ```sh
    watch microk8s kubectl get pods,akric,akrii,services -o wide
    ```
    For K3s and vanilla Kubernetes
    ```sh
    watch kubectl get pods,akric,akrii,services -o wide
    ```
Look at the Configuration and Instances in more detail. 
1. Inspect the Configuration that was created via the Akri udev Helm template and values that were set when installing Akri by running the following.
    ```sh
    kubectl get akric -o yaml
    ```
1. Inspect the two Instances. Notice that in the metadata of each instance, you can see the device nodes (`/dev/video1` or `/dev/video2`) that the Instance represents. This metadata of each Instance was passed to it's broker Pod as an environment variable. This told the broker which device to connect to. We can also see in the Instance a usage slot and that it was reserved for this node. Each Instance represents a device and its usage.
    ```sh 
    kubectl get akrii -o yaml
    ```
    If this was a shared device (such as an IP camera), you may have wanted to increase the number of nodes that could use the same device by specifying `capacity`. There is a `capacity` parameter for each protocol, which defaults to `1`. Its value could have been increased when installing Akri (via `--set <protocol>.capacity=2` to allow 2 nodes to use the same device) and more usage slots (the number of usage slots is equal to `capacity`) would have been created in the Instance. 
## Deploying a streaming application
1. Deploy a video streaming web application that points to both the Configuration and Instance level services that were automatically created by Akri.
    ```sh
    kubectl apply -f https://raw.githubusercontent.com/deislabs/akri/main/deployment/samples/akri-video-streaming-app.yaml
    watch kubectl get pods
    ```
1. Determine which port the service is running on. Be sure to save this port number for the next step.
    ```sh
   kubectl get service/akri-video-streaming-app --output=jsonpath='{.spec.ports[?(@.name=="http")].nodePort}' && echo
   ```
1.  SSH port forwarding can be used to access the streaming application. In a new terminal, enter your ssh command to to access your VM followed by the port forwarding request. The following command will use port 50000 on the host. Feel free to change it if it is not available. Be sure to replace `<streaming-app-port>` with the port number outputted in the previous step. 
    ```sh
    ssh someuser@<Ubuntu VM IP address> -L 50000:localhost:<streaming-app-port>
    ```
    > **Note** we've noticed issues with port forwarding with WSL 2. Please use a different terminal.
1. Navigate to `http://localhost:50000/`. The large feed points to Configuration level service (`udev-camera-svc`), while the bottom feed points to the service for each Instance or camera (`udev-camera-svc-<id>`).


## Cleanup 
1. Bring down the streaming service.
    ```sh
    kubectl delete service akri-video-streaming-app
    kubectl delete deployment akri-video-streaming-app
    ```
    For MicroK8s
    ```sh
    watch microk8s kubectl get pods
    ```
    For K3s and vanilla Kubernetes
    ```sh
    watch kubectl get pods
    ```
1. Delete the configuration, and watch the associated instances, pods, and services be deleted.
    ```sh
    kubectl delete akric akri-udev-video
    ```
    For MicroK8s
    ```sh
    watch microk8s kubectl get pods,services,akric,akrii -o wide
    ```
    For K3s and vanilla Kubernetes
    ```sh
    watch kubectl get pods,services,akric,akrii -o wide
    ```
1. If you are done using Akri, it can be uninstalled via Helm.
    ```sh
    helm delete akri
    ```
1. Delete Akri's CRDs.
    ```sh
    kubectl delete crd instances.akri.sh
    kubectl delete crd configurations.akri.sh
    ```
1. Stop video streaming from the video devices.
    ```sh
    if pgrep gst-launch-1.0 > /dev/null; then
        sudo pkill -9 gst-launch-1.0
    fi
    ```
1. "Unplug" the fake video devices by removing the kernel module.
    ```sh
    sudo modprobe -r v4l2loopback
    ```

## Going beyond the demo
1. Plug in real cameras! You can [pass environment variables](./udev-video-sample.md#modifying-the-brokerpod-spec) to the frame server broker to specify the format, resolution width/height, and frames per second of your cameras.
1. Apply the [ONVIF configuration](onvif-configuration.md) and make the streaming app display footage from both the local video devices and onvif cameras. To do this, modify the [video streaming yaml](../deployment/samples/akri-video-streaming-app.yaml) as described in the inline comments in order to create a larger service that aggregates the output from both the `udev-camera-svc` service and `onvif-camera-svc` service.
1. Add more nodes to the cluster.
1. [Modify the udev rule](udev-video-sample.md#modifying-the-udev-rule) to find a more specific subset of cameras.
1. Discover other udev devices by creating a new udev configuration and broker. Learn more about the udev protocol [here](udev-configuration.md).
