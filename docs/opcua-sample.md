# OPC UA Sample
OPC UA (Object Linking and Embedding for Process Control Unified Architecture) is a communication protocol for
industrial automation. Oftentimes, these Servers output data that needs to be monitored. Akri has provided a sample
broker that includes an OPC UA client that subscribes to an OPC UA Variable with a given NodeID and serves the value of
that variable over a gRPC server for an end application to consume. A sample web application has been provided as an end
consumer of the OPC UA Variable values. It does ML outlier detection on the values and displays a live log of the
values, showing the anomalies in red text. Background on the OPC UA protocol implementation can be found in the
[proposal](proposals/opcua.md).

## Usage
Every OPC UA server/application has a DiscoveryEndpoint that Clients can access without establishing a session. The
address for this endpoint is defined by a DiscoveryURL. A Local Discovery Server (LDS) is a unique type of OPC UA server
which maintains a list of OPC UA servers that have registered with it. The OPC UA Monitoring Configuration takes in a list of
DiscoveryURLs, whether for LDSes or a specific servers, the NodeID of the OPC UA Variable to monitor, and optionally a
list of application names to either include or exclude. By default, if no DiscoveryURLs are set, Agent will attempt to
reach out to the Local Discovery Server on it's host at the default address [from OPC UA Specification
12](https://reference.opcfoundation.org/v104/Core/docs/Part6/7.6/) of `opc.tcp://localhost:4840/` and get the list of
OPC UA servers registered with it. 

To enable OPC UA discovery via the default LDS DiscoveryURL in your Akri-enabled cluster, you must set
`opcuaMonitoring.enabled=true` when installing the Akri Helm chart.  
```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri \
    --set imagePullSecrets[0].name="regcred" \
    --set useLatestContainers=true \
    --set opcuaMonitoring.enabled=true \
    --set opcuaMonitoring.brokerPod.env.identifier='SomeVariable' \
    --set opcuaMonitoring.brokerPod.env.namespaceIndex='2'
```
The Helm command above specifies that the brokers monitor an OPC UA Variable with a NodeID where `namespaceIndex` is 2 and
`identifier` is "SomeVariable".

The OPC UA Monitoring Configuration can be tailored to your cluster by modifying the [Akri Helm chart
values](../deployment/helm/values.yaml) in the following ways:

* Modifying the `identifier` and `namespaceIndex` of the OPC UA Variable to monitor
* Specifying the DiscoveryURLs for OPC UA LocalDiscoveryServers
* Specifying the DiscoveryURLs for specific OPC UA Servers
* Specifying the DiscoveryURLs for both LocalDiscoveryServers and Servers
* Filtering the Servers by application name
* Mounting OPC UA credentials to enable security
* Changing the capacity
* Modifying the broker PodSpec (See [Modifying a Akri
  Installation](./modifying-a-akri-installation#modifying-the-brokerpodspec))
* Modifying instanceServiceSpec or configurationServiceSpec (See [Modifying a Akri
  Installation](./modifying-a-akri-installation#modifying-instanceservicespec-or-configurationservicespec))

### Modifying the `identifier` and `namespaceIndex` of the OPC UA Variable to monitor
The OPC UA Monitoring Configuration takes in the NodeID for the OPC UA Variable it will monitor. The broker will subscribe to that
Variable and send its value over gRPC for the end application to consume. An [OPC UA
NodeID](https://reference.opcfoundation.org/v104/Core/docs/Part3/8.2.1/) consists of a `namespaceIndex` and `identifier`
of a specific type. The OPC UA monitoring broker currently only supports identifiers with type string. If no
`namespaceIndex` is passed, the default Helm value is "2". The following sets a NodeID with an `identifier` of
"AnotherVariable" and `namespaceIndex` of "1".

```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri \
    --set imagePullSecrets[0].name="regcred" \
    --set useLatestContainers=true \
    --set opcuaMonitoring.enabled=true \
    --set opcuaMonitoring.brokerPod.env.identifier='AnotherVariable' \
    --set opcuaMonitoring.brokerPod.env.namespaceIndex='1'
```

### Specifying the DiscoveryURLs for OPC UA LocalDiscoveryServers
If no DiscoveryURLs are passed as Helm values, the default DiscoveryURL for LocalDiscoveryServers is used. Instead of
using the default `opc.tcp://localhost:4840/` LDS DiscoveryURL, an operator can specify the addresses of one or more
Local Discovery Servers, like in the following example:
```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri \
    --set imagePullSecrets[0].name="regcred" \
    --set useLatestContainers=true \
    --set opcuaMonitoring.enabled=true \
    --set opcuaMonitoring.brokerPod.env.identifier='SomeVariable' \
    --set opcuaMonitoring.brokerPod.env.namespaceIndex='2' \
    --set opcuaMonitoring.discoveryUrls[0]="opc.tcp://10.1.2.3:4840/" \
    --set opcuaMonitoring.discoveryUrls[1]="opc.tcp://10.1.3.4:4840/" 
```

### Specifying the DiscoveryURLs for specific OPC UA Servers
If an operator knows the DiscoveryURLs for the OPC UA Servers they wants Akri to discover (and deploy brokers to), they
can manually list them when deploying Akri like in the following.
```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri \
    --set imagePullSecrets[0].name="regcred" \
    --set useLatestContainers=true \
    --set opcuaMonitoring.enabled=true \
    --set opcuaMonitoring.brokerPod.env.identifier='SomeVariable' \
    --set opcuaMonitoring.brokerPod.env.namespaceIndex='2' \
    --set opcuaMonitoring.discoveryUrls[0]="opc.tcp://10.123.456.7:4855/"
```

### Specifying the DiscoveryURLs for both LocalDiscoveryServers and Servers
OPC UA discovery can also receive a list of both OPC UA LDS DiscoveryURLs and specific Server urls, as in the following.

```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri \
    --set imagePullSecrets[0].name="regcred" \
    --set useLatestContainers=true \
    --set opcuaMonitoring.enabled=true \
    --set opcuaMonitoring.brokerPod.env.identifier='SomeVariable' \
    --set opcuaMonitoring.brokerPod.env.namespaceIndex='2' \
    --set opcuaMonitoring.discoveryUrls[0]="opc.tcp://10.1.2.3:4840/" \
    --set opcuaMonitoring.discoveryUrls[1]="opc.tcp://10.1.3.4:4840/" \
    --set opcuaMonitoring.discoveryUrls[2]="opc.tcp://10.123.456.7:4855/"
```

**Note**: Agent's OPC UA discovery method only supports tcp DiscoveryURLs, since the [Rust OPC UA
library](https://github.com/locka99/opcua) has yet to support http(s).

### Filtering the Servers by application name
Instead of deploying brokers to all servers registered with specified Local Discovery Servers, an opperator can choose
to include or exclude a list of application names (the `applicationName` property of a server's `ApplicationDescription`
as specified by UA Specification 12). For example, to discover all servers registered with the default LDS except for
the server named "Duke", do the following.
```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri \
    --set imagePullSecrets[0].name="regcred" \
    --set useLatestContainers=true \
    --set opcuaMonitoring.enabled=true \
    --set opcuaMonitoring.brokerPod.env.identifier='SomeVariable' \
    --set opcuaMonitoring.brokerPod.env.namespaceIndex='2' \
    --set opcuaMonitoring.applicationNames.action=Exclude \
    --set opcuaMonitoring.applicationNames.items[0]="Duke"
```
Alternatively, to only discover the server named "Go Tar Heels!", do the following:
```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri \
    --set imagePullSecrets[0].name="regcred" \
    --set useLatestContainers=true \
    --set opcuaMonitoring.enabled=true \
    --set opcuaMonitoring.brokerPod.env.identifier='SomeVariable' \
    --set opcuaMonitoring.brokerPod.env.namespaceIndex='2' \
    --set opcuaMonitoring.applicationNames.action=Include \
    --set opcuaMonitoring.applicationNames.items[0]="Go Tar Heels!"
```

### Mounting OPC UA credentials to enable security
Using security with the OPC UA monitoring broker is optional so long as the discovered OPC UA Servers support a Security
Policy of None, which is the policy the brokers will use if it finds no credentials mounted.

Otherwise, in order for the monitoring broker's OPC UA Client to establish a secure connection with an OPC UA Server,
the Client and Server must trust each other's x509 v3 certificates. This can be done in one of the three ways explained
in the [opcua proposal](./proposals/opcua.md#giving-proper-credentials-to-the-akri-broker). The simplest method is to
sign the OPC UA monitoring broker's certificate with the same Certificate Authority (CA) as the Server with which it
wishes to connect. The certificates are passed to the broker via a Kubernetes Secret mounted as a volume. 

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
Certificates can be created and signed with a CA manually using openssl or using the OPCFoundation [certificate
generator tool](https://github.com/OPCFoundation/Misc-Tools). The monitoring broker, specifically the .NET OPC Client,
expects certificates (both client and CA) to be der files and the private key to be bundled with its certificate in PFX
format, as in the kubectl command above.

Finally, when mounting certificates is enabled with with Helm via `--set opcuaMonitoring.mountCertificates='true'`, the
secret named `opcua-broker-credentials` will be mounted into the OPC UA monitoring brokers. It is mounted to the volume
`credentials` at the `mountPath` /etc/opcua-certs/client-pki, as shown in the [OPC UA monitoring Helm
template](../deployment/helm/templates/opcua-monitoring.yaml). This is the path where the broker expects to find the
certificates. The following is an example how how to enable security:
```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri \
    --set imagePullSecrets[0].name="regcred" \
    --set useLatestContainers=true \
    --set opcuaMonitoring.enabled=true \
    --set opcuaMonitoring.brokerPod.env.identifier='SomeVariable' \
    --set opcuaMonitoring.brokerPod.env.namespaceIndex='2' \
    --set opcuaMonitoring.mountCertificates='true'
```

### Changing the capacity
To modify the Configuration so that an OPC UA Server is accessed by more or fewer protocol broker Pods, update the
`capacity` property to reflect the correct number.  For example, if your high availability needs are met by having only
1 redundant pod, you can update the Configuration like this:
```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri \
    --set imagePullSecrets[0].name="regcred" \
    --set useLatestContainers=true \
    --set opcuaMonitoring.enabled=true \
    --set opcuaMonitoring.brokerPod.env.identifier='SomeVariable' \
    --set opcuaMonitoring.brokerPod.env.namespaceIndex='2' \
    --set opcuaMonitoring.capacity=2
```

## Modifying a Configuration
More information about how to modify an installed Configuration, add additional protocol Configurations to a cluster, or
delete a Configuration can be found in the [Modifying a Akri Installation document](./modifying-a-akri-installation.md).

## Implementation details
The OPC UA implementation can be understood by looking at several things:
1. [OpcuaDiscoveryHandlerConfig](../shared/src/akri/configuration.rs) defines the required properties.
1. [The opcua property in akri-configuration-crd.yaml](../deployment/helm/crds/akri-configuration-crd.yaml) validates
   the CRD input.
1. [OpcuaDiscoveryHandler](../agent/src/protocols/opcua/discovery_handler.rs) defines OPC UA Server discovery.
1. [sample-brokers/opcua-monitoring-broker](../sample-brokers/opcua-monitoring-broker) defines a OPC UA protocol broker
   that monitors an OPC UA Variable with a specific NodeID.