# Raspberry Pi 4 Demo   
This demo will demonstrate how to get Akri working on a **Raspberry Pi 4**, all the way from discovering local video cameras to the footage being streamed on a web application. This will show how Akri can dynamically discover devices, deploy brokers pods to perform some action on a device (in this case grabbing video frames and serving them over gRPC), and deploy broker services for obtaining the results of that action.

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
1. Install a kernel module to make v4l2 loopback video devices. Learn more about this module [here](https://github.com/umlaeute/v4l2loopback).
    ```sh
    curl http://deb.debian.org/debian/pool/main/v/v4l2loopback/v4l2loopback-dkms_0.12.5-1_all.deb -o v4l2loopback-dkms_0.12.5-1_all.deb 
    sudo dpkg -i v4l2loopback-dkms_0.12.5-1_all.deb
    ```
1. Insert the kernel module, creating /dev/video1 and /dev/video2 devnodes. To create different number video devices modify the `video_nr` argument. 
    ```sh
    sudo modprobe v4l2loopback exclusive_caps=1 video_nr=1,2
    ```
1. Install Gstreamer main packages
    ```sh
    sudo apt-get install -y \
        libgstreamer1.0-0 gstreamer1.0-tools gstreamer1.0-plugins-base \
        gstreamer1.0-plugins-good gstreamer1.0-libav
    ```
1. Open two new terminals (one for each fake video device), and in each terminal ssh into your Rasperry Pi.
1. In one terminal, stream a test video of a white ball moving around a black background from the first fake video device.
    ```sh
    sudo gst-launch-1.0 -v videotestsrc pattern=ball ! "video/x-raw,width=640,height=480,framerate=10/1" ! avenc_mjpeg ! v4l2sink device=/dev/video1
    ```
    If this generates an error, be sure that there are no existing video streams targeting /dev/video1 (you can query with commands like this: `ps -aux | grep gst-launch-1.0 | grep "/dev/video1"`).
1. In the other terminal, stream a test video of SMPTE 100%% color bars moving horizontally from the second fake video device.
    ```sh
    sudo gst-launch-1.0 -v videotestsrc pattern=smpte horizontal-speed=1 ! "video/x-raw,width=640,height=480,framerate=10/1" ! avenc_mjpeg ! v4l2sink device=/dev/video2
    ```
    If this generates an error, be sure that there are no existing video streams targeting /dev/video1 (you can query with commands like this: `ps -aux | grep gst-launch-1.0 | grep "/dev/video2"`).

## Set up Akri
1. Install Akri Helm chart and enable the udev video configuration which will search for all video devices on the node, as specified by the udev rule `KERNEL=="video[0-9]*"` in the configuration. Since the /dev/video1 and /dev/video2 devices are running on this node, the Akri Agent will discover them and create an Instance for each camera. Watch two broker pods spin up, one for each camera.
    ```sh
    helm repo add akri-helm-charts https://deislabs.github.io/akri/
    helm install akri akri-helm-charts/akri-dev \
        --set useLatestContainers=true \
        --set udev.enabled=true \
        --set udev.name=akri-udev-video \
        --set udev.udevRules[0]='KERNEL=="video[0-9]*"' \
        --set udev.brokerPod.image.repository="ghcr.io/deislabs/akri/udev-video-broker:latest-dev"
    watch kubectl get pods,akric,akrii -o wide
    ```
    Run `kubectl get crd`, and you should see the crds listed.
    Run `kubectl get pods -o wide`, and you should see the Akri pods.
    Run `kubectl get akric`, and you should see `akri-udev-video`. If IP cameras were discovered and pods spun up, the instances can be seen by running `kubectl get akrii` and further inspected by running `kubectl get akrii akri-udev-video-<ID> -o yaml`
    More information about the Akri Helm charts can be found in the [user guide](./user-guide.md#understanding-akri-helm-charts).

1. Inspect the two instances, seeing that the correct devnodes in the metadata and that one of the usage slots for each instance was reseved for this node.
    ```sh 
    kubectl get akrii -o yaml
    ```
1. Deploy the streaming web application and watch a pod spin up for the app.
    ```sh
    # This file url is not available while the Akri repo is private.  To get a valid url, open 
    # https://github.com/deislabs/akri/blob/main/deployment/samples/akri-video-streaming-app.yaml
    # and click the "Raw" button ... this will generate a link with a token that can be used below.
    curl -o akri-video-streaming-app.yaml <RAW LINK WITH TOKEN>
    kubectl apply -f akri-video-streaming-app.yaml
    watch kubectl get pods -o wide
    ```
1. Determine which port the service is running on.
    ```sh
    kubectl get services
    ```
    Something like the following will be displayed. The ids of the camera services (`udev-camera-<id>-svc`) will likely be different as they are determined by hostname.
    ```
    NAME                     TYPE        CLUSTER-IP       EXTERNAL-IP   PORT(S)        AGE
    kubernetes               ClusterIP   10.XXX.XXX.X     <none>        443/TCP        2d5h
    streaming                NodePort    10.XXX.XXX.XX    <none>        80:31143/TCP   41m
    udev-camera-901a7b-svc   ClusterIP   10.XXX.XXX.XX    <none>        80/TCP         42m
    udev-camera-e2548e-svc   ClusterIP   10.XXX.XXX.XX    <none>        80/TCP         42m
    udev-camera-svc          ClusterIP   10.XXX.XXX.XXX   <none>        80/TCP         42m
    ```
1. Navigate in your browser to http://ip-address:31143/ where ip-address is the IP address of your ubuntu VM and the port number is from the output of `kubectl get services`. You should see three videos. The top video streams frames from all udev cameras (from the overarching `udev-camera-svc` service), while each of the bottom videos displays the streams from each of the individual camera services (`udev-camera-901a7b-svc` and `udev-camera-e2548e-svc`). Note: the streaming web application displays at a rate of 1 fps.

## Cleanup 
1. Bring down the streaming service.
    ```sh
    kubectl delete service akri-video-streaming-app
    kubectl delete deployment akri-video-streaming-app
    watch kubectl get pods
    ```
1. Delete the configuration and watch the instances, pods, and services be deleted.
    ```sh
    kubectl delete akric akri-udev-video
    watch kubectl get pods,services,akric,akrii -o wide
    ```
1. Bring down the Akri Agent, Controller, and CRDs.
    ```sh
    helm delete akri
    kubectl delete crd instances.akri.sh
    kubectl delete crd configurations.akri.sh
    ```
1. Stop video streaming on dummy devices and remove kernel module.
    ```sh
    # If terminal has timed out, search for process to kill.
    # ps ax | grep gst-launch-1.0
    # sudo kill <PID>
    sudo modprobe -r v4l2loopback
    ```

## Going beyond the demo
1. Plug in real cameras! You can [pass environment variables](./udev-video-sample.md#modifying-ther-brokerpod-spec) to the frame server broker to specify the format, resolution width/height, and frames per second of your cameras.
1. Apply the [ONVIF configuration](onvif-configuration.md) and make the streaming app display footage from both the local video devices and onvif cameras. To do this, modify the [video streaming yaml](../deployment/samples/akri-video-streaming-app.yaml) as described in the inline comments in order to create a larger service that aggregates the output from both the `udev-camera-svc` service and `onvif-camera-svc` service.
1. Add more nodes to the cluster.
1. [Modify the udev rule](udev-video-sample.md#modifying-the-udev-rule) to find a more specific subset of cameras.
1. Discover other udev devices by creating a new udev configuration and broker. Learn more about the udev protocol [here](udev-configuration.md).
