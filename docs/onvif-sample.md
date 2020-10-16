# ONVIF sample
As an example of handling dynamic shared leaf devices, an implementation has been created for the ONVIF camera protocol.
ONVIF is a standard used by many IP cameras and defines discovery and access for RTSP camera streams. Along with a protocol implementation for ONVIF, Akri has provided an ONVIF Configuration and sample broker (`akri-onvif-video-broker`), which acts as a frame server.

Using Akri's default ONVIF Configuration to discover and utilize ONVIF cameras looks like the following:

<img src="./media/onvif-flow.svg" alt="Akri ONVIF Flow" style="padding-bottom: 5px; padding-top: 5px;
margin-right: auto; display: block; margin-left: auto;"/>
1. An operator applies the ONVIF Configuration to the cluster (by enabling ONVIF when installing the Akri Helm chart).
1. The Akri Agent uses the ONVIF protocol to discover the IP cameras and creates Instances for each discovered camera.
1. The Akri Controller sees the Instances and deploys `akri-onvif-video-broker` pods, which were specified in the Configuration. The Controller also creates a Kubernetes service for each ONVIF camera along with one service for all the ONVIF cameras.

## Usage
To use the default ONVIF Configuration in your Akri-enabled cluster, you can simply set `onvifVideo.enabled=true` when installing the Akri helm chart.  

```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri \
    --set useLatestContainers=true \
    --set onvifVideo.enabled=true
```

The default Configuration will find any ONVIF camera and ensure that 5 protocol broker Pods are running at all times,
supplying each Instance Service and the Configuration Service with frames.

The ONVIF Configuration can be tailored to your cluster by:

* Filtering ONVIF cameras
* Changing the discovery timeout
* Changing the capacity
* Modifying the broker PodSpec (See [Modifying a Akri
  Installation](./modifying-akri-installation#modifying-the-brokerpodspec))
* Modifying instanceServiceSpec or configurationServiceSpec (See [Modifying a Akri
  Installation](./modifying-akri-installation#modifying-instanceservicespec-or-configurationservicespec))

### Filtering ONVIF cameras
To ensure that this Configuration only describes certain cameras, a basic filter capability has been provided.  This
will allow you to either include or exclude specific IP addresses, MAC addresses, or ONVIF scopes.

For example, you can enable cluster access for every camera that does not have an IP address of 10.0.0.1 by using this:
```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri \
    --set useLatestContainers=true \
    --set onvifVideo.enabled=true \
    --set onvifVideo.ipAddresses.action=Exclude \
    --set onvifVideo.ipAddresses.items[0]=10.0.0.1
```

You can enable cluster access for every camera with a specific name, you can modify the Configuration like so:
```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri \
    --set useLatestContainers=true \
    --set onvifVideo.enabled=true \
    --set onvifVideo.scopes.action=Include \
    --set onvifVideo.scopes.items[0]="onvif://www.onvif.org/name/GreatONVIFCamera" \
    --set onvifVideo.scopes.items[1]="onvif://www.onvif.org/name/AwesomeONVIFCamera"
```

### Changing the discovery timeout
The ONVIF protocol will search for up to `discoveryTimeoutSeconds` for IP cameras. This timeout can be increased or
decreased as desired, and defaults to 1 second if left unconfigured. It can be set in the Configuration like this:
```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri \
    --set useLatestContainers=true \
    --set onvifVideo.enabled=true \
    --set onvifVideo.discoveryTimeoutSeconds=2
```

### Changing the capacity
To modify the Configuration so that a camera is accessed by more or fewer protocol broker Pods, update the `capacity`
property to reflect the correct number.  For example, if your high availability needs are met by having only 1 redundant
pod, you can update the Configuration like this:
```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri \
    --set useLatestContainers=true \
    --set onvifVideo.enabled=true \
    --set onvifVideo.capacity=2
```

## Modifying a Configuration
More information about how to modify an installed Configuration, add additional protocol Configurations to a cluster, or
delete a Configuration can be found in the [Modifying a Akri Installation
document](./modifying-akri-installation.md).

## Implementation details
The ONVIF implementation can be understood by looking at several things:

1. [OnvifDiscoveryHandlerConfig](../shared/src/akri/configuration.rs) defines the required properties
1. [The onvif property in akri-configuration-crd.yaml](../deployment/helm/crds/akri-configuration-crd.yaml) validates
   the CRD input
1. [OnvifDiscoveryHandler](../agent/src/protocols/onvif/discovery_handler.rs) defines ONVIF camera discovery
1. [samples/brokers/onvif-video-broker](../samples/brokers/onvif-video-broker) defines the ONVIF protocol broker