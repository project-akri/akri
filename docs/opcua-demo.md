# OPC UA End-to-End Demo
OPC UA is a communication protocol for industrial automation. It is a client/server technology that comes with a
security and communication framework. This demo will help you get started using Akri to discover OPC UA Servers and
utilize them via a broker that contains an OPC UA Client. Specifically, a Akri Configuration called OPC UA Monitoring
was created for this scenario, which will show how Akri can be used to detect anomaly values of a specific OPC UA
Variable. To do so, the OPC UA Clients in the brokers will subscribe to that variable and serve it's value over gRPC for
an anomaly detection web application to consume. This Configuration could be used to monitor a barometer, CO detector,
and more; however, for this example, that variable will represent the temperature of a thermostat and any value outside
the range of 70-80 degrees is an anomaly. 

The demo consists of the following components:
1. Two .NET OPC UA Servers with a temperature variable
1. (Optional) Certificates for the Servers and Akri brokers
1. An OPC UA Monitoring broker that contains an OPC UA Client that subscribes to a specific NodeID (for that temperature
   variable)
1. A Akri installation
1. An anomaly detection web application

## Demo Flow
<img src="./media/opcua-demo-diagram.svg" alt="Demo Overview" style="padding-bottom: 10px padding-top: 10px;
margin-right: auto; display: block; margin-left: auto;"/>
1. An operator (meaning you!) applies to a single-node cluster the OPC UA Monitoring Configuration, which specifies
   the addresses of the OPC UA Servers, which OPC UA Variable to monitor, and whether to use security.
1. Agent sees the OPC UA Monitoring Configuration, discovers the servers specified in the Configuration, and creates an
   Instance for each server.
1. The Akri Controller sees the Instances in etcd and schedules an OPC UA Monitoring broker pod for each server.
1. Once the OPC UA Monitoring broker pod starts up, it will create an OPC UA Client that will create a secure channel
   with its server.
1. The OPC UA Client will subscribe to the OPC UA Variable with the NodeID with `Identifier` "Thermometer_Temperature"
   and `NamespaceIndex` 2 as specified in the OPC UA Configuration. The server will publish any time the value of that
   variable changes.
1. The OPC UA Monitoring broker will serve over gRPC the latest value of the OPC UA Variable and the address of the OPC
   UA Server that published the value.
1. The anomaly detection web application will test whether that value is an outlier to its pre-configured dataset. It
   then will display a log of the values on a web application, showing outliers in red and normal values in green.

The following steps need to be completed to run the demo:
1. Setting up a single-node cluster with MicroK8s
1. (Optional) Creating X.509 v3 Certificates for the servers and Akri broker and storing them in a Kubernetes Secret
1. Creating two OPC UA Servers
1. Running Akri
1. Deploying an anomaly detection web application as an end consumer of the brokers

If at any point in the demo, you want to dive deeper into OPC UA or clarify a term, you can reference the [online OPC UA
specifications](https://reference.opcfoundation.org/v104/).

## Setting up a single-node cluster with MicroK8s
Before running Akri, we must first set up a single-node Kubernetes cluster. Instead of using native Kubernetes, we will
use MicroK8s, which is the smallest Kubernetes and can be installed quickly.
1. To run Akri, you will need to obtain a Linux environment. For this demo, we recommend setting up an Ubuntu VM. See the [MicroK8s documentation](https://microk8s.io/docs) for details about which Ubuntu versions are currently supported and recommended VM size.
1. Install [MicroK8s](https://microk8s.io).
    ```sh
    sudo snap install microk8s --classic --channel=1.18/stable
    ```
2. Grant admin privilege for running MicroK8s commands.
    ```sh
    sudo usermod -a -G microk8s $USER
    sudo chown -f -R $USER ~/.kube
    su - $USER
    ```
1. Check MicroK8s status.
    ```sh
    microk8s status --wait-ready
    ```
1. If you don't have an existing `kubectl` installation, add a kubectl alias. If you do not want to set an alias, add
   `microk8s` in front of all kubectl commands.
    ```sh
    alias kubectl='microk8s kubectl'
    ```
1. Install Helm, the Kubernetes package manager, which we will use to install Akri.
    ```sh
    sudo apt install -y curl
    curl -L https://raw.githubusercontent.com/helm/helm/master/scripts/get-helm-3 | bash
    ```
1. Enable Helm for MicroK8s.
    ```sh
    kubectl config view --raw >~/.kube/config
    microk8s enable helm3
    ```
1. Enable dns.
    ```sh
    microk8s enable dns
    ```
1. Since device plugins must run in a privileged context, enable privileged pods and restart MicroK8s.
    ```sh
    echo "--allow-privileged=true" >> /var/snap/microk8s/current/args/kube-apiserver
    microk8s.stop
    microk8s.start
    ```
1. Since MicroK8s by default does not have a node with the label `node-role.kubernetes.io/master=`, add the label to the
   control plane node so the controller gets scheduled.
    ```sh
    kubectl label node ${HOSTNAME,,} node-role.kubernetes.io/master= --overwrite=true
    ```
1. Apply Docker secret to cluster in order to pull down Akri Agent, Controller, Broker, and anomaly detection app pods.
    ```sh
    kubectl create secret docker-registry regcred --docker-server=ghcr.io  --docker-username=<request username> --docker-password=<request password>
    ```

## Creating X.509 v3 Certificates
**If security is not desired, this section can be skipped, each monitoring broker will use an OPC UA Security Policy
of None if it cannot find credentials mounted in its pod.**

Akri will deploy an OPC UA Monitoring broker for each OPC UA Server a node in the cluster can see. This broker contains
an OPC UA Client that will need the proper credentials in order to communicate with the OPC UA Server in a secure
fashion. Specifically, before establishing a session, an OPC UA Client and Server must create a secure channel over the
communication layer to ensure message integrity, confidentiality, and application authentication. Proper application
credentials in the form of X.509 v3 certificates are needed for application authentication.

Every OPC UA Application, whether Client, Server, or DiscoveryServer, has a certificate store, which includes the application's own credentials along with a list of trusted and rejected application instance certificates. According to OPC UA specification, there are three ways to configure OPC UA Server and Clients' certificate stores so that they trust each other's certificates, which are explained in the [OPC UA proposal](./proposals/opcua.md). This demo will walk through the third method of creating
Client and Server certificates that are issued by a common Certificate Authority (CA). Then, that CA's certificate
simply needs to be added to the trusted folder of Client and Servers' certificate stores, and they will automatically trust each other on the basis of having a common CA. The following image walks through how to configure the Client and Server certificate stores for Akri.

<img src="./media/opcua-certificates-diagram.svg" alt="OPC UA Certificate Creation Diagram" style="padding-bottom:
10px padding-top: 10px; margin-right: auto; display: block; margin-left: auto;"/>
1. Generate an X.509 v3 Certificate for Akri OPC UA Monitoring brokers and sign it with the same CA that has signed the
   certificates of all the OPC UA Servers that will be discovered.
1. Create a Kubernetes Secret named opcua-broker-credentials that contains four items with the following key names:
   client_certificate, client_key, ca_certificate, and ca_crl.  
1. The credentials will be mounted in the broker at the path /etc/opcua-certs/client-pki.

### Running the certificate creation application
A .NET Console [OPC UA Certificate Generator application](../samples/opcua-certificate-generator) has been created to simplify the process of creating a Certificate Authority (CA) and X.509 v3 certificates issued by that CA for the OPC UA Client and Servers in this demo. Clone the Akri repository, navigate to the `opcua-certificate-generator` and follow the instructions of the [README](../samples/opcua-certificate-generator/README.md) to generate the necessary certificates.

### Creating an opcua-broker-credentials Kubernetes Secret
The OPC UA Client certificate will be passed to the OPC UA Monitoring broker as a Kubernetes Secret mounted as a volume.
Read more about the decision to use Kubernetes secrets to pass the Client certificates in the [Credentials Passing
Proposal](./proposals/credentials-passing.md). Create a Kubernetes Secret, projecting each certificate/crl/private key
with the expected key name (ie `client_certificate`, `client_key`, `ca_certificate`, and `ca_crl`). Specify the file
paths such that they point to the credentials made in the previous section.
```bash
microk8s kubectl create secret generic opcua-broker-credentials \
--from-file=client_certificate=/path/to/AkriBroker/own/certs/AkriBroker [<hash>].der \
--from-file=client_key=/path/to/AkriBroker/own/private/AkriBroker [<hash>].pfx \
--from-file=ca_certificate=/path/to/ca/certs/SomeCA [<hash>].der \
--from-file=ca_crl=/path/to/ca/crl/SomeCA [<hash>].crl
```

When mounting certificates is enabled later in the [Running Akri section](#running-akri) with Helm via `--set
opcuaMonitoring.mountCertificates='true'`, the secret named `opcua-broker-credentials` will be mounted into the OPC UA
monitoring brokers. It is mounted to the volume `credentials` at the `mountPath` /etc/opcua-certs/client-pki, as shown
in the [OPC UA monitoring helm template](../deployment/helm/templates/opcua-monitoring.yaml). This is the path where the
brokers expect to find the certificates.

## Creating OPC UA Servers
Now, we must create some OPC UA Servers to discover. Instead of starting from scratch, we make some small modifications
to the OPC Foundation's .NET Console Reference Server.

1. Clone the [repository](https://github.com/OPCFoundation/UA-.NETStandard).

1. Open the UA Reference solution file and navigate to NetCoreReferenceServer project.

1. Open `Quickstarts.Reference.Config.xml`. This application configuration file is where many features can configured,
   such as the application description (application name, uri, etc), security configuration, and base address. Only the
   latter needs to be modified if using no security. On lines 76 and 77, modify the address of the server, by replacing
   `localhost` with the IP address of the machine the server is running on. If left as   `localhost` the application
   will automatically replace it with the hostname of the machine which will be unreachable to the broker pod. On the
   same lines, modify the ports if they are already taken. Akri will preference using the tcp endpoint, since according to
   the [OPC UA Security Specification](https://reference.opcfoundation.org/v104/Core/docs/Part2/4.10/), secure channels
   over HTTPS do not provide application authentication.

1. (Optional) If using security, and you have already created certificates in the previous section, now you can modify
   the security configuration inside `Quickstarts.Reference.Config.xml` to point to those certificates. After using the
   OPC UA certificate generator application, your first Server's certificate store folder should be named SomeServer0. In line
   17, change the `StorePath` to be `/path/to/SomeServer0/own`. Do the same in lines 24, 30, and 36, replacing
   `%LocalApplicationData%/OPC Foundation/pki/` with `/path/to/SomeServer0`. Finally, change the subject name in line 18
   to be `CN=SomeServer0`. 

1. Now it is time to create our temperature OPC UA Variable. Navigate to the function `CreateAddressSpace` on line 174
    of `ReferenceNodeManager.cs` that creates the AddressSpace of the OPC UA Server. To review some terms, [OPC UA
    specification](https://reference.opcfoundation.org/v104/Core/docs/Part1/3.2/) defines AddressSpace as the
    "collection of information that a Server makes visible to its Clients", a Node as "a fundamental component of an
    AddressSpace", and a Variable as a "Node that contains a value". Let create a thermometer Node which has a
    temperature variable. On line 195, insert the following:
    ```c#
    #region Thermometer
    FolderState thermometerFolder = CreateFolder(root, "Thermometer", "Thermometer");
    CreateDynamicVariable(thermometerFolder, "Thermometer_Temperature", "Temperature", DataTypeIds.Int16, ValueRanks.Scalar);
    #endregion
    ```
    We selected the `root` folder as the parent of the Thermometer node, which is the `CTT` folder created in line 185.
    The path to our Thermometer node is Server/CTT/Thermometer, making the NamespaceIndex of the Thermometer Node (and
    its variables) 2. We care about the `NamespaceIndex` because it along with `Identifier`, are the two fields to a
    `NodeId`. If you inspect the `CreateDynamicVariable` function, you will see that it creates an OPC UA variable,
    using the `path` parameter ("Thermometer_Temperature") as the `Identifier` when creating the NodeID for that variable.
    It then add the variable to the `m_dynamicNodes` list. At the bottom of `CreateAddressSpace` the following line
    initializes a simulation that will periodically change the value of all the variables in `m_dynamicNodes`: 
    ``` c#
    m_simulationTimer = new Timer(DoSimulation, null, 1000, 1000);
    ```
    Lets change the simulation so that it usually returns a value between 70-80 and periodically returns an outlier of
    120. Go to the `DoSimulation` function. Replace `variable.Value = GetNewValue(variable);` with the following
    ```c#
    Random rnd = new Random();
    int value = rnd.Next(70, 80);
    if (value == 75)
    {
        value = 120;
    }
    variable.Value = value;
    ```
    Congrats! You've set up your first OPC UA Server. You should now be able to run it.

1. Repeat all the steps above to create a second OPC UA Server, using SomeServer1 certificates for step 4 if using
   security. In step 3, be sure your servers have different base address by modifying the port or running the second
   Server on a different host.

## Running Akri
1. Make sure your OPC UA Servers are running
1. Now it is time to install the Akri using Helm. We can specify that when installing Akri, we also want to create an
   OPC UA Monitoring configuration by setting the helm value `--set opcuaMonitoring.enabled=true`. We must also specify
   the `Identifier` and `NamespaceIndex` of the NodeID we want the brokers to monitor. These values are mounted as
   environment variables in the brokers. In our case that is our temperature variable we made earlier, which has an
   `Identifier` of `Thermometer_Temperature` and `NamespaceIndex` of `2`. Finally, since we did not set up a
   Local Discovery Server -- see [Setting up and using a Local Discovery Server](#setting-up-and-using-a-local-discovery-server-(windows-only)) in the Extensions section at the bottom of this document to use a LDS -- we must specify the DiscoveryURLs of the OPC UA Servers we want Agent
   to discover. Those are the tcp addresses that we modified in step 3 of [Creating OPC UA
   Servers](#creating-opc-ua-servers). Be sure to set the appropriate IP address and port number for the DiscoveryURLs
   in the Helm command below. If using security, uncomment `--set opcuaMonitoring.mountCertificates='true'`.   
    ```sh
    helm repo add akri-helm-charts https://deislabs.github.io/akri/
    helm install akri akri-helm-charts/akri \
        --set imagePullSecrets[0].name="regcred" \
        --set useLatestContainers=true \
        --set opcuaMonitoring.enabled=true \
        --set opcuaMonitoring.brokerPod.env.identifier='Thermometer_Temperature' \
        --set opcuaMonitoring.brokerPod.env.namespaceIndex='2' \
        --set opcuaMonitoring.discoveryUrls[0]="opc.tcp://<SomeServer0 IP address>:<SomeServer0 port>/Quickstarts/ReferenceServer/" \
        --set opcuaMonitoring.discoveryUrls[1]="opc.tcp://<SomeServer1 IP address>:<SomeServer1 port>/Quickstarts/ReferenceServer/" \
        # --set opcuaMonitoring.mountCertificates='true'
    ```
    Akri Agent will discover the two Servers and create an Instance for each Server. Watch two broker pods spin up,
    one for each Server.
    ```sh
    watch microk8s kubectl get pods
    ```
    To inspect more of the elements of Akri:
    - Run `kubectl get crd`, and you should see the CRDs listed.
    - Run `kubectl get akric`, and you should see `akri-opcua-monitoring`. 
    - If OPC UA Servers were discovered and pods spun up, the instances can be seen by running `kubectl get akrii` and
      further inspected by running `kubectl get akrii akri-opcua-monitoring-<ID> -o yaml`

## Deploying an anomaly detection web application as an end consumer of the brokers
A sample anomaly detection web application was created for this end-to-end demo. It has a gRPC stub that calls the
brokers' gRPC clients, getting the latest temperature value. It then determines whether this value is an outlier to the
dataset using the Local Outlier Factor strategy. The dataset is simply a csv with the numbers between 70-80 repeated
several times; therefore, any value significantly outside this range will be seen as an outlier. The web application
serves as a log, displaying all the temperature values and the address of the OPC UA Server that sent the value. It
shows anomaly values in red. The anomalies always have a value of 120 due to how we set up the `DoSimulation` function
in the OPC UA Servers.
1. Download the anomaly detection app deployment yaml, deploy the application, and watch a pod spin up for the app.
    ```sh
    # This file url is not available while the Akri repo is private.  To get a valid url, open 
    # https://github.com/deislabs/akri/blob/main/deployment/samples/anomaly-detection-app.yaml
    # and click the "Raw" button ... this will generate a link with a token that can be used below.
    curl -o anomaly-detection-app.yaml <RAW LINK WITH TOKEN>
    kubectl apply -f anomaly-detection-app.yaml
    watch microk8s kubectl get pods -o wide
    ```
1. Determine which port the service is running on.
    ```sh
    kubectl get services
    ```
    Something like the following will be displayed. The ids of the broker services (`akri-opcua-monitoring-<id>-svc`) will likely
    be different as they are determined by hostname.
    ```
    NAME                                TYPE        CLUSTER-IP       EXTERNAL-IP   PORT(S)        AGE
    anomaly-detection-app               NodePort    10.XXX.XXX.XXX   <none>        80:32624/TCP   66s
    kubernetes                          ClusterIP   10.XXX.XXX.X     <none>        443/TCP        15d
    akri-opcua-monitoring-7dd1e7-svc   ClusterIP   10.XXX.XXX.XXX   <none>        80/TCP         3m38s
    akri-opcua-monitoring-5fc2e6-svc   ClusterIP   10.XXX.XXX.XXX   <none>        80/TCP         3m38s
    akri-opcua-monitoring-svc          ClusterIP   10.XXX.XXX.XXX   <none>        80/TCP         3m38s
    ```
1. Navigate in your browser to http://ip-address:32624/ where ip-address is the IP address of your Ubuntu VM (not the
   cluster-IP) and the port number is from the output of `kubectl get services`. It takes 3 seconds for the site to
   load, after which, you should a log of the temperature values, which updates every few seconds. Note how the values
   are coming from two different DiscoveryURLs, namely the ones for each of the two OPC UA Servers.

## Clean up
1. Delete the anomaly detection application deployment and watch the pod be brought down.
    ```sh
    kubectl delete -f anomaly-detection-app.yaml
    watch microk8s kubectl get pods
    ```
1. Delete the OPC UA Monitoring Configuration and watch the instances, pods, and services be deleted.
    ```sh
    kubectl delete akric akri-opcua-monitoring
    watch microk8s kubectl get pods,services,akric,akrii -o wide
    ```
1. Bring down the Akri Agent, Controller, and CRDs.
    ```sh
    helm delete akri
    kubectl delete crd instances.akri.sh
    kubectl delete crd configurations.akri.sh
    ```

## Extensions
Now that you have the end to end demo running lets talk about some ways you can go beyond the demo to better understand
the advantages of Akri. This section will cover:
1. Adding a node to the cluster
1. Using a Local Discovery Server to discover the Servers instead of passing the DiscoveryURLs to the OPC UA Monitoring Configuration
1. Modifying the OPC UA Monitoring Configuration to filter out an OPC UA Server
1. Creating a different broker and end application
1. Creating a new OPC UA Configuration

### Adding a Node to the MicroK8s Cluster
To see how Akri easily scales as nodes are added to the cluster, let create another MicroK8s instance and add it to our
cluster.
1. Create another MicroK8s instance, following the same steps as in [Setting up a single-node cluster with
   Microk8s](#setting-up-a-single-node-cluster-with-microk8s) above, skipping the second to last step of labeling the
   control plane node since this node will be acting as a worker in the cluster.
1. In your first VM that is currently running Akri, get the join command by running:
```
microk8s add-node
```
1. In your new VM, run one of the join commands output in the previous step. Go back to your control plane VM and
   you should be able to see the node joined:
```sh
kubectl get no
```
1. You can see that another Agent pod and two new OPC UA Monitoring brokers have been deployed to the new node.
```sh
kubectl get pods -o wide
```
1. There are still only two OPC UA Monitoring instances, but now two of the five slots of the deviceUsage section of the
   instances are taken. There are five slots because the default `capacity` for OPC UA Monitoring is 5. This means that
   if you were to add 4 more nodes to the cluster (creating a total of 6), one would not get a broker deployed to it.
```sh
microk8s kubectl get akrii -o yaml
```
1. Let's play around with the capacity value and use the `helm upgrade` command to modify our OPC UA Monitoring
   Configuration such that the capacity is 1. On the control plane node, run the following, once again uncommenting
   `--set opcuaMonitoring.mountCertificates='true'` if using security. Watch as both brokers terminate and only one
   comes back online in a Running state.
```sh
helm upgrade akri akri-helm-charts/akri \
    --set imagePullSecrets[0].name="regcred" \
    --set useLatestContainers=true \
    --set opcuaMonitoring.enabled=true \
    --set opcuaMonitoring.brokerPod.env.identifier='Thermometer_Temperature' \
    --set opcuaMonitoring.brokerPod.env.namespaceIndex='2' \
    --set opcuaMonitoring.discoveryUrls[0]="opc.tcp://<SomeServer0 IP address>:<SomeServer0 port>/Quickstarts/ReferenceServer/" \
    --set opcuaMonitoring.discoveryUrls[1]="opc.tcp://<SomeServer1 IP address>:<SomeServer1 port>/Quickstarts/ReferenceServer/" \
    --set opcuaMonitoring.capacity=1 \
    # --set opcuaMonitoring.mountCertificates='true'
watch microk8s kubectl get pods -o wide
```
**Note**: The fact that the second broker comes back and stays in a Pending state is a known bug and will be fixed.
1. Once you are done using Akri, you can remove your worker node from the cluster by running on the worker node:
```sh
microk8s leave
```
1. To complete the node removal, on the host run the following, inserting the name of the worker node (you can look it
   up with `microk8s kubectl get no`):
```sh
    microk8s remove-node <node name>
```

### Setting up and using a Local Discovery Server (Windows Only)
**This walk-through only supports setting up an LDS on Windows, since that is the OS the OPC Foundation sample LDS
executable was written for.** 

A Local Discovery Server (LDS) is a unique type of OPC UA server which maintains a list of
OPC UA servers that have registered with it. The OPC UA Monitoring Configuration takes in a list of DiscoveryURLs,
whether for LDSes or a specific servers. Rather than having to pass in the DiscoveryURL for every OPC UA Server you want
Akri to discover and deploy brokers to, you can set up a Local Discovery Server on the machine your servers are running
on, make the servers register with the LDS on start up, and pass only the LDS DiscoveryURL into the OPC UA Monitoring
Configuration. Agent will ask the LDS for the addresses of all the servers registered with it and the demo continues as
it would've without an LDS. 

The OPC Foundation has provided a Windows based LDS executable which can be downloaded from their
[website](https://opcfoundation.org/developer-tools/samples-and-tools-unified-architecture/local-discovery-server-lds/).
Download version 1.03.401. It runs as a background service on Windows and can be started or stopped under Windows ->
Services. The OPC Foundation has provided [documentation](https://apps.opcfoundation.org/LDS/) on configuring your LDS.
Most importantly, it states that you must add the LDS executable to your firewall as an inbound rule. The .NET OPC UA
Console Servers that we set up previously are already configured to register with the LDS on its host at the default
address [from OPC UA Specification 12](https://reference.opcfoundation.org/v104/Core/docs/Part6/7.6/) of
`opc.tcp://localhost:4840/`. This is seen on line 205 of `Quickstarts.ReferenceServer.xml`. 

Make sure you have restarted your OPC UA Servers, since they attempt to register with their LDS on start up. Now we can
install Akri with the OPC UA Monitoring Configuration, passing in the LDS DiscoveryURL instead of both server's
DiscoveryURLs. Replace "Windows host IP address" with the IP address of the Windows machine you installed the LDS on
(and is hosting the servers). Be sure to uncomment mounting certificates if you are enabling security:
```sh
helm install akri akri-helm-charts/akri \
    --set imagePullSecrets[0].name="regcred" \
    --set useLatestContainers=true \
    --set opcuaMonitoring.enabled=true \
    --set opcuaMonitoring.brokerPod.env.identifier='Thermometer_Temperature' \
    --set opcuaMonitoring.brokerPod.env.namespaceIndex='2' \
    --set opcuaMonitoring.discoveryUrls[0]="opc.tcp://<Windows host IP address>:4840/" \
    # --set opcuaMonitoring.mountCertificates='true'
```
You can watch as an Instance is created for each Server and two broker pods are spun up.
```sh
watch microk8s kubectl get pods,akrii -o wide
```

### Modifying the OPC UA Monitoring Configuration to filter out an OPC UA Server 
Instead of deploying brokers to all servers registered with specified Local Discovery Servers, an operator can choose
to include or exclude a list of application names (the `applicationName` property of a server's `ApplicationDescription`
as specified by UA Specification 12). For example, to discover all servers registered with the default LDS except for
the server named "SomeServer0", do the following.
```bash
helm install akri akri-helm-charts/akri \
    --set imagePullSecrets[0].name="regcred" \
    --set useLatestContainers=true \
    --set opcuaMonitoring.enabled=true \
    --set opcuaMonitoring.brokerPod.env.identifier='Thermometer_Temperature' \
    --set opcuaMonitoring.brokerPod.env.namespaceIndex='2' \
    --set opcuaMonitoring.discoveryUrls[0]="opc.tcp://<Windows host IP address>:4840/" \
    --set opcuaMonitoring.applicationNames.action=Exclude \
    --set opcuaMonitoring.applicationNames.items[0]="SomeServer0" \
    # --set opcuaMonitoring.mountCertificates='true'
```
Alternatively, to only discover the server named "SomeServer0", do the following:
```bash
helm install akri akri-helm-charts/akri \
    --set imagePullSecrets[0].name="regcred" \
    --set useLatestContainers=true \
    --set opcuaMonitoring.enabled=true \
    --set opcuaMonitoring.brokerPod.env.identifier='Thermometer_Temperature' \
    --set opcuaMonitoring.brokerPod.env.namespaceIndex='2' \
    --set opcuaMonitoring.discoveryUrls[0]="opc.tcp://<Windows host IP address>:4840/" \
    --set opcuaMonitoring.applicationNames.action=Include \
    --set opcuaMonitoring.applicationNames.items[0]="SomeServer0" \
    # --set opcuaMonitoring.mountCertificates='true'
```
### Creating a different broker and end application
The OPC UA Monitoring broker and anomaly detection application support a very specific scenario: monitoring an OPC UA
Variable for anomalies. Since the OPC UA Monitoring broker mounts the NodeID `Identifier` and `NamespaceIndex` in the
broker pods, so long as you are interested in targeting a specific OPC UA Node, you can change the broker pod and end
application to suit your needs. OPC UA Nodes can be anything from objects, to variables, to events, to functions, so the
options for broker implementations are endless. For example, the brokers could take a more active roll on the servers,
and instead of monitoring an OPC UA Variable of a specific NodeID, they could add OPC UA Variables to an OPC UA Object
with a specific NodeID. Or a broker could invoke an OPC UA Method with a specific NodeID. Once the broker docker
container is created, simply set it as the image for the broker pod.
```sh
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri \
    --set imagePullSecrets[0].name="regcred" \
    --set useLatestContainers=true \
    --set opcuaMonitoring.enabled=true \
    --set opcuaMonitoring.brokerPod.env.identifier='Some_Node_Identifier' \
    --set opcuaMonitoring.brokerPod.env.namespaceIndex='2' \
    --set opcuaMonitoring.discoveryUrls[0]="opc.tcp://<IP address>:<port>/" \
    --set opcuaMonitoring.discoveryUrls[1]="opc.tcp://<IP address>:<port>/" \
    --set opcuaMonitoring.brokerPod.image.repository='<docker image>'
    # --set opcuaMonitoring.mountCertificates='true'
```
Now, your broker will be deployed to all discovered OPC UA servers. Next, you can create a Kubernetes deployment for your
own end application like [anomaly-detection-app.yaml](../deployment/samples/anomaly-detection-app.yaml) and apply it
to your Kubernetes cluster. 

### Creating a new OPC UA Configuration
If the OPC UA Monitoring Configuration does not meet your scenario, say since targeting one NodeID is too limiting or
irrelevant, you can create your own OPC UA Configuration. A good way to start would be by downloading the OPC UA
Monitoring Configuration. 
```sh
helm template akri akri-helm-charts/akri \
    --set opcuaMonitoring.enabled=true \
    --set opcuaMonitoring.brokerPod.env.identifier='identifier_is_required' \
    --set controller.enabled=false \
    --set agent.enabled=false > opcua_configuration.yaml
```
Now, you can modify the `brokerPodSpec`, `instanceServiceSpec`, `configurationServiceSpec`, `properties`, and `capacity`
in  `opcua_configuration.yaml` to suit your needs. To look at the implementation, see the [Akri Configuration struct
definition](../shared/src/akri/configuration.rs#L168). Now, install Akri without any Configurations and apply your OPC
UA Configuration.
```sh
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri \
    --set imagePullSecrets[0].name="regcred" \
    --set useLatestContainers=true
kubectl apply -f opcua_configuration.yaml
```
