# Udev camera sample
As an example of handling local capabilities, an implementation was made for video cameras that can be discovered using the udev protocol. Udev is a device manager for the Linux kernel. The udev protocol parses udev rules listed in a Configuration, searches for them using udev, and returns a list of device nodes (ie: /dev/video0). An instance is created for each device node. Since this sample uses a broker that streams frames from a local camera, the rule added to the Configuration is `KERNEL=="video[0-9]*"`. To determine if a node has video devices that will be discovered by this Configuration, run `ls -l /sys/class/video4linux/` or `sudo v4l2-ctl --list-devices`.

## Usage
To use enable a udev camera in your Akri-enabled cluster, you can simply set `udevVideo.enabled=true` when installing the Akri helm chart.  
```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri \
    --set useLatestContainers=true \
    --set udevVideo.enabled=true
```

The default Configuration will find any video4linux camera and ensure that 5 protocol broker Pods are running at all times, supplying each Instance Service and the Configuration Service with frames.

The udev Configuration can be tailored to your cluster by modifying the [Akri helm chart values](../deployment/helm/values.yaml) in the following ways:

* Modifying the udev rule
* Changing the capacity
* Modifying brokerPodSpec
* Modifying instanceServiceSpec or configurationServiceSpec (See [Modifying a Akri Installation](./modifying-akri-installation#modifying-instanceservicespec-or-configurationservicespec))

### Modifying the udev rule
Instead of finding all video4linux device nodes, the udev rule can be modified to exclude certain device nodes, find devices only made by a certain manufacturer, and more. To learn more about what udev rule fields are currently supported see [udev_rule_grammar.pest](../agent/src/protocols/udev/udev_rule_grammar.pest). To learn more about udev rules in general, see the [udev wiki](https://wiki.archlinux.org/index.php/Udev). 

For example, the rule can be narrowed by matching cameras with specific properties. To see the properties of a camera on a node, do `udevadm info --query=property --name /dev/video0`, passing in the proper devnode name. In this example, `ID_VENDOR=Microsoft` was one of the outputted properties. To only find cameras made by Microsoft, the rule can be modified like the following:
```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri \
    --set useLatestContainers=true \
    --set udevVideo.enabled=true \
    --set udevVideo.udevRules[0]='KERNEL=="video[0-9]*"\, ENV{ID_VENDOR}=="Microsoft"'
```

As another example, to make sure that the camera has a capture capability rather than just being a video output device, modify the udev rule as follows: 
```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri \
    --set useLatestContainers=true \
    --set udevVideo.enabled=true \
    --set udevVideo.udevRules[0]='KERNEL=="video[0-9]*"\, ENV{ID_V4L_CAPABILITIES}=="*:capture:*"'
```

### Changing the capacity
To modify the Configuration so that a camera is accessed by more or fewer protocol broker Pods, update the `capacity` property to reflect the correct number.  For example, if your high availability needs are met by having only 1 redundant pod, you can update the Configuration like this:
```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri \
    --set useLatestContainers=true \
    --set udevVideo.enabled=true \
    --set udevVideo.capacity=2
```

### Modifying the brokerPod spec
The `brokerPodSpec` property is a full [PodSpec](https://kubernetes.io/docs/reference/generated/kubernetes-api/v1.18/#podspec-v1-core) and can be modified as such.  For example, to configure the frame rate, resolution, and image type the broker streams from the discovered video cameras, environment variables can be modified in the podspec. To examine what settings are supported by a camera, install `v4l-utils` and run `sudo v4l2-ctl -d /dev/video0 --list-formats-ext` on the node. By default, the environment variables are set to MJPG format, 640x480 resolution, and 10 frames per second. If the broker sees that those settings are not supported by the camera, it will query the v4l device for supported settings and use the first format, resolution, and fps in the lists returned. The environment variables can be changed when installing the Akri helm chart. The following tells the broker to stream JPEG format, 1000x800 resolution, and 30 frames per second.
```bash
  helm install akri akri-helm-charts/akri \
    --set useLatestContainers=true \
    --set udevVideo.enabled=true \
    --set udevVideo.brokerPod.env.format=JPEG \
    --set udevVideo.brokerPod.env.width=1000 \
    --set udevVideo.brokerPod.env.height=800 \
    --set udevVideo.brokerPod.env.fps=30
```

**Note:** that udev broker pods must run as privileged in order for udev to be able to access the video device.

Reference [Modifying a Akri Installation](./modifying-akri-installation#modifying-the-brokerpodspec)) for more examples of how the broker spec can be modified. 

## Modifying a Configuration
More information about how to modify an installed Configuration, add additional protocol Configurations to a cluster, or delete a Configuration can be found in the [Modifying a Akri Installation document](./modifying-akri-installation.md).

## Implementation details
The udev implementation can be understood by looking at several things:

1. [UdevDiscoveryHandlerConfig](../shared/src/akri/configuration.rs) defines the required properties
1. [The udev property in akri-configuration-crd.yaml](../deployment/helm/crds/akri-configuration-crd.yaml) validates the CRD input
1. [UdevDiscoveryHandler](../agent/src/protocols/udev/discovery_handler.rs) defines udev camera discovery
1. [samples/brokers/udev-video-broker](../samples/brokers/udev-video-broker) defines the udev protocol broker
1. [udev_rule_grammar.pest](../agent/src/protocols/udev/udev_rule_grammar.pest) defines the grammar for parsing udev rules and enumerate which fields are supported (such as `ATTR` and `TAG`), which are yet to be supported (`ATTRS` and `TAGS`), and which fields will never be supported, mainly due to be assignment rather than matching fields (such as `ACTION` and `GOTO`).