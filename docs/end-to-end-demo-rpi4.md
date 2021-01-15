# Raspberry Pi 4 Demo   
This will demonstrate how to get Akri working on a **Raspberry Pi 4** and walk through using Akri to discover mock USB cameras attached to nodes in a Kubernetes cluster. You'll see how Akri automatically deploys workloads to pull frames from the cameras. We will then deploy a streaming application that will point to services automatically created by Akri to access the video frames from the workloads.

The following will be covered in this demo:
1. Setting up single node cluster on a Raspberry Pi 4
1. Setting up mock udev video devices
1. Installing Akri via Helm with settings to create your Akri udev Configuration
1. Inspecting Akri
1. Deploying a streaming application
1. Cleanup
1. Going beyond the demo

## Set up single node cluster on a Raspberry Pi 4
1. Using instructions found [here](https://ubuntu.com/download/raspberry-pi), download 64-bit Ubuntu:18.04
1. Using the instructions found [here](https://ubuntu.com/download/raspberry-pi/thank-you?version=18.04&versionPatch=.4&architecture=arm64+raspi3), apply the Ubuntu image to an SD card.
1. Plug in SD card and start Raspberry Pi 4.
1. Install docker.
    ```sh
    sudo apt install -y docker.io
    ```
1. Install Helm.
    ```sh
    sudo apt install -y curl
    curl -L https://raw.githubusercontent.com/helm/helm/master/scripts/get-helm-3 | bash
    ```
1. Install Kubernetes.
    ```sh
    curl -s https://packages.cloud.google.com/apt/doc/apt-key.gpg | sudo apt-key add
    sudo apt-add-repository "deb http://apt.kubernetes.io/ kubernetes-xenial main"
    sudo apt install -y kubectl kubeadm kubelet
    ```
1. Enable cgroup memory by appending `cgroup_enable=cpuset` and `cgroup_enable=memory cgroup_memory=1` to this file: `/boot/firmware/nobtcmd.txt`
1. Start master node
    ```sh
	sudo kubeadm init
    ```
1. To enable workloads on our single-node cluster, remove the master taint.
    ```sh
    kubectl taint nodes --all node-role.kubernetes.io/master-
    ```
1. Apply a network provider to the cluster.
    ```sh
    kubectl apply -f "https://cloud.weave.works/k8s/net?k8s-version=$(kubectl version | base64 | tr -d '\n')"
    ```

## Set up mock udev video devices
1. Open a new terminal and ssh into your ubuntu server that your cluster is running on.
1. To setup fake usb video devices, install the v4l2loopback kernel module and its prerequisites. Learn more about v4l2 loopback [here](https://github.com/umlaeute/v4l2loopback)
    ```sh
    curl http://deb.debian.org/debian/pool/main/v/v4l2loopback/v4l2loopback-dkms_0.12.5-1_all.deb -o v4l2loopback-dkms_0.12.5-1_all.deb 
    sudo dpkg -i v4l2loopback-dkms_0.12.5-1_all.deb
    ```
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
## Installing Akri
You tell Akri what you want to find with an Akri Configuration, which is one of Akri's Kubernetes custom resources. The Akri Configuration is simply a `yaml` file that you apply to your cluster. Within it, you specify three things: 
1. a discovery protocol
2. any additional device filtering
3. an image for a Pod (that we call a "broker") that you want to be automatically deployed to utilize each discovered device

For this demo, we will specify (1) Akri's udev discovery protocol, which is used to discover devices in the Linux device file system. Akri's udev discovery protocol supports (2) filtering by udev rules. We want to find all video devices in the Linux device file system, which can be specified by the udev rule `KERNEL=="video[0-9]*"`. Say we wanted to be more specific and only discover devices made by Great Vendor, we could adjust our rule to be `KERNEL=="video[0-9]*"\, ENV{ID_VENDOR}=="Great Vendor"`. For (3) a broker Pod image, we will use a sample container that Akri has provided that pulls frames from the cameras and serves them over gRPC. 

Instead of having to build a Configuration from scratch, Akri has provided [Helm templates](../deployment/helm/templates) for each supported discovery protocol. Lets customize the generic [udev Helm template](../deployment/helm/templates/udev.yaml) with our three specifications above. We can also set the name for the Configuration to be `akri-udev-video`.

1. Add the Akri Helm chart and run the install command, setting Helm values as described above.
    ```sh
    helm repo add akri-helm-charts https://deislabs.github.io/akri/
    helm install akri akri-helm-charts/akri \
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
    watch kubectl get pods
    ```
1. Delete the configuration, and watch the associated instances, pods, and services be deleted.
    ```sh
    kubectl delete akric akri-udev-video
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
