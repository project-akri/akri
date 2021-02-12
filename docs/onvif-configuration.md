# Using the ONVIF Discovery Protocol in a Configuration
## Background
ONVIF is a standard used by many IP cameras and defines discovery and access for RTSP camera streams. Along with a protocol implementation for ONVIF, Akri has provided a generic ONVIF Configuration. Akri has also provided a sample broker (`akri-onvif-video-broker`), which acts as a frame server.

Using Akri's default ONVIF Configuration to discover and utilize ONVIF cameras looks like the following:

<img src="./media/onvif-flow.svg" alt="Akri ONVIF Flow" style="padding-bottom: 5px; padding-top: 5px;
margin-right: auto; display: block; margin-left: auto;"/>
1. An operator applies the ONVIF Configuration to the cluster (by enabling ONVIF when installing the Akri Helm chart). They also specific a broker image -- `akri-onvif-video-broker` in the figure.
1. The Akri Agent uses the ONVIF protocol to discover the IP cameras and creates Instances for each discovered camera.
1. The Akri Controller sees the Instances and deploys `akri-onvif-video-broker` pods, which were specified in the Configuration. The Controller also creates a Kubernetes service for each ONVIF camera along with one service for all the ONVIF cameras.

## Usage
To use the default ONVIF Configuration in your Akri-enabled cluster, you simply set `onvif.enabled=true` when installing the Akri Helm chart. If you would like broker pods to be deployed automatically to discovered cameras, set `udev.brokerPod.image.repository` to point to your broker image. Alternatively, if it meets your scenario, you could use the Akri frame server broker as done below. If you would rather manually deploy pods to utilize the cameras advertized by Akri, don't specify a broker pod and see our documentation on [requesting resources advertized by Akri](./requesting-akri-resources.md). More information about the Akri Helm charts can be found in the [user guide](./user-guide.md#understanding-akri-helm-charts).

```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri \
    --set onvif.enabled=true \
    --set onvif.brokerPod.image.repository="ghcr.io/deislabs/akri/onvif-video-broker:latest-dev"
```

The default Configuration will find any ONVIF camera and will deploy up to one broker pod to each camera, since `capacity` defaults to one. The brokers will supply the automatically created Instance Services and the Configuration Service with frames.

The ONVIF Configuration can be tailored to your cluster by:

* Filtering ONVIF cameras
* Changing the discovery timeout
* Changing the capacity
* Disabling automatic service creation
* Modifying the broker PodSpec (See [Customizing Akri
  Installation](./customizing-akri-installation.md#modifying-the-brokerpodspec))
* Modifying instanceServiceSpec or configurationServiceSpec (See [Customizing Akri
  Installation](./customizing-akri-installation.md#modifying-instanceservicespec-or-configurationservicespec))

### Filtering ONVIF cameras
To ensure that this Configuration only describes certain cameras, a basic filter capability has been provided.  This
will allow you to either include or exclude specific IP addresses, MAC addresses, or ONVIF scopes.

For example, you can enable cluster access for every camera that does not have an IP address of 10.0.0.1 by using this:
```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri \
    --set onvif.enabled=true \
    --set onvif.brokerPod.image.repository="ghcr.io/deislabs/akri/onvif-video-broker:latest-dev" \
    --set onvif.ipAddresses.action=Exclude \
    --set onvif.ipAddresses.items[0]=10.0.0.1
```

You can enable cluster access for every camera with a specific name, you can modify the Configuration like so:
```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri \
    --set onvif.enabled=true \
    --set onvif.brokerPod.image.repository="ghcr.io/deislabs/akri/onvif-video-broker:latest-dev" \
    --set onvif.scopes.action=Include \
    --set onvif.scopes.items[0]="onvif://www.onvif.org/name/GreatONVIFCamera" \
    --set onvif.scopes.items[1]="onvif://www.onvif.org/name/AwesomeONVIFCamera"
```

### Changing the discovery timeout
The ONVIF protocol will search for up to `discoveryTimeoutSeconds` for IP cameras. This timeout can be increased or
decreased as desired, and defaults to 1 second if left unconfigured. It can be set in the Configuration like this:
```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri \
    --set onvif.enabled=true \
    --set onvif.brokerPod.image.repository="ghcr.io/deislabs/akri/onvif-video-broker:latest-dev" \
    --set onvif.discoveryTimeoutSeconds=2
```

### Changing the capacity
To modify the Configuration so that a camera is accessed by more or fewer protocol broker Pods, update the `capacity`
property to reflect the correct number.  For example, if your high availability needs are met by having only 1 redundant
pod, you can update the Configuration like this:
```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri \
    --set onvif.enabled=true \
    --set onvif.brokerPod.image.repository="ghcr.io/deislabs/akri/onvif-video-broker:latest-dev" \
    --set onvif.capacity=2
```

## Disabling automatic service creation
By default, the generic ONVIF Configuration will create services for all the brokers of a specific Akri Instance and all the brokers of an Akri Configuration. Disable the create of Instance level services and Configuration level services by setting `--set onvif.createInstanceServices=false` and `--set onvif.createConfigurationService=false`, respectively.

## Modifying a Configuration
More information about how to modify an installed Configuration, add additional protocol Configurations to a cluster, or
delete a Configuration can be found in the [Customizing an Akri Installation
document](./customizing-akri-installation.md).

## Implementation details
The ONVIF implementation can be understood by looking at several things:

1. [OnvifDiscoveryHandlerConfig](../shared/src/akri/configuration.rs) defines the required properties
1. [The onvif property in akri-configuration-crd.yaml](../deployment/helm/crds/akri-configuration-crd.yaml) validates
   the CRD input
1. [OnvifDiscoveryHandler](../agent/src/protocols/onvif/discovery_handler.rs) defines ONVIF camera discovery
1. [samples/brokers/onvif-video-broker](../samples/brokers/onvif-video-broker) defines the ONVIF protocol broker