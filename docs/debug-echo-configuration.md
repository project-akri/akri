# Debugging Akri using the Debug Echo Discovery Handler and Configuration
## Background
In order to kick start using and debugging Akri, a "debug echo" Discovery Handler has been created. The Discovery
Handler "discovers" all devices listed in the `descriptions` array in the `discoveryDetails` of a Debug Echo
configuration. Devices are visible to the Discovery Handler so long as the word "OFFLINE" does not exist in the file
`/tmp/debug-echo-availability.txt` in the Pod in which the Discovery Handler is running.

## Deploying the Debug Echo Discovery Handler
In order for the Agent to know how to discover Debug Echo devices, the Debug Echo Discovery Handler must exist. Akri
supports an Agent image that includes all supported Discovery Handlers. This Agent will be used if `agent.full=true`. By
default, a slim Agent without any embedded Discovery Handlers is deployed and the required Discovery Handlers can be
deployed as DaemonSets. This documentation will use that strategy, deploying Debug Echo Discovery Handlers by specifying
`debugEcho.discovery.enabled=true` when installing Akri. Notes are provided for how the steps change if using embedded
Discovery Handlers.

Since the Debug Echo Discovery Handler is for debugging, it's use must be explicitly enabled by setting
`agent.allowDebugEcho=true`.

## Quickstart
### Installation
To install Akri with **external** Debug Echo Discovery Handlers and a Configuration to discover unshared Debug Echo
devices, run:
```bash
helm repo add akri-helm-charts https://deislabs.github.io/akri/
helm install akri akri-helm-charts/akri-dev \
    --set agent.allowDebugEcho=true \
    --set debugEcho.discovery.enabled=true \
    --set debugEcho.configuration.enabled=true \
    --set debugEcho.configuration.shared=false
```

> Note: To instead install Akri with Debug Echo Discovery Handlers **embedded** in the Agent, set `agent.full=true` and
> remove `debugEcho.discovery.enabled=true` like in the following installation:
>```bash
>helm repo add akri-helm-charts https://deislabs.github.io/akri/
>helm install akri akri-helm-charts/akri-dev \
>   --set agent.allowDebugEcho=true \
>   --set agent.full=true \
>   --set debugEcho.configuration.enabled=true \
>   --set debugEcho.configuration.shared=false
>```

By default, the Debug Echo Configuration discovers two devices, `foo1` and `foo2`, and automatically deploys an empty
nginx broker Pod to each discovered device, so you should see two instances and brokers created as a result of your
installation. By default, it also creates an Instance service for each device and a Configuration service for all
discovered devices. The Akri Agents, Controller, and (if using external Discovery Handlers) Debug Echo Discovery
Handlers should also be created.

```sh
watch kubectl get pods,akric,akrii,services -o wide
```

Set `debugEcho.configuration.shared=true` to discover Debug Echo devices that are shared by all nodes. For example, when
Akri is installed like above with `debugEcho.configuration.shared=false` onto a 3 node cluster. 6 Debug Echo devices
will be discovered and 6 Instances will be created, 2 for each Node. However, is `debugEcho.configuration.shared=true`
is set, only 2 will be discovered as it is mocking all 3 nodes "utilizing" the same two devices. Set
`debugEcho.configuration.capacity=3` to allow all 3 nodes to receive brokers to utilize each of the shared devices. It
defaults to `1`. 

### Marking Devices "OFFLINE"
Debug Echo devices are "unplugged"/"disconnected" by writing `"OFFLINE"` into the `/tmp/debug-echo-availability.txt`
file inside the pod in which the Discovery Handler is running.

By default, Debug Echo Discovery Handlers run in their own Pods, so exec into each to mark the devices offline. For
single a single node cluster:
```sh 
DEBUG_ECHO_DH_POD_NAME=$(kubectl get pods --selector=name=akri-debug-echo-discovery | grep akri | awk '{print $1}')
kubectl exec -i $DEBUG_ECHO_DH_POD_NAME -- /bin/bash -c "echo "OFFLINE" > /tmp/debug-echo-availability.txt"
```
>Note: `shared` devices have a 5 minute grace period before their instances are deleted, as they are more often network
>devices prone to intermittent connectivity.

>Note: For, multi-node clusters, each Agent or Debug Echo Discovery Handler must be `exec`ed into. 

> Note: If `agent.full=true` was specified when installing Akri, the Debug Echo Discovery Handlers run inside the Agent,
> so exec into each Agent to mark the devices offline. For single a single node cluster:
> ```sh 
> AGENT_POD_NAME=$(kubectl get pods --selector=name=akri-agent | grep akri | awk '{print $1}')
> kubectl exec -i $AGENT_POD_NAME -- /bin/bash -c "echo "OFFLINE" > /tmp/debug-echo-availability.txt"
> ```

Caveat: **Debug Echo devices likely should not be marked as shared for multi-node clusters**. This is because the
contents of `/tmp/debug-echo-availability.txt` could be different for each node. If one node marks a device as "OFFLINE"
but another does not, there is inconsistency around the existence of the device. However, this may be a scenario you
want to consider or test.

### Marking Devices "ONLINE"
Debug Echo devices are "plugged in"/"reconnected" by removing `"OFFLINE"` from the `/tmp/debug-echo-availability.txt`
file inside the pod in which the Discovery Handler is running. The commands below replace the file contents with
`"ONLINE"`.

By default, Debug Echo Discovery Handlers run in their own Pods, so exec into each to mark the devices offline. For
single a single node cluster:
```sh 
DEBUG_ECHO_DH_POD_NAME=$(kubectl get pods --selector=name=akri-debug-echo-discovery | grep akri | awk '{print $1}')
kubectl exec -i $DEBUG_ECHO_DH_POD_NAME -- /bin/bash -c "echo "ONLINE" > /tmp/debug-echo-availability.txt"
```

>Note: For, multi-node clusters, each Agent or Debug Echo Discovery Handler must be `exec`ed into. 

> Note: If `agent.full=true` was specified when installing Akri, the Debug Echo Discovery Handlers run inside the Agent,
> so exec into each Agent to mark the devices offline. For single a single node cluster:
> ```sh 
> AGENT_POD_NAME=$(kubectl get pods --selector=name=akri-agent | grep akri | awk '{print $1}')
> kubectl exec -i $AGENT_POD_NAME -- /bin/bash -c "echo "ONLINE" > /tmp/debug-echo-availability.txt"
> ```

## In the Weeds: Debug Echo Configuration Settings

## Discovery Handler Discovery Details Settings
Discovery Handlers are passed discovery details that are set in a Configuration to determine what to discover, filter
out of discovery, and so on. The Debug Echo Discovery Handler simply "discovers" a device for each string in
`discoveryDetails.descriptions` in a Configuration.

| Helm Key | Value | Default | Description |
|---|---|---|---|
| debugEcho.configuration.discoveryDetails.description | array of arbitrary Strings | ["foo1", "foo2"] | Names for fake devices that will be discovered | 

### Broker Pod Settings
By default, brokers are deployed to discovered Debug Echo devices. Set
`debugEcho.configuration.brokerPod.image.repository=""` to not deploy broker Pods. | Helm Key | Value | Default |
Description |
|---|---|---|---|
| debugEcho.configuration.brokerPod.image.repository | image string | nginx | image of broker Pod that should be
deployed to discovered devices | | debugEcho.configuration.brokerPod.image.tag | tag string | "latest" | image tag of
broker Pod that should be deployed to discovered devices |

### Disabling Automatic Service Creation
By default, if a broker Pod is specified, the Debug ECho Configuration will create services for all the brokers of a
specific Akri Instance and all the brokers of an Akri Configuration. The creation of these services can be disabled. |
Helm Key | Value | Default | Description |
|---|---|---|---|
| debugEcho.configuration.createInstanceServices | true, false | true | a service should be automatically created for
each broker Pod | | debugEcho.configuration.createConfigurationService | true, false | true | a single service should be
created for all brokers of a Configuration |

### Capacity Setting
By default, if a broker Pod is specified, a single broker Pod is deployed to each device. To modify the Configuration so
that an OPC UA server is accessed by more or fewer nodes via broker Pods, update the `debugEcho.configuration.capacity`
setting to reflect the correct number. For example, if your high availability needs are met by having 1 redundant pod,
you can update the Configuration like this by setting `debugEcho.configuration.capacity=2`. | Helm Key | Value | Default
| Description |
|---|---|---|---|
| debugEcho.configuration.capacity | number | 1 | maximum number of brokers that can be deployed to utilize a device (up
to 1 per Node) |

## Modifying a Configuration
Akri has provided further documentation on [modifying the broker
PodSpec](./customizing-akri-installation.md#modifying-the-brokerpodspec), [instanceServiceSpec, or
configurationServiceSpec](./customizing-akri-installation.md#modifying-instanceservicespec-or-configurationservicespec)
More information about how to modify an installed Configuration, add additional Configurations to a cluster, or delete a
Configuration can be found in the [Customizing an Akri Installation document](./customizing-akri-installation.md).

## Implementation details
The DebugEcho implementation can be understood by looking at its [Discovery
Handler](../discovery-handlers/debug-echo/src/discovery_handler.rs), which contains the `DebugEchoDiscoveryDetails`
struct, which describes the expected format of a Configuration's `DiscoveryDetails`.