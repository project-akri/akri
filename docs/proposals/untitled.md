# OPC UA Implementation

## [Next - ProposalsAkri Security Proposal](https://app.gitbook.com/@akri/s/akri/~/drafts/-McReBFrvf3iY-9fvCnh/proposals/untitled-1)Goal

Agent should have the ability to discover instances via OPC UA and brokers should be able to communicate with instances via OPC UA over a secure connection. In this scenario, instances are OPC UA servers.

## OPC UA Background

OPC UA \(Object Linking and Embedding for Process Control Unified Architecture\) is a communication protocol for industrial automation. It expands beyond and is backwards-compatible with OPC Common which was largely limited to Windows hosts. To learn more about OPC UA, see the [OPC UA Specifications](https://reference.opcfoundation.org/v104/).

## OPC UA Discovery Process

Every OPC UA server has a DiscoveryEndpoint that OPC UA clients can access without establishing a session. The address for this endpoint is defined by a DiscoveryURL. Hence, no credentials are needed for OPC UA discovery; rather, the goal of OPC UA server discovery in Agent is to get a list of valid DiscoveryURLs. There are multiple methods for obtaining this list of DiscoveryURLs that will be implemented as `OpcuaDiscoveryMethods`.

### Discovery via Server DiscoveryURLs

In this scenario, the operator already has a list of DiscoveryURLs for OPC UA servers they want Akri to discover and passes them to Agent via the OPC UA Configuration. Rather than finding the servers, Agent is simply asserting the validity of the DiscoveryURLs and that the servers pass the requirements of any filters also passed in the Configuration.

### Discovery via a Local Discovery Server \(LDS\)

In this scenario, Agent will use the FindServers service on one or more LDSs. A DiscoveryServer is an "application that maintains a list of OPC UA Servers that are available on the network and provides mechanisms for Clients to obtain this list”.1 A LocalDiscoveryServer is a DiscoveryServer implementation, which usually runs on a host at the URL `opc.tcp://localhost:4840/UADiscovery` for TCP.2 An LDS maintains a list of all servers that have **registered with it**, which are usually servers running on the same host. An operator can specify a list of DiscoveryURLs for LDSs in the OPC UA Configuration.

**Note**: Agent's `OpcuaDiscoveryMethod::standard`, which does both of the discovery via Server DiscoveryURLs and discovery via a Local Discovery Server listed above, only supports tcp DiscoveryURLs, since the [Rust OPC UA library](https://github.com/locka99/opcua) has yet to support http\(s\).

### Discovery via Network Scanning

In this scenario, Agent will scan the network for OPC UA Servers, checking common OPC UA ports. Azure Industrial IoT has implemented this functionality in their discovery module.4

## Broker interfacing with an OPC UA Server

Part of an OPC UA broker will be an OPC UA client that will call functions on a Server’s API to request services from a real object. The implemented OPC UA monitoring broker example, will subscribe to a specific Node in a Server's address space, receiving updates whenever the Node's value changes.

![OPC UA Server Address Space](https://reference.opcfoundation.org/src/v104/Core/docs/Part1/readme_files/image008.png)

Before the broker can explore a server's address space, it must establish a secure channel and create a session with the server. OPC UA Clients get the security information needed to establish a secure connection with a server by calling the GetEndpoints service on the DiscoveryEndpoint. This returns an EndpointDescription which can be used to create a secure channel between a Client and a Server.

### Giving Proper Credentials to the Akri Broker

OPC UA has two levels of security: one for the communication layer and the other for the application layer. ![OPC UA Security Layers](https://reference.opcfoundation.org/src/v104/Core/docs/Part2/readme_files/image005.png)

Before establishing a session, an OPC UA Client and Server must create a secure channel over the communication layer to ensure message integrity, confidentiality, and application authentication. Proper application credentials in the form of X.509 v3 certificates are needed for application authentication -- if the transport protocol provides application authentication at all.5 Once an application is authorized, a specific user can be authorized when creating a secure channel via a User Identity Token in the form of username/password, X.509 v3 certificates, or an issued identity token \(such as Kerberos tokens\). If no user information is available, a client can use the Anonymous User Identity Token. Currently, Brokers will have to establish sessions with OPC UA Servers with this Anonymous User Identity token, as only application layer authentication will be supported in the OPC UA Configuration.

Every OPC UA Application, whether Client, Server, or DiscoveryServer, has a certificate store, which includes a list of trusted and rejected application instance certificates. According to OPC UA specification, there are three ways to configure OPC UA Server and Clients' certificate stores so that they trust each other's certificates. 1. The Client certificate is added to a Server's trusted folder and vice versa. 2. The Client and Server certificates are issued by a common intermediate CA and that intermediate CA's certificate is added to each of their trusted folders. Additionally, for certificate chains, the certificates of all other intermediate CAs in the chain along with the root CA must be put in their issuers folders.6 3. The Client and Server certificates are issued by a common root Certificate Authority \(CA\) and that CA's certificate is added to each of their trusted folders.

The following image shows what the certificate store of a Client should look like for strategies 2 and 3 described above. ![Certificate Store Configuration](https://documentation.unified-automation.com/uasdkhp/1.0.0/html/certificate_store_clientx.png)

If an operator wishes the OPC UA brokers to establish secure connections with their discovered OPC UA Servers, they must configure a certificate store for the brokers and pass it to the brokers as a Kubernetes Secret mounted as a volume in the broker PodSpec. A broker should know the path at which to find the certificates. The OPC UA monitoring broker, for example, expects to find the certificates mounted at `/etc/opcua-cert/client-pki/`. Learn more about passing credentials via Kubernetes Secrets by reading the [credentials passing proposal](credentials-passing-in-akri.md).

1 [DiscoveryServer definition](https://reference.opcfoundation.org/v104/GDS/docs/3.1/)

2 [Well known addresses for Local Discovery Servers](https://reference.opcfoundation.org/v104/Core/docs/Part6/7.6/)

3 [Local Discovery Server image source](https://reference.opcfoundation.org/src/v104/GDS/docs/readme_files/image006.png)

4 [Azure Industrial Iot Discovery Module](https://github.com/Azure/Industrial-IoT/blob/master/docs/modules/discovery.md)

5 According to the [OPC UA Security Specification](https://reference.opcfoundation.org/v104/Core/docs/Part2/4.10/), secure channels over HTTPS do not provide application authentication.

6 [Good OPC UA security configuration visual overview](https://documentation.unified-automation.com/uasdkhp/1.0.0/html/_l2_ua_discovery_connect.html)

