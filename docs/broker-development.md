# Creating a Broker to Utilize Discovered Devices
Akri's Agent discovers devices described by an Akri Configuration, and for each discovered device, it creates Kubernetes
resources using the Device Plugin Framework, which can later be requested by Pods. Akri's Controller can automate the
usage of discovered devices by deploying Pods that request the newly created resources. **Akri calls these Pods brokers.**

> Background: Akri chose the term "broker" because one use case Akri initially envisioned was deploying Pods that acted
> as protocol translation gateways. For example, Akri could discover USB cameras and automatically deploy a broker to
> each camera that advertizes the camera as an IP camera that could be accessed outside the Node. 

Akri takes a micro-service approach to deploying brokers. A broker is deployed to each Node that can see a discovered
device (limited by a `capacity` that can be set in a Configuration to limit the number of Nodes that can utilize a
device at once). Each broker is provisioned with device connection information and other metadata as environment
variables. These environment variables come from two sources: a Configuration's `brokerProperties` and the `properties`
of a `Device` discovered by a Discovery Handler. The former is where an operator can specify environment variables that
will be set in brokers that utilize any device discovered via the Configuration. The latter is specific to one device
and usually contains connection information such as an RTSP URL for an ONVIF camera or a devnode for a USB device. Also,
while `brokerProperties` can be unique to a scenario, the `properties` environment variable keys are consistent to a
Discovery Handler with values changing based on device. All the environment variables from these two sources are
displayed in an Instance that represents a discovered device, making it a good reference for what environment variables
the broker should expect. The image below expresses how a broker Pod's environment variables come from the two
aforementioned sources.

![Diagram depicting source of broker Pod environment variables](./media/setting-broker-environment-variables.svg "Source
of broker Pod environment variables")

## Discovery Handler specified environment variables
The first step to developing a broker is understanding what information will be made available to the Pod via the
Discovery Handler (aka the `Device.properties`). The following table contains the environment variables specified by
each of Akri's currently supported Discovery Handlers, and the expected content of the environment variables.

| Discovery Handler | Env Var Name | Value Type | Examples | Always Present? (Y/N) |
|---|---|---|---|---|
| debugEcho (for testing) | `DEBUG_ECHO_DESCRIPTION` | some random string | `foo`, `bar` | Y |
| ONVIF | `ONVIF_DEVICE_SERVICE_URL` | ONVIF camera source URL | `http://10.123.456.789:1000/onvif/device_service` | Y |
| ONVIF | `ONVIF_DEVICE_IP_ADDRESS` | IP address of the camera | `10.123.456.789` | Y |
| ONVIF | `ONVIF_DEVICE_MAC_ADDRESS` | MAC address of the camera | `48:0f:cf:4e:1b:3d`, `480fcf4e1b3d`| Y |
| OPC UA | `OPCUA_DISCOVERY_URL` | [DiscoveryURL](https://reference.opcfoundation.org/GDS/docs/4.3.3/) of specific OPC UA Server/Application  | `10.123.456.789:1000/Some/Path/` | Y |
| udev | `UDEV_DEVNODE` | device node for specific device | `/dev/video1`, `/dev/snd/pcmC1D0p`, `/dev/dri/card0` | Y |

A broker should look up the variables set by the appropriate Discovery Handler and use the contents to connect to a
specific device. 

## Exposing device information over a service
Oftentimes, it is useful for a broker to expose some information from its device over a service. Akri, by default,
assumes this behavior, creating a Kubernetes service for each broker (called an Instance level service) and for all
brokers of a Configuration (called a Configuration level service). This allows an application to target a specific
device/broker or all devices/brokers, the latter of which allows the application to be oblivious to the coming and going
of devices (and thereby brokers). 

> Note: This default creation of Instance and Configuration services can be disabled by setting `<Discovery Handler
> name>.configuration.createInstanceServices=false` and `<Discovery Handler
> name>.configuration.createConfigurationService=false` when installing Akri's Helm chart.

A broker can expose information via REST, gRPC, etc. Akri's [sample brokers](../samples/brokers) all use gRPC. For
example, the udev video and ONVIF brokers both use the same [camera proto
file](../samples/brokers/udev-video-broker/proto/camera.proto) for their gRPC interfaces, which contains a service that
serves camera frames. This means that one end application can be deployed that implements the client side of the
interface and grabs frames from all cameras, whether IP or USB based. This is exactly what our [sample streaming
application](../samples/apps/video-streaming-app) does.

## Deploying your custom broker
Once you have created a broker, you can ask Akri to automatically deploy it to all all devices discovered by a
Configuration by specifying the image in `<Discovery Handler name>.configuration.brokerPod.image.repository` and
`<Discovery Handler name>.configuration.brokerPod.image.tag`. For example, say you created a broker that connects to a
USB camera and advertises it as an IP camera. You want to deploy it to all USB cameras on your cluster's nodes using
Akri, so you deploy Akri with a Configuration that uses the udev Discovery Handler and set the image of your broker (say
`ghcr.io/brokers/camera-broker:v0.0.1`), like so:
```sh
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri-dev \
    --set udev.discovery.enabled=true \
    --set udev.configuration.enabled=true \
    --set udev.configuration.name=akri-udev-video \
    --set udev.configuration.discoveryDetails.udevRules[0]='KERNEL=="video[0-9]*"' \
    --set udev.configuration.brokerPod.image.repository="ghcr.io/brokers/camera-broker" \
    --set udev.configuration.brokerPod.image.tag="v0.0.1" 
```
### Setting compute resource requests and limits for your broker
The default broker Pod memory and CPU resource request and limits in Akri's Helm chart are based off the requirements of Akri's sample brokers. The following brokers were created for demo purposes:
| Discovery Handler | Akri Sample Broker Pod image | Description |
|---|---|---|
| debugEcho | `nginx:stable-alpine` | standard nginx image for testing |
| ONVIF | `ghcr.io/deislabs/akri/onvif-video-broker:latest` | .NET camera frame server |
| OPC UA | `ghcr.io/deislabs/akri/opcua-monitoring-broker:latest` | .Net App subscribes to specific NodeID and serves latest value |
| udev | `ghcr.io/deislabs/akri/udev-video-broker:latest` | Rust camera frame server |

The limit and request bounds were obtained using Kubernetes' [Vertical Pod Autoscaler (VPA)](https://github.com/kubernetes/autoscaler/tree/master/vertical-pod-autoscaler). You should choose bounds appropriate to your broker Pod. [This blog](https://pretired.dazwilkin.com/posts/210305/#vertical-pod-autoscaler-vpa) is a good starting point for learning how to use the VPA to choose bounds.

## Specifying additional broker environment variables in a Configuration
You can request that additional environment variables are set in Pods that request devices discovered via an Akri
Configuration. These are set as key/value pairs in a Configuration's `brokerProperties`. For example, take the scenario
of brokers being deployed to USB cameras discovered by Akri. You may wish to give the brokers extra information about the
image format and resolution the cameras support. The brokers then can look up these variables to know how to properly
utilize their camera. These `brokerProperties` could be set in a Configuration during a Helm installation as follows:
```sh
  helm repo add akri-helm-charts https://deislabs.github.io/akri/
  helm install akri akri-helm-charts/akri-dev \
  --set udev.discovery.enabled=true \
  --set udev.configuration.enabled=true \
  --set udev.configuration.name=akri-udev-video \
  --set udev.configuration.discoveryDetails.udevRules[0]='KERNEL=="video[0-9]*"' \
  --set udev.configuration.brokerPod.image.repository="ghcr.io/deislabs/akri/udev-video-broker" \
  --set udev.configuration.brokerProperties.FORMAT='JPEG' \
  --set udev.configuration.brokerProperties.RESOLUTION_WIDTH='1000' \
  --set udev.configuration.brokerProperties.RESOLUTION_HEIGHT='800'
```