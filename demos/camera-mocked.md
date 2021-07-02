# Camera ⁠– Mocked

In this guide, we will walk through using Akri to discover mock USB cameras attached to nodes in a Kubernetes cluster. You'll see how Akri automatically deploys workloads to pull frames from the cameras. We will then deploy a streaming application that will point to services automatically created by Akri to access the video frames from the workloads.

The following will be covered in this demo: 

1. Setting up mock udev video devices
2. Setting up a cluster
3. Installing Akri via Helm with settings to create your Akri udev Configuration
4. Inspecting Akri
5. Deploying a streaming application
6. Cleanup
7. Going beyond the demo

## Setting up mock udev video devices

1. Acquire an Ubuntu 20.04 LTS, 18.04 LTS or 16.04 LTS environment to run the

   commands. If you would like to deploy the demo to a cloud-based VM, see the

   instructions for [DigitalOcean](end-to-end-demo-do.md) or \[Google Compute

   Engine\]\(end-to-end-demo-gce.md\) \(and you can skip the rest of the steps in

   this document\).

2. To setup fake usb video devices, install the v4l2loopback kernel module and its prerequisites. Learn more about v4l2 loopback [here](https://github.com/umlaeute/v4l2loopback)

   ```bash
    sudo apt update
    sudo apt -y install linux-headers-$(uname -r)
    sudo apt -y install linux-modules-extra-$(uname -r)
    sudo apt -y install dkms
    curl http://deb.debian.org/debian/pool/main/v/v4l2loopback/v4l2loopback-dkms_0.12.5-1_all.deb -o v4l2loopback-dkms_0.12.5-1_all.deb
    sudo dpkg -i v4l2loopback-dkms_0.12.5-1_all.deb
   ```

   > **Note** When running on Ubuntu 20.04 LTS, 18.04 LTS or 16.04 LTS, do NOT install v4l2loopback through `sudo apt install -y v4l2loopback-dkms`, you will get an older version \(0.12.3\). 0.12.5-1 is required for gstreamer to work properly.

> **Note**: If not able to install the debian package of v4l2loopback due to using a different Linux kernel, you can clone the repo, build the module, and setup the module dependencies like so:
>
> ```bash
> git clone https://github.com/umlaeute/v4l2loopback.git
> cd v4l2loopback
> make & sudo make install
> sudo make install-utils
> sudo depmod -a
> ```

1. "Plug-in" two cameras by inserting the kernel module. To create different number video devices modify the `video_nr` argument. 

   ```bash
    sudo modprobe v4l2loopback exclusive_caps=1 video_nr=1,2
   ```

2. Confirm that two video device nodes \(video1 and video2\) have been created.

   ```bash
    ls /dev/video*
   ```

3. Install the necessary Gstreamer packages.

   ```bash
    sudo apt-get install -y \
        libgstreamer1.0-0 gstreamer1.0-tools gstreamer1.0-plugins-base \
        gstreamer1.0-plugins-good gstreamer1.0-libav
   ```

4. Now that our cameras are set up, lets use Gstreamer to pass fake video streams through them.

   ```bash
    mkdir camera-logs
    sudo gst-launch-1.0 -v videotestsrc pattern=ball ! "video/x-raw,width=640,height=480,framerate=10/1" ! avenc_mjpeg ! v4l2sink device=/dev/video1 > camera-logs/ball.log 2>&1 &
    sudo gst-launch-1.0 -v videotestsrc pattern=smpte horizontal-speed=1 ! "video/x-raw,width=640,height=480,framerate=10/1" ! avenc_mjpeg ! v4l2sink device=/dev/video2 > camera-logs/smpte.log 2>&1 &
   ```

   > **Note**: If this generates an error, be sure that there are no existing video streams targeting the video device nodes by running the following and then re-running the previous command:
   >
   > ```bash
   > if pgrep gst-launch-1.0 > /dev/null; then
   >   sudo pkill -9 gst-launch-1.0
   > fi
   > ```

## Setting up a cluster

Reference our [cluster setup documentation](setting-up-cluster.md) to set up a cluster for this demo. For ease of setup, only create single-node cluster, so if installing K3s or MicroK8s, you can skip the last step of the installation instructions of adding additional nodes. If you have an existing cluster, feel free to leverage it for the demo. This documentation assumes you are using a single-node cluster; however, you can certainly use a multi-node cluster. You will see additional Akri Agents and Discovery Handlers deployed [when inspecting the Akri installation]().

> Note, if using MicroK8s, enable privileged Pods, as the udev video broker pods run privileged to easily grant them access to video devices. More explicit device access could have been configured by setting the appropriate [security context](udev-configuration.md#setting-the-broker-pod-security-context) in the broker PodSpec in the Configuration.

## Installing Akri

You tell Akri what you want to find with an Akri Configuration, which is one of Akri's Kubernetes custom resources. The Akri Configuration is simply a `yaml` file that you apply to your cluster. Within it, you specify three things: 1. a Discovery Handler 2. any additional device filtering 3. an image for a Pod \(that we call a "broker"\) that you want to be automatically deployed to utilize each discovered device

For this demo, we will specify \(1\) Akri's udev Discovery Handler, which is used to discover devices in the Linux device file system. Akri's udev Discovery Handler supports \(2\) filtering by udev rules. We want to find all video devices in the Linux device file system, which can be specified with the udev rule `KERNEL=="video[0-9]*"`. Say we wanted to be more specific and only discover devices made by Great Vendor, we could adjust our rule to be `KERNEL=="video[0-9]*"\, ENV{ID_VENDOR}=="Great Vendor"`. For \(3\) a broker Pod image, we will use a sample container that Akri has provided that pulls frames from the cameras and serves them over gRPC.

All of Akri's components can be deployed by specifying values in its Helm chart during an installation. Instead of having to build a Configuration from scratch, Akri has provided [Helm templates](../deployment/helm/templates) for Configurations for each supported Discovery Handler. Lets customize the generic [udev Configuration Helm template](../deployment/helm/templates/udev-configuration.yaml) with our three specifications above. We can also set the name for the Configuration to be `akri-udev-video`. Also, if using MicroK8s or K3s, configure the crictl path and socket using the `AKRI_HELM_CRICTL_CONFIGURATION` variable created when setting up your cluster.

In order for the Agent to know how to discover video devices, the udev Discovery Handler must exist. Akri supports an Agent image that includes all supported Discovery Handlers. This Agent will be used if `agent.full=true` is set. By default, a slim Agent without any embedded Discovery Handlers is deployed and the required Discovery Handlers can be deployed as DaemonSets. This demo will use that strategy, deploying the udev Discovery Handlers by specifying `udev.discovery.enabled=true` when installing Akri.

1. Add the Akri Helm chart and run the install command, setting Helm values as described above.

   ```bash
    helm repo add akri-helm-charts https://deislabs.github.io/akri/
    helm install akri akri-helm-charts/akri \
        $AKRI_HELM_CRICTL_CONFIGURATION \
        --set udev.discovery.enabled=true \
        --set udev.configuration.enabled=true \
        --set udev.configuration.name=akri-udev-video \
        --set udev.configuration.discoveryDetails.udevRules[0]='KERNEL=="video[0-9]*"' \
        --set udev.configuration.brokerPod.image.repository="ghcr.io/deislabs/akri/udev-video-broker"
   ```

## Inspecting Akri

After installing Akri, since the /dev/video1 and /dev/video2 devices are running on this node, the Akri Agent will discover them and create an Instance for each camera.

1. List all that Akri has automatically created and deployed, namely Akri Configuration we created when installing Akri, two Instances \(which are the Akri custom resource that represents each device\), two broker Pods \(one for each camera\), a service for each broker Pod, a service for all brokers, the Controller Pod, Agent Pod, and the udev Discovery Handler Pod.

   ```bash
    watch microk8s kubectl get pods,akric,akrii,services -o wide
   ```

   For K3s and vanilla Kubernetes

   ```bash
    watch kubectl get pods,akric,akrii,services -o wide
   ```

   Look at the Configuration and Instances in more detail.

2. Inspect the Configuration that was created via the Akri udev Helm template and values that were set when installing Akri by running the following.

   ```bash
    kubectl get akric -o yaml
   ```

3. Inspect the two Instances. Notice that in the `brokerProperties` of each instance, you can see the device nodes \(`/dev/video1` or `/dev/video2`\) that the Instance represents. The `brokerProperties` of an Instance are set as environment variables in the broker Pods that are utilizing the device the Instance represents. This told the broker which device to connect to. We can also see in the Instance a usage slot and that it was reserved for this node. Each Instance represents a device and its usage.

   ```bash
    kubectl get akrii -o yaml
   ```

    If this was a shared device \(such as an IP camera\), you may have wanted to increase the number of nodes that could use the same device by specifying `capacity`. There is a `capacity` parameter for each Configuration, which defaults to `1`. Its value could have been increased when installing Akri \(via `--set <discovery handler name>.configuration.capacity=2` to allow 2 nodes to use the same device\) and more usage slots \(the number of usage slots is equal to `capacity`\) would have been created in the Instance. 

   **Deploying a streaming application**

4. Deploy a video streaming web application that points to both the Configuration and Instance level services that were automatically created by Akri.

   ```bash
    kubectl apply -f https://raw.githubusercontent.com/deislabs/akri/main/deployment/samples/akri-video-streaming-app.yaml
   ```

    For MicroK8s

   ```bash
    watch microk8s kubectl get pods
   ```

    For K3s and vanilla Kubernetes

   ```bash
    watch kubectl get pods
   ```

5. Determine which port the service is running on. Be sure to save this port number for the next step.

   ```bash
   kubectl get service/akri-video-streaming-app --output=jsonpath='{.spec.ports[?(@.name=="http")].nodePort}' && echo
   ```

6. SSH port forwarding can be used to access the streaming application. In a new terminal, enter your ssh command to to access your VM followed by the port forwarding request. The following command will use port 50000 on the host. Feel free to change it if it is not available. Be sure to replace `<streaming-app-port>` with the port number outputted in the previous step. 

   ```bash
   ssh someuser@<Ubuntu VM IP address> -L 50000:localhost:<streaming-app-port>
   ```

   > **Note** we've noticed issues with port forwarding with WSL 2. Please use a different terminal.

7. Navigate to `http://localhost:50000/`. The large feed points to Configuration level service \(`udev-camera-svc`\), while the bottom feed points to the service for each Instance or camera \(`udev-camera-svc-<id>`\).

## Cleanup

1. Bring down the streaming service.

   ```bash
    kubectl delete service akri-video-streaming-app
    kubectl delete deployment akri-video-streaming-app
   ```

    For MicroK8s

   ```bash
    watch microk8s kubectl get pods
   ```

    For K3s and vanilla Kubernetes

   ```bash
    watch kubectl get pods
   ```

2. Delete the configuration, and watch the associated instances, pods, and services be deleted.

   ```bash
    kubectl delete akric akri-udev-video
   ```

    For MicroK8s

   ```bash
    watch microk8s kubectl get pods,services,akric,akrii -o wide
   ```

    For K3s and vanilla Kubernetes

   ```bash
    watch kubectl get pods,services,akric,akrii -o wide
   ```

3. If you are done using Akri, it can be uninstalled via Helm.

   ```bash
    helm delete akri
   ```

4. Delete Akri's CRDs.

   ```bash
    kubectl delete crd instances.akri.sh
    kubectl delete crd configurations.akri.sh
   ```

5. Stop video streaming from the video devices.

   ```bash
    if pgrep gst-launch-1.0 > /dev/null; then
        sudo pkill -9 gst-launch-1.0
    fi
   ```

6. "Unplug" the fake video devices by removing the kernel module.

   ```bash
    sudo modprobe -r v4l2loopback
   ```

## Going beyond the demo

1. Plug in real cameras! You can [pass environment variables](broker-development#Specifying-additional-broker-environment-variables-in-a-Configuration) to the frame server broker to specify the format, resolution width/height, and frames per second of your cameras.
2. Apply the [ONVIF Configuration](onvif-configuration.md) and make the streaming app display footage from both the local video devices and onvif cameras. To do this, modify the [video streaming yaml](../deployment/samples/akri-video-streaming-app.yaml) as described in the inline comments in order to create a larger service that aggregates the output from both the `udev-camera-svc` service and `onvif-camera-svc` service.
3. Add more nodes to the cluster.
4. [Modify the udev rule](udev-video-sample.md#modifying-the-udev-rule) to find a more specific subset of cameras.
5. Discover other udev devices by creating a new udev configuration and broker. Learn more about the udev Discovery Handler Configuration [here](udev-configuration.md).

