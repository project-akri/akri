# Custom Discovery Handlers

Akri has [implemented discovery via several protocols](../community/roadmap.md#implement-additional-discovery-handlers) with sample brokers and applications to demonstrate usage. However, there may be protocols you would like to use to discover resources that have not been implemented as Discovery Handlers yet. To enable the discovery of resources via a new protocol, you will implement a Discovery Handler \(DH\), which does discovery on behalf of the Agent. A Discovery Handler is anything that implements the `DiscoveryHandler` service and `Registration` client defined in the [Akri's discovery gRPC proto file](https://github.com/deislabs/akri/blob/main/discovery-utils/proto/discovery.proto). These DHs run as their own Pods and are expected to register with the Agent, which hosts the `Registration` service defined in the gRPC interface.

This document will walk you through the development steps to implement a Discovery Handler. If you would rather walk through an example, see Akri's [extensibility demo](development-walkthrough.md), which walks through creating a Discovery Handler that discovers HTTP based devices. This document will also cover the steps to get your Discovery Handler added to Akri, should you wish to [contribute it back](../community/contributing.md).

Before continuing, you may wish to reference the [Akri architecture](../architecture/architecture-overview.md) and [Akri agent](../architecture/agent-in-depth.md) documentation. They will provide a good understanding of Akri, how it works, and what components it is composed of.

A Discovery Handler can be written in any language using protobuf; however, Akri has provided a template for accelerating the development of Rust Discovery Handlers. This document will walk through both of those options. If using the Rust template, still read through the non-Rust section to gain context on the Discovery Handler interface.

## Creating a Discovery Handler using Akri's Discovery Handler proto file

This section covers how to use [Akri's discovery gRPC proto file](https://github.com/deislabs/akri/blob/main/discovery-utils/proto/discovery.proto) to create a Discovery Handler in the language of your choosing. It consists of three steps: 

1. Registering your Discovery Handler with the Akri Agent
2. Specifying device filtering in a Configuration
3. Implementing the `DiscoveryHandler` service

### Registering with the Akri Agent

Discovery Handlers and Agents run on each worker Node in a cluster. A Discovery Handler should register with the Agent running on its Node at the Agent's registration socket, which defaults to `/var/lib/akri/agent-registration.sock`. The directory can be changed when installing Akri by setting `agent.host.discoveryHandlers`. For example, to request that the Agent's `Registration` service live at `~/akri/sockets/agent-registration.sock` set `agent.host.discoveryHandlers=~/akri/sockets` when installing Akri. The Agent hosts the `Registration` service defined in [Akri's discovery interface](https://github.com/deislabs/akri/blob/main/discovery-utils/proto/discovery.proto) on this socket.

When registering with the Agent, a Discovery Handler specifies its name \(the one that will later be specified in Configurations\), the endpoint of its Discovery Handler service, and whether the devices it discovers are shared \(visible to multiple nodes\).

```text
message RegisterDiscoveryHandlerRequest {
    // Name of the `DiscoveryHandler`. This name is specified in an
    // Akri Configuration, to request devices discovered by this `DiscoveryHandler`.
    string name = 1;
    // Endpoint for the registering `DiscoveryHandler`
    string endpoint = 2;
    // Specifies the type of endpoint.
    enum EndpointType {
        UDS = 0;
        NETWORK = 1;
    }
    EndpointType endpoint_type = 3;
    // Specifies whether this device could be used by multiple nodes (e.g. an IP camera)
    // or can only be ever be discovered by a single node (e.g. a local USB device) 
    bool shared = 4;
}
```

Also note, that a Discovery Handler must also specify an `EndpointType` of either `UDS` or `Network` in the `RegisterDiscoveryHandlerRequest`. While Discovery Handlers must register with the Agent's `Registration` service over UDS, a `DiscoveryHandler` service can run over UDS or an IP based endpoint. However, the current convention is to use UDS for both registration and discovery.

### Specifying device filtering in a Configuration

Discovery Handlers are passed information about what subset of devices to discover from a Configuration's `discoveryDetails`. Akri's Configuration CRD takes in [`DiscoveryHandlerInfo`](https://github.com/deislabs/akri/blob/main/shared/src/akri/configuration.rs), which is defined structurally in Rust as follows:

```rust
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryHandlerInfo {
    pub name: String,
    #[serde(default)]
    pub discovery_details: String,
}
```

When creating a Discovery Handler, you must decide what name to give it and add any details you would like your Discovery Handler to receive in the `discovery_details` string. The Agent passes this string to Discovery Handlers as part of a `DiscoverRequest`. A Discovery Handler must then parse this string -- Akri's built in Discovery Handlers store an expected structure in it as serialized YAML -- to determine what to discover, filter out of discovery, and so on.

For example, a Configuration that uses the ONVIF Discovery Handler, which allows filtering IP cameras by IP address, MAC address, and scopes, looks like the following.

```yaml
apiVersion: akri.sh/v0
kind: Configuration
metadata:
name: http
spec:
discoveryHandler:
    name: onvif
    discoveryDetails: |+
        ipAddresses: 
        action: Exclude
        items:
        - 10.0.0.1
        - 10.0.0.2
        macAddresses:
        action: Exclude
        items: []
        scopes:
        action: Include
        items:
        - onvif://www.onvif.org/name/GreatONVIFCamera
        - onvif://www.onvif.org/name/AwesomeONVIFCamera
        discoveryTimeoutSeconds: 2
```

The `discoveryHandler.name` must match `RegisterDiscoveryHandlerRequest.name` the Discovery Handler uses when registering with the Agent. Once you know what will be passed to your Discovery Handler, its time to implement the discovery functionality.

### Implementing the `DiscoveryHandler` service

The service should have all the functionality desired for discovering devices via your protocol and filtering for only the desired set. Each device a Discovery Handler discovers is represented by the `Device` type, as shown in a subset of the [discovery proto file](https://github.com/deislabs/akri/blob/main/discovery-utils/proto/discovery.proto) below. A Discovery Handler sets a unique `id` for the device, device connection information that needs to be set as environment variables in Pods that request the device in `properties`, and any mounts or devices that should be available to requesting Pods.

```text
message DiscoverResponse {
    // List of discovered devices
    repeated Device devices = 1;
}

message Device {
    // Identifier for this device
    string id = 1;
    // Properties that identify the device. These are stored in the device's instance
    // and set as environment variables in the device's broker Pods. May be information
    // about where to find the device such as an RTSP URL or a device node (e.g. `/dev/video1`)
    map<string, string> properties = 2;
    // Optionally specify mounts for Pods that request this device as a resource
    repeated Mount mounts = 3;
    // Optionally specify device information to be mounted for Pods that request this device as a resource
    repeated DeviceSpec device_specs = 4;
}
```

Note, `discover` creates a streamed connection with the Agent, where the Agent gets the receiving end of the channel and the Discovery Handler sends device updates via the sending end of the channel. If the Agent drops its end, the Discovery Handler should stop discovery and attempt to re-register with the Agent. The Agent may drop its end due to an error or a deleted Configuration.

## Creating a Discovery Handler in Rust using a template

Rust Discovery Handler development can be kick-started using Akri's [Discovery Handler template](https://github.com/kate-goldenring/akri-discovery-handler-template) and [`cargo-generate`](https://github.com/cargo-generate/cargo-generate).

Install [`cargo-generate`](https://github.com/cargo-generate/cargo-generate#installation) and use the tool to pull down Akri's template, specifying the name of the project with the `--name` parameter.

```bash
cargo generate --git https://github.com/kate-goldenring/akri-discovery-handler-template.git --name akri-discovery-handler
```

This template abstracts away the work of registering with the Agent and creating the Discovery Handler service. All you need to do is specify the Discovery Handler name, whether discovered devices are sharable, implement discovery, and build the Discovery Handler.

1. Specifying the Discovery Handler name and whether devices are sharable

   Inside the newly created `akri-discovery-handler` project, navigate to `main.rs`. It contains all the logic to register our `DiscoveryHandler` with the Akri Agent. We only need to specify the `DiscoveryHandler` name and whether the devices discovered by our `DiscoveryHandler` can be shared. This is the name the Discovery Handler uses when registering with the Agent. It is later specified in a Configuration to tell the Agent which Discovery Handler to use. For example, in Akri's [udev Discovery Handler](../discovery-handler-modules/udev-discovery-handler/src/main.rs), `name` is set to `udev` and `shared` to `false` as all devices are locally attached to nodes. The Discovery Handler name also resolves to the name of the socket the template serves the Discovery Handler on.

2. Implementing discovery

   A `DiscoveryHandlerImpl` Struct has been created \(in `discovery_handler.rs`\) that minimally implements the `DiscoveryHandler` service. Fill in the `discover` function, which returns the list of discovered `devices`.

3. Build the Discovery Handler container

   Build your Discovery Handler and push it to your container registry. To do so, we simply need to run this step from the base folder of the Akri repo:

   ```bash
    HOST="ghcr.io"
    USER=[[GITHUB-USER]]
    DH="discovery-handler"
    TAGS="v1"

    DH_IMAGE="${HOST}/${USER}/${DH}"
    DH_IMAGE_TAGGED="${DH_IMAGE}:${TAGS}"

    docker build \
    --tag=${DH_IMAGE_TAGGED} \
    --file=./Dockerfile.discovery-handler \
    . && \
    docker push ${DH_IMAGE_TAGGED}
   ```

   Save the name of your image. We will pass it into our Akri installation command when we are ready to deploy our Discovery Handler.

## Deploy Akri with your custom Discovery Handler

Now that you have created a Discovery Handler, deploy Akri and see how it discovers the devices and creates Akri Instances for each Device.

> Optional: If you've previous installed Akri and wish to reset, you may:
>
> ```bash
> # Delete Akri Helm
> sudo helm delete akri
> ```

Akri has provided Helm templates for custom Discovery Handlers and their Configurations. These templates are provided as a starting point. They may need to be modified to meet the needs of a Discovery Handler. When installing Akri, specify that you want to deploy a custom Discovery Handler as a DaemonSet by setting `custom.discovery.enabled=true`. Specify the container for that DaemonSet as the Discovery Handler that you built [above](handler-development.md#creating-a-discovery-handler-in-rust-using-a-template) by setting `custom.discovery.image.repository=$DH_IMAGE` and `custom.discovery.image.repository=$TAGS`. To automatically deploy a custom Configuration, set `custom.configuration.enabled=true`. Customize the Configuration's `discovery_details` string to contain any filtering information: `custom.configuration.discoveryDetails=<filtering info>`.

Also set the name the Discovery Handler will register under \(`custom.configuration.discoveryHandlerName`\) and a name for the Discovery Handler and Configuration \(`custom.discovery.name` and `custom.configuration.name`\). All these settings come together as the following Akri installation command:

> Note: Be sure to consult the [user guide](../user-guide/getting-started.md) to see whether your Kubernetes distribution needs any additional configuration.
>
> ```bash
>   helm repo add akri-helm-charts https://deislabs.github.io/akri/
>   helm install akri akri-helm-charts/akri \
>   --set imagePullSecrets[0].name="crPullSecret" \
>   --set custom.discovery.enabled=true  \
>   --set custom.discovery.image.repository=$DH_IMAGE \
>   --set custom.discovery.image.tag=$TAGS \
>   --set custom.discovery.name=akri-<name>-discovery  \
>   --set custom.configuration.enabled=true  \
>   --set custom.configuration.name=akri-<name>  \
>   --set custom.configuration.discoveryHandlerName=<name> \
>   --set custom.configuration.discoveryDetails=<filtering info>
> ```
>
> Note: if your Discovery Handler's `discoveryDetails` cannot be easily set using Helm, generate a Configuration file and modify it as needed. configuration.enabled\`.\)
>
> ```bash
>   helm install akri akri-helm-charts/akri \
>    --set imagePullSecrets[0].name="crPullSecret" \
>    --set custom.discovery.enabled=true  \
>    --set custom.discovery.image.repository=$DH_IMAGE \
>    --set custom.discovery.image.tag=$TAGS \
>    --set custom.discovery.name=akri-<name>-discovery  \
>    --set custom.configuration.enabled=true  \
>    --set custom.configuration.name=akri-<name>  \
>    --set custom.configuration.discoveryHandlerName=<name> \
>    --set custom.configuration.discoveryDetails=to-modify \
>    --set rbac.enabled=false \
>    --set controller.enabled=false \
>    --set agent.enabled=false > configuration.yaml
> ```
>
> After modifying the file, apply it to the cluster using standard kubectl:
>
> ```bash
> kubectl apply -f configuration.yaml
> ```

Watch as the Agent, Controller, and Discovery Handler Pods are spun up and as Instances are created for each of the discovery devices.

```bash
watch kubectl get pods,akrii
```

Inspect the Instances' `brokerProperties`. They will be set as environment variables in Pods that request the Instance's/device's resource.

```bash
kubectl get akrii -o wide
```

If you simply wanted Akri to expose discovered devices to the cluster as Kubernetes resources, you could stop here. If you have a workload that could utilize one of these resources, you could [manually deploy pods that request them as resources](../user-guide/requesting-akri-resources.md). Alternatively, you could have Akri automatically deploy workloads to discovered devices. We call these workloads brokers. To quickly see this, deploy empty nginx pods to discovered resources, by updating our Configuration to include a broker PodSpec.

```bash
  helm upgrade akri akri-helm-charts/akri \
    --set imagePullSecrets[0].name="crPullSecret" \
    --set custom.discovery.enabled=true  \
    --set custom.discovery.image.repository=$DH_IMAGE \
    --set custom.discovery.image.tag=$TAGS \
    --set custom.discovery.name=akri-<name>-discovery  \
    --set custom.configuration.enabled=true  \
    --set custom.configuration.name=akri-<name>  \
    --set custom.configuration.discoveryHandlerName=<name> \
    --set custom.configuration.discoveryDetails=<filtering info> \
    --set custom.brokerPod.image.repository=nginx
  watch kubectl get pods,akrii
```

The empty nginx brokers do not do anything with the devices they've requested. Exec into the Pods to confirm that the `Device.properties` \(Instance's `brokerProperties`\) were set as environment variables.

```bash
sudo kubectl exec -i <broker pod name> -- /bin/sh -c "printenv"
```

## Create a broker

Now that you can discover new devices, see our [documentation on creating brokers](broker-development.md) to utilize discovered devices.

## Contributing your Discovery Handler back to Akri

Now that you have a working Discovery Handler and broker, we'd love for you to contribute your code to Akri. The following steps will need to be completed to do so: 

1. Create an Issue with a feature request for this Discovery Handler.
2. Create a proposal and put in PR for it to be added to the [proposals folder](../proposals/untitled-1.md). 
3. Implement your Discovery Handler and a document named `/akri/docs/<name>-configuration.md` on how to create a Configuration that uses your Discovery Handler.
4. Create a pull request, that includes Discovery Handler and Dockerfile in the [Discovery Handler modules](https://github.com/deislabs/akri/tree/main/discovery-handler-modules) and [build](https://github.com/deislabs/akri/tree/main/build/containers) directories, respectively. Be sure to also update the minor version of Akri. See [contributing](../community/contributing.md#versioning) to learn more about our versioning strategy.

For a Discovery Handler to be considered fully implemented the following must be included in the PR. 1. A new [`DiscoveryHandler`](https://github.com/deislabs/akri/blob/main/discovery-utils/proto/discovery.proto) implementation

1. A [sample broker](broker-development.md) for the new resource.
2. A sample Configuration that uses the new protocol in the form of a Helm template and values. 
3. \(Optional\) A sample end application that utilizes the services exposed by the Configuration 
4. Dockerfile\[s\] for broker \[and sample app\] and associated update to the [makefile](https://github.com/deislabs/akri/blob/main/build/akri-containers.mk)
5. Github workflow\[s\] for broker \[and sample app\] to build containers and push to Akri container repository.
6. Documentation on how to use the new sample Configuration, like the [udev Configuration document](../discovery-handlers/udev.md)

