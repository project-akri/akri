# Configuring Akri to Discover Devices via ONVIF

The Constrained Application Protocol (CoAP) is a specialized web transfer protocol for use with constrained nodes and constrained (e.g., low-power, lossy) networks. More information about the protocol is available in [RFC7252](https://datatracker.ietf.org/doc/html/rfc7252). 

To try out CoAP, see the [CoAP end-to-end demo](coap-demo.md).

All of Akri's components can be deployed by specifying values in its Helm chart during an installation. This section will cover the values that should be set to 

1. Deploy the CoAP Discovery Handlers
2. Apply a Configuration that tells Akri to discover CoAP devices using that Discovery Handler.

## Deploying the CoAP Discovery Handler

In order for the Agent to know how to discover CoAP servers an CoAP Discovery Handler must exist. Akri supports an Agent image that includes all supported Discovery Handlers. This Agent will be used if `agent.full=true`.

By default, a slim Agent without any embedded Discovery Handlers is deployed and the required Discovery Handlers can be deployed as DaemonSets. This documentation will use the latter strategy, deploying distinct CoAP Discovery Handlers by specifying `coap.discovery.enabled=true` when installing Akri.

## CoAP Configuration Settings

Instead of having to assemble your own CoAP Configuration yaml, we have provided a [Helm
template](../deployment/helm/templates/coap-configuration.yaml). Helm allows us to parametrize the commonly modified fields in our configuration files, and we have provided many for CoAP (to see
them, run `helm inspect values akri-helm-charts/akri`). More information about the Akri Helm charts can be found in the [user guide](./user-guide.md#understanding-akri-helm-charts).

To apply the CoAP Configuration to your cluster, simply set `coap.configuration.enabled=true` along with any of the following additional Configuration settings when installing Akri.

### Discovery Handler Discovery Details Settings

Discovery Handlers are passed discovery details that are set in a Configuration to determine what to discover, filter out of discovery, and so on.

By default, the CoAP Discovery Handler doesn't require any additional information as it implements the multicast CoAP discovery (described in [Section 8 of RFC 7252](https://datatracker.ietf.org/doc/html/rfc7252#section-8)). The IPv4 address reserved for "All CoAP Nodes" is `224.0.1.187`. Devices must support the default port `5683` as specified by the standard. At the time of writing, IPv6 multicast is not supported in the CoAP Discovery Handler.

Additional settings can be configured:

| Helm Key | Type | Default | Description |
|---|---|---|---|
| coap.configuration.discoveryDetails.multicast | boolean | true | Enable IPv4 multicast discovery | 
| coap.configuration.discoveryDetails.multicastIpAddress | string | 224.0.1.187 | The IPv4 to which the Discovery Handler sends the packets | 
| coap.configuration.discoveryDetails.staticIpAddresses | Array of strings | [] | Additional static IP addresses to look for during the discovery. | 
| coap.configuration.discoveryDetails.queryFilter | { name: string, value: string } | {} | Single name-value pair to filter the discovered resource, as described in [Section 4.1 of RFC 6690](https://datatracker.ietf.org/doc/html/rfc6690#section-4.1) | 

### Broker Pod Settings

If you would like workloads ("broker" Pods) to be deployed automatically to discovered devices, a broker image should be specified in the Configuration. Alternatively, if it meets your scenario, you could use the Akri's default CoAP broker ("ghcr.io/deislabs/akri/coap-broker").

The default CoAP broker supports the following features:

- Expose CoAP resources as REST resources via HTTP. **Only GET requests are currently supported**
- Cache CoAP responses for successful GET requests. If the device becomes unavailable, the cached resource is returned. 

If you would rather manually deploy pods to utilize the devices advertized by Akri, don't specify a broker pod and see our documentation on [requesting resources advertized by Akri](./requesting-akri-resources.md). 

| Helm Key | Value | Default | Description |
|---|---|---|---|
| coap.configuration.brokerPod.image.repository | image string | "" | image of broker Pod that should be deployed to discovered devices |
| coap.configuration.brokerPod.image.tag | tag string | "latest" | image tag of broker Pod that should be deployed to discovered devices |

### Other settings

The CoAP Discovery Handlers supports the same "Capacity" and "Automatic Service Creation" as OPC UA. Refer to the latter [documentation](https://github.com/deislabs/akri/blob/main/docs/opcua-configuration.md#disabling-automatic-service-creation) for additional information.

### Installing Akri with the CoAP Configuration and Discovery Handler

Leveraging the above settings, Akri can be installed with the CoAP Discovery Handler and an CoAP Configuration that specifies discovery via multicast:

```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri \
    --set coap.discovery.enabled=true \
    --set coap.configuration.enabled=true 
```

### Specifying additional IP addresses

An operator can specify the addresses of one or more additional IP addresses to include in the discovery, like in the following example:

```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri \
    --set coap.discovery.enabled=true \
    --set coap.configuration.enabled=true \
    --set coap.configuration.discoveryDetails.staticIpAddresses[0]="192.168.1.126" \
    --set coap.configuration.discoveryDetails.staticIpAddresses[1]="192.168.1.69" 
```
