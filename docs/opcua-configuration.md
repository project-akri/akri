# Configuring Akri to Discover Devices via OPC UA
## Background
OPC UA (Object Linking and Embedding for Process Control Unified Architecture) is a communication protocol for
industrial automation. Akri has implemented a Discovery Handler for discovering OPC UA Servers that live at specified endpoints or are registered with specified Local Discovery Servers. Background on the OPC UA Discovery Handler implementation can be found in the [proposal](proposals/opcua.md). To try out using Akri to discover and utilize OPC UA servers, see the [OPC UA end-to-end demo](./opcua-demo.md).

All of Akri's components can be deployed by specifying values in its Helm chart during an installation. This section will cover the values that should be set to (1) deploy the OPC UA Discovery Handlers and (2) apply a Configuration that tells Akri to discover devices using that Discovery Handler.
## Deploying the OPC UA Discovery Handler
In order for the Agent to know how to discover OPC UA servers an OPC UA Discovery Handler must exist. Akri supports an Agent image that includes all supported Discovery Handlers. This Agent will be used if `agent.full=true`. By default, a slim Agent without any embedded Discovery Handlers is deployed and the required Discovery Handlers can be deployed as DaemonSets. This documentation will use that strategy, deploying OPC UA Discovery Handlers by specifying `opcua.discovery.enabled=true` when installing Akri.

## OPC UA Configuration Settings
Instead of having to assemble your own OPC UA Configuration yaml, we have provided a [Helm
template](../deployment/helm/templates/opcua-configuration.yaml). Helm allows us to parametrize the commonly modified fields in our configuration files, and we have provided many for OPC UA (to see
them, run `helm inspect values akri-helm-charts/akri`). More information about the Akri Helm charts can be found in the [user guide](./user-guide.md#understanding-akri-helm-charts).
To apply the OPC UA Configuration to your cluster, simply set `opcua.configuration.enabled=true` along with any of the following additional Configuration settings when installing Akri.
### Discovery Handler Discovery Details Settings
Discovery Handlers are passed discovery details that are set in a Configuration to determine what to discover, filter out of discovery, and so on.
The OPC UA Discovery Handler, requires a set of DiscoveryURLs to direct its search.
Every OPC UA server/application has a DiscoveryEndpoint that Clients can access without establishing a session. The
address for this endpoint is defined by a DiscoveryURL. A Local Discovery Server (LDS) is a unique type of OPC UA server
which maintains a list of OPC UA servers that have registered with it. 

The generic OPC UA Configuration takes in a list of DiscoveryURLs, whether for LDSes or a specific servers and an optional list of application names to either include or exclude. 
By default, if no DiscoveryURLs are set, the Discovery Handler will attempt to reach out to the Local Discovery Server on its host at the default address [from OPC UA Specification
12](https://reference.opcfoundation.org/v104/Core/docs/Part6/7.6/) of `opc.tcp://localhost:4840/` and get the list of
OPC UA servers registered with it. 
| Helm Key | Value | Default | Description |
|---|---|---|---|
| opcua.configuration.discoveryDetails.discoveryUrls | array of DiscoveryURLs | ["opc.tcp://localhost:4840/"] | DiscoveryURLs for OPC UA Servers or Local Discovery Servers | 
| opcua.configuration.discoveryDetails.applicationNames.action | Include, Exclude | Exclude | filter action to take on a set of OPC UA Applications |
| opcua.configuration.discoveryDetails.applicationNames.items | array of application names | empty | application names that the filter action acts upon |

### Broker Pod Settings
If you would like workloads ("broker" Pods) to be deployed automatically to discovered devices, a broker image should be specified in the Configuration. Alternatively, if it meets your scenario, you could use the Akri frame server broker ("ghcr.io/deislabs/akri/opcua-video-broker"). If you would rather manually deploy pods to utilize the devices advertized by Akri, don't specify a broker pod and see our documentation on [requesting resources advertized by Akri](./requesting-akri-resources.md). 
| Helm Key | Value | Default | Description |
|---|---|---|---|
| opcua.configuration.brokerPod.image.repository | image string | "" | image of broker Pod that should be deployed to discovered devices |
| opcua.configuration.brokerPod.image.tag | tag string | "latest" | image tag of broker Pod that should be deployed to discovered devices |

### Mounting Credentials Settings
See [Mounting OPC UA credentials to enable security](#mounting-opc-ua-credentials-to-enable-security) for more details on how to use this setting.
| Helm Key | Value | Default | Description |
|---|---|---|---|
| opcua.configuration.mountCertificates| true, false | false | specify whether to mount a secret named `opcua-broker-credentials` into the OPC UA brokers |

### Disabling Automatic Service Creation
By default, if a broker Pod is specified, the generic OPC UA Configuration will create services for all the brokers of a specific Akri Instance and all the brokers of an Akri Configuration. The creation of these services can be disabled.
| Helm Key | Value | Default | Description |
|---|---|---|---|
| opcua.configuration.createInstanceServices | true, false | true | a service should be automatically created for each broker Pod |
| opcua.configuration.createConfigurationService | true, false | true | a single service should be created for all brokers of a Configuration |

### Capacity Setting
By default, if a broker Pod is specified, a single broker Pod is deployed to each device. To modify the Configuration so that an OPC UA server is accessed by more or fewer nodes via broker Pods, update the `opcua.configuration.capacity` setting to reflect the correct number. For example, if your high availability needs are met by having 1 redundant
pod, you can update the Configuration like this by setting `opcua.configuration.capacity=2`.
| Helm Key | Value | Default | Description |
|---|---|---|---|
| opcua.configuration.capacity | number | 1 | maximum number of brokers that can be deployed to utilize a device (up to 1 per Node) |

### Installing Akri with the OPC UA Configuration and Discovery Handler
Leveraging the above settings, Akri can be installed with the OPC UA Discovery Handler and an OPC UA Configuration that specifies discovery via the default LDS DiscoveryURL:
```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri-dev \
    --set opcua.discovery.enabled=true \
    --set opcua.configuration.enabled=true 
```

If you have a workload that you would like to automatically be deployed to each discovered server, specify the workload image when installing Akri. As an example, the installation below will deploy an
empty nginx pod for each server. Instead, you should point to your image, say `ghcr.io/<USERNAME>/opcua-broker`.
```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri-dev \
    --set opcua.discovery.enabled=true \
    --set opcua.configuration.enabled=true \
    --set opcua.configuration.brokerPod.image.repository=nginx
```
> Note: set `opcua.configuration.brokerPod.image.tag` to specify an image tag (defaults to `latest`).

The following installation examples have been given to show how to the OPC UA Configuration can be tailored to you cluster:

* Specifying the DiscoveryURLs for OPC UA Local Discovery Servers
* Specifying the DiscoveryURLs for specific OPC UA servers
* Specifying the DiscoveryURLs for both Local Discovery Servers and servers
* Filtering the servers by application name
* Mounting OPC UA credentials to enable security

### Specifying the DiscoveryURLs for OPC UA LocalDiscoveryServers
If no DiscoveryURLs are passed as Helm values, the default DiscoveryURL for LocalDiscoveryServers is used. Instead of
using the default `opc.tcp://localhost:4840/` LDS DiscoveryURL, an operator can specify the addresses of one or more
Local Discovery Servers, like in the following example:
```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri-dev \
    --set opcua.discovery.enabled=true \
    --set opcua.configuration.enabled=true \
    --set opcua.configuration.discoveryDetails.discoveryUrls[0]="opc.tcp://10.1.2.3:4840/" \
    --set opcua.configuration.discoveryDetails.discoveryUrls[1]="opc.tcp://10.1.3.4:4840/" 
```

### Specifying the DiscoveryURLs for specific OPC UA Servers
If you know the DiscoveryURLs for the OPC UA Servers you want Akri to discover, manually list them when deploying Akri, like in the following:
```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri-dev \
    --set opcua.discovery.enabled=true \
    --set opcua.configuration.enabled=true \
    --set opcua.configuration.discoveryDetails.discoveryUrls[0]="opc.tcp://10.123.456.7:4855/"
```

### Specifying the DiscoveryURLs for both LocalDiscoveryServers and Servers
OPC UA discovery can also receive a list of both OPC UA LDS DiscoveryURLs and specific Server urls, as in the following.

```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri-dev \
    --set opcua.discovery.enabled=true \
    --set opcua.configuration.enabled=true \
    --set opcua.configuration.discoveryDetails.discoveryUrls[0]="opc.tcp://10.1.2.3:4840/" \
    --set opcua.configuration.discoveryDetails.discoveryUrls[1]="opc.tcp://10.1.3.4:4840/" \
    --set opcua.configuration.discoveryDetails.discoveryUrls[2]="opc.tcp://10.123.456.7:4855/"
```

>**Note**: The Agent's OPC UA discovery method only supports tcp DiscoveryURLs, since the [Rust OPC UA
library](https://github.com/locka99/opcua) has yet to support http(s).

### Filtering the Servers by application name
Instead of discovering all servers registered with specified Local Discovery Servers, you can choose
to include or exclude a list of application names (the `applicationName` property of a server's `ApplicationDescription`
as specified by [OPC UA Specification](https://reference.opcfoundation.org/v104/Core/DataTypes/ApplicationDescription/)). For example, to discover all servers registered with the default LDS except for
the server named "Duke", do the following.
```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri-dev \
    --set opcua.discovery.enabled=true \
    --set opcua.configuration.enabled=true \
    --set opcua.configuration.discoveryDetails.applicationNames.action=Exclude \
    --set opcua.configuration.discoveryDetails.applicationNames.items[0]="Duke"
```
Alternatively, to only discover the server named "Go Tar Heels!", do the following:
```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri-dev \
    --set opcua.discovery.enabled=true \
    --set opcua.configuration.enabled=true \
    --set opcua.configuration.discoveryDetails.applicationNames.action=Include \
    --set opcua.configuration.discoveryDetails.applicationNames.items[0]="Go Tar Heels!"
```

### Mounting OPC UA credentials to enable security
For your broker pod to utilize a discovered OPC UA server, it will need to contain an OPC UA Client. OPC UA Clients and Servers can establish an insecure connection so long as the OPC UA Servers support a Security Policy of None. However, if you would like your broker's OPC UA Client to establish a secure connection with an OPC UA server, the Client and Server must trust each other's x509 v3 certificates. This can be done in one of the three ways explained
in the [OPC UA proposal](./proposals/opcua.configuration.md#giving-proper-credentials-to-the-akri-broker). The simplest method is to
sign the OPC UA broker's certificate with the same Certificate Authority (CA) as the Server with which it
wishes to connect. The certificates are passed to the broker via a Kubernetes Secret mounted as a volume to the directory `/etc/opcua-certs/client-pki`.

It is the operator's responsibility to generate the certificates and securely create a Kubernetes Secret named
`opcua-broker-credentials`, ideally using a KMS. More information about using Kubernetes Secrets securely can be found
in the [credentials passing proposal](proposals/credentials-passing.md). The following is an example kubectl command to
create the Kubernetes Secret, projecting each certificate/crl/private key with the expected key name (ie
`client_certificate`, `client_key`, `ca_certificate`, and `ca_crl`).
``` bash
kubectl create secret generic opcua-broker-credentials \
--from-file=client_certificate=/path/to/AkriBroker.der \
--from-file=client_key=/path/to/AkriBroker.pfx \
--from-file=ca_certificate=/path/to/SomeCA.der \
--from-file=ca_crl=/path/to/SomeCA.crl
```
Certificates can be created and signed with a CA manually using openssl, by using the OPC Foundation [certificate
generator tool](https://github.com/OPCFoundation/Misc-Tools), or Akri's [certificate generator](../samples/opcua-certificate-generator/README.md). Be sure that the certificates are in the format expected by your OPC UA Client.

Finally, when mounting certificates is enabled with Helm via `--set opcua.configuration.mountCertificates='true'`, the
secret named `opcua-broker-credentials` will be mounted into the OPC UA brokers. It is mounted to the volume
`credentials` at the `mountPath` /etc/opcua-certs/client-pki, as shown in the [OPC UA Helm
template](../deployment/helm/templates/opcua.configuration.yaml). This is the path where the broker expects to find the
certificates. The following is an example how to enable security:
```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri-dev \
    --set opcua.discovery.enabled=true \
    --set opcua.configuration.enabled=true \
    --set opcua.configuration.mountCertificates='true'
```
>**Note**: If the Helm template for the OPC UA Configuration is too specific, you can [customize the Configuration yaml](./customizing-akri-installation.md#generating-modifying-and-applying-a-custom-configuration) to suit your needs.

## Modifying a Configuration
Akri has provided further documentation on [modifying the broker PodSpec](./customizing-akri-installation.md#modifying-the-brokerpodspec), [instanceServiceSpec, or configurationServiceSpec](./customizing-akri-installation.md#modifying-instanceservicespec-or-configurationservicespec)
More information about how to modify an installed Configuration, add additional Configurations to a cluster, or
delete a Configuration can be found in the [Customizing an Akri Installation
document](./customizing-akri-installation.md).

## Implementation details
The OPC UA implementation can be understood by looking at several things:
1. [OpcuaDiscoveryDetails](../discovery-handlers/opcua/src/discovery_handler.rs) defines the required properties.
1. [OpcuaDiscoveryHandler](../discovery-handlers/opcua/src/discovery_handler.rs) defines OPC UA Server discovery.
1. [sample-brokers/opcua-monitoring-broker](../samples/brokers/opcua-monitoring-broker) defines a sample OPC UA protocol broker
   that monitors an OPC UA Variable with a specific NodeID.