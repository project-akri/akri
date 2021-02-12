# OPC UA Sample
OPC UA (Object Linking and Embedding for Process Control Unified Architecture) is a communication protocol for
industrial automation. Akri has implemented a discovery handler for discovering OPC UA Servers that live at specified endpoints or are registered with specified Local Discovery Servers. Background on the OPC UA protocol implementation can be found in the [proposal](proposals/opcua.md). To try out using Akri to discover and utilize OPC UA servers, see the [OPC UA end-to-end demo](./opcua-demo.md).

## Usage
Every OPC UA server/application has a DiscoveryEndpoint that Clients can access without establishing a session. The
address for this endpoint is defined by a DiscoveryURL. A Local Discovery Server (LDS) is a unique type of OPC UA server
which maintains a list of OPC UA servers that have registered with it. The generic OPC UA Configuration takes in a list of
DiscoveryURLs, whether for LDSes or a specific servers and an optional list of application names to either include or exclude. By default, if no DiscoveryURLs are set, Agent will attempt to reach out to the Local Discovery Server on its host at the default address [from OPC UA Specification
12](https://reference.opcfoundation.org/v104/Core/docs/Part6/7.6/) of `opc.tcp://localhost:4840/` and get the list of
OPC UA servers registered with it. 

To enable OPC UA discovery via the default LDS DiscoveryURL in your Akri-enabled cluster, you must set
`opcua.enabled=true` when installing the Akri Helm chart.  
```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri \
    --set opcua.enabled=true 
```

If you have a workload that you would like to automatically be deployed to each discovered server, specify the workload image when installing Akri. As an example, the installation below will deploy an
empty nginx pod for each server. Instead, you should point to your image, say `ghcr.io/<USERNAME>/opcua-broker`.
```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri \
    --set opcua.enabled=true \
    --set opcua.brokerPod.image.repository=nginx
```

The generic OPC UA Configuration can be tailored to your cluster by modifying the [Akri Helm chart
values](../deployment/helm/values.yaml) in the following ways:

* Specifying the DiscoveryURLs for OPC UA Local Discovery Servers
* Specifying the DiscoveryURLs for specific OPC UA servers
* Specifying the DiscoveryURLs for both Local Discovery Servers and servers
* Filtering the servers by application name
* Mounting OPC UA credentials to enable security
* Changing the capacity
* Modifying the broker PodSpec (See [Modifying a Akri
  Installation](./modifying-a-akri-installation#modifying-the-brokerpodspec))
* Modifying instanceServiceSpec or configurationServiceSpec (See [Modifying a Akri
  Installation](./modifying-a-akri-installation#modifying-instanceservicespec-or-configurationservicespec))

### Specifying the DiscoveryURLs for OPC UA LocalDiscoveryServers
If no DiscoveryURLs are passed as Helm values, the default DiscoveryURL for LocalDiscoveryServers is used. Instead of
using the default `opc.tcp://localhost:4840/` LDS DiscoveryURL, an operator can specify the addresses of one or more
Local Discovery Servers, like in the following example:
```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri \
    --set opcua.enabled=true \
    --set opcua.discoveryUrls[0]="opc.tcp://10.1.2.3:4840/" \
    --set opcua.discoveryUrls[1]="opc.tcp://10.1.3.4:4840/" 
```

### Specifying the DiscoveryURLs for specific OPC UA Servers
If you know the DiscoveryURLs for the OPC UA Servers you want Akri to discover, manually list them when deploying Akri, like in the following:
```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri \
    --set opcua.enabled=true \
    --set opcua.discoveryUrls[0]="opc.tcp://10.123.456.7:4855/"
```

### Specifying the DiscoveryURLs for both LocalDiscoveryServers and Servers
OPC UA discovery can also receive a list of both OPC UA LDS DiscoveryURLs and specific Server urls, as in the following.

```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri \
    --set opcua.enabled=true \
    --set opcua.discoveryUrls[0]="opc.tcp://10.1.2.3:4840/" \
    --set opcua.discoveryUrls[1]="opc.tcp://10.1.3.4:4840/" \
    --set opcua.discoveryUrls[2]="opc.tcp://10.123.456.7:4855/"
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
helm install akri akri-helm-charts/akri \
    --set opcua.enabled=true \
    --set opcua.applicationNames.action=Exclude \
    --set opcua.applicationNames.items[0]="Duke"
```
Alternatively, to only discover the server named "Go Tar Heels!", do the following:
```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri \
    --set opcua.enabled=true \
    --set opcua.applicationNames.action=Include \
    --set opcua.applicationNames.items[0]="Go Tar Heels!"
```

### Mounting OPC UA credentials to enable security
For your broker pod to utilize a discovered OPC UA server, it will need to contain an OPC UA Client. OPC UA Clients and Servers can establish an insecure connection so long as the OPC UA Servers support a Security Policy of None. However, if you would like your broker's OPC UA Client to establish a secure connection with an OPC UA server, the Client and Server must trust each other's x509 v3 certificates. This can be done in one of the three ways explained
in the [OPC UA proposal](./proposals/opcua.md#giving-proper-credentials-to-the-akri-broker). The simplest method is to
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

Finally, when mounting certificates is enabled with Helm via `--set opcua.mountCertificates='true'`, the
secret named `opcua-broker-credentials` will be mounted into the OPC UA brokers. It is mounted to the volume
`credentials` at the `mountPath` /etc/opcua-certs/client-pki, as shown in the [OPC UA Helm
template](../deployment/helm/templates/opcua.yaml). This is the path where the broker expects to find the
certificates. The following is an example how to enable security:
```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri \
    --set opcua.enabled=true \
    --set opcua.mountCertificates='true'
```
>**Note**: If the Helm template for the OPC UA Configuration is too specific, you can [customize the Configuration yaml](./customizing-akri-installation.md#generating-modifying-and-applying-a-custom-configuration) to suit your needs.

### Changing the capacity
By default in the generic OPC UA Configuration, `capacity` is set to 1, so only a single workload can be scheduled to an OPC UA server. To modify the Configuration so that more or fewer Nodes may deploy brokers to an OPC UA Server, update the
`capacity` property to reflect the correct number. For example, if your high availability needs are met by having only
1 redundant pod, you can update the Configuration like this:
```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri \
    --set opcua.enabled=true \
    --set opcua.capacity=2
```

## Modifying a Configuration
More information about how to modify an installed Configuration, add additional protocol Configurations to a cluster, or
delete a Configuration can be found in the [Customizing an Akri Installation
document](./customizing-akri-installation.md).

## Implementation details
The OPC UA implementation can be understood by looking at several things:
1. [OpcuaDiscoveryHandlerConfig](../shared/src/akri/configuration.rs) defines the required properties.
1. [The OPC UA property in akri-configuration-crd.yaml](../deployment/helm/crds/akri-configuration-crd.yaml) validates
   the CRD input.
1. [OpcuaDiscoveryHandler](../agent/src/protocols/opcua/discovery_handler.rs) defines OPC UA Server discovery.
1. [sample-brokers/opcua-monitoring-broker](../samples/brokers/opcua-monitoring-broker) defines a sample OPC UA protocol broker
   that monitors an OPC UA Variable with a specific NodeID.