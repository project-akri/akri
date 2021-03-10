# Using the Udev Discovery Protocol to Discover USB Cameras
As an example of handling local capabilities, a sample broker and streaming app have been made for utilizing video cameras discovered by Akri's udev protocol. To create an Akri Configuration to discover other devices via udev, see the [udev Configuration documentation](./udev-configuration.md). 

Udev is a device manager for the Linux kernel. The udev discovery handler parses udev rules listed in a Configuration, searches for them using udev, and returns a list of device nodes (ie: /dev/video0). An instance is created for each device node. Since this example uses a [sample broker](../samples/brokers/udev-video-broker) that streams frames from a local camera, the rule added to the Configuration is `KERNEL=="video[0-9]*"`. To determine if a node has video devices that will be discovered by this Configuration, run `ls -l /sys/class/video4linux/` or `sudo v4l2-ctl --list-devices`. 

## Usage
To use create a udev Configuration for video devices for your cluster, you can simply set `udev.enabled=true` and a udev rule of `--set udev.udevRules[0]='KERNEL==\"video[0-9]*\"'` when installing the Akri Helm chart. Optionally, set a name for your generated Configuration by setting `--set udev.name=akri-udev-video` and add a broker image in the case you want a workload automatically deployed to discovered devices. More information about the Akri Helm charts can be found in the [user guide](./user-guide.md#understanding-akri-helm-charts).
```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri \
    --set udev.enabled=true \
    --set udev.name=akri-udev-video \
    --set udev.udevRules[0]='KERNEL=="video[0-9]*"' \
    --set udev.brokerPod.image.repository="ghcr.io/deislabs/akri/udev-video-broker"
```

Akri will find all video4linux cameras and ensure that broker Pods are running on nodes that can access the cameras at all times, supplying each Instance Service and the Configuration Service with frames.

The udev Configuration can be tailored to your cluster by modifying the [Akri helm chart values](../deployment/helm/values.yaml) in the following ways:

* Modifying the udev rule
* Modifying brokerPodSpec
* Modifying instanceServiceSpec or configurationServiceSpec (See [Customizing an Akri Installation](./customizing-akri-installation.md#modifying-instanceservicespec-or-configurationservicespec))

### Modifying the udev rule
Instead of finding all video4linux device nodes, the udev rule can be modified to exclude certain device nodes, find devices only made by a certain manufacturer, and more. To learn more about what udev rule fields are currently supported see [udev_rule_grammar.pest](../agent/src/protocols/udev/udev_rule_grammar.pest). To learn more about udev rules in general, see the [udev wiki](https://wiki.archlinux.org/index.php/Udev). 

For example, the rule can be narrowed by matching cameras with specific properties. To see the properties of a camera on a node, do `udevadm info --query=property --name /dev/video0`, passing in the proper devnode name. In this example, `ID_VENDOR=Microsoft` was one of the outputted properties. To only find cameras made by Microsoft, the rule can be modified like the following:
```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri \
    --set udev.enabled=true \
    --set udev.udevRules[0]='KERNEL=="video[0-9]*"\, ENV{ID_VENDOR}=="Microsoft"' \
    --set udev.brokerPod.image.repository="ghcr.io/deislabs/akri/udev-video-broker"
```

As another example, to make sure that the camera has a capture capability rather than just being a video output device, modify the udev rule as follows: 
```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri \
    --set udev.enabled=true \
    --set udev.udevRules[0]='KERNEL=="video[0-9]*"\, ENV{ID_V4L_CAPABILITIES}=="*:capture:*"' \
    --set udev.brokerPod.image.repository="ghcr.io/deislabs/akri/udev-video-broker"
```

### Modifying the brokerPod spec
The `brokerPodSpec` property is a full [PodSpec](https://kubernetes.io/docs/reference/generated/kubernetes-api/v1.18/#podspec-v1-core) and can be modified as such.  For example, to configure the frame rate, resolution, and image type the broker streams from the discovered video cameras, environment variables can be modified in the podspec. To examine what settings are supported by a camera, install `v4l-utils` and run `sudo v4l2-ctl -d /dev/video0 --list-formats-ext` on the node. By default, the environment variables are set to MJPG format, 640x480 resolution, and 10 frames per second. If the broker sees that those settings are not supported by the camera, it will query the v4l device for supported settings and use the first format, resolution, and fps in the lists returned. The environment variables can be changed when installing the Akri Helm chart. For example, tell the broker to stream JPEG format, 1000x800 resolution, and 30 frames per second by setting those environment variables when installing Akri.
```bash
  helm install akri akri-helm-charts/akri \
    --set udev.enabled=true \
    --set udev.udevRules[0]='KERNEL=="video[0-9]*"' \
    --set udev.brokerPod.image.repository="ghcr.io/deislabs/akri/udev-video-broker" \
    --set udev.brokerPod.env.FORMAT='JPEG' \
    --set udev.brokerPod.env.RESOLUTION_WIDTH='1000' \
    --set udev.brokerPod.env.RESOLUTION_HEIGHT='800' \
    --set udev.brokerPod.env.FRAMES_PER_SECOND='30'
```

**Note:** The udev video broker pods run privileged in order to access the video devices. More explicit device access
   could have been configured by setting the appropriate [security
   context](udev-configuration.md#setting-the-broker-pod-security-context) in the broker PodSpec in the Configuration.

Reference [Customizing an Akri Installation](./customizing-akri-installation.md#modifying-the-brokerpodspec) for more examples of how the broker spec can be modified. 

## Modifying a Configuration
More information about how to modify an installed Configuration, add additional protocol Configurations to a cluster, or delete a Configuration can be found in the [Customizing an Akri Installation document](./customizing-akri-installation.md).