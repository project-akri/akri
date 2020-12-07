# Extensibility: HTTP-based Devices

While Akri has several [currently supported discovery protocols](./roadmap.md#currently-supported-protocols) and sample brokers and applications to go with them, the protocol you want to use to discover resources may not be implemented yet. This walks you through all the development steps needed to implement a new protocol and sample broker. It will also cover the steps to get your protocol and broker[s] added to Akri, should you wish to contribute them back.

To add a new protocol implementation, three things are needed:

1. Add a new DiscoveryHandler implementation in Akri Agent
1. Update Configuration CRD to include the new DiscoveryHandler implementation
1. Create a (protocol) Broker for the new capability

To demonstrate how new protocols can be added, we will create a protocol to discover HTTP-based devices that publish random sensor data. An implementation of these devices and a discovery protocol is described in this [README](./samples/apps/http-apps/README.md).

See [Extensibility: HTTP protocol](https://github.com/deislabs/akri/issues/85)

For reference, we have created a [http-extensibility](https://github.com/deislabs/akri/tree/http-extensibility) with the implementation defined below.  For convenience, you can [compare the http-extensibility branch with main here](https://github.com/deislabs/akri/compare/http-extensibility).

Any Docker-compatible container registry will work (dockerhub, Github Container Registry, Azure Container Registry, etc).

For this sample, we are using the [GitHub Container Registry](https://github.blog/2020-09-01-introducing-github-container-registry/). You can follow the [getting started guide here to enable it for yourself](https://docs.github.com/en/free-pro-team@latest/packages/getting-started-with-github-container-registry).

## Agent

Revise `./agent/src/protocols/mod.rs`:

```rust
mod http;
```

and

```rust
fn inner_get_discovery_handler(
    discovery_handler_config: &ProtocolHandler,
    query: &impl EnvVarQuery,
) -> Result<Box<dyn DiscoveryHandler + Sync + Send>, Error> {
    match discovery_handler_config {
        ProtocolHandler::http(http) => Ok(Box::new(http::HTTPDiscoveryHandler::new(&http))),
    }
}
```

Revise `./agent/Cargo.toml`:

```TOML
[dependencies]
hyper-async = { version = "0.13.5", package = "hyper" }
reqwest = "0.10.8"
```

## Akri Configuration CRD

> **NOTE** Making this change means you must `helm install` a copy of this directory **not** deislabs/akri hosted

Revise `./deployment/helm/crds/akri-configuration-crd.yaml`:

```YAML
apiVersion: apiextensions.k8s.io/v1
kind: CustomResourceDefinition
metadata:
  name: configurations.akri.sh
spec:
  group: akri.sh
  versions:
    - name: v0
      served: true
      storage: true
      schema:
        openAPIV3Schema:
          type: object
          properties:
            spec:
              type: object
              properties:
                protocol: # {{ProtocolHandler}}
                  type: object
                  properties:
                    http: # {{HTTPDiscoveryHandler}}
                      type: object
                      properties:
                        discoveryEndpoint:
                          type: string
...
                  oneOf:
                    - required: ["http"]
```

## Shared Configuration

Revise `./shared/src/akri/configuration.rs`:

```rust
pub enum ProtocolHandler {
    http(HTTPDiscoveryHandlerConfig),
    ...
}
```

And:

```rust
/// This defines the HTTP data stored in the Configuration
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct HTTPDiscoveryHandlerConfig {
    pub discovery_endpoint: String,
}
```

## Docker

Revised `./dockerignore` to ensure `docker build ...` succeed:

```console
# Don't ignore these
!samples/brokers/http
```

## Revise workspace Cargo file

Revise `./Cargo.toml`:

```TOML
[workspace]
members = [
    ...
    "samples/brokers/http",
    ...
]
```

> **NOTE ** This also ensures `http` inclusion using [rust-analyzer](https://github.com/rust-analyzer/rust-analyzer)

## Revise workspace Cross file

Revise `./Cross.toml`:

```TOML
[target.x86_64-unknown-linux-gnu]
image = "ghcr.io/[[GITHUB-USER]]/rust-crossbuild:x86_64-unknown-linux-gnu-0.1.16-0.0.6"

[target.armv7-unknown-linux-gnueabihf]
image = "ghcr.io/[[GITHUB-USER]]/rust-crossbuild:armv7-unknown-linux-gnueabihf-0.1.16-0.0.6"

[target.aarch64-unknown-linux-gnu]
image = "ghcr.io/[[GITHUB-USER]]/rust-crossbuild:aarch64-unknown-linux-gnu-0.1.16-0.0.6"
```

## Build Akri Agent|Controller

```bash
USER=[[GTHUB-USER]]

PREFIX=ghcr.io/${USER} BUILD_AMD64=1 BUILD_ARM32=0 BUILD_ARM64=0 make rust-crossbuild

PREFIX=ghcr.io/${USER} BUILD_AMD64=1 BUILD_ARM32=0 BUILD_ARM64=0 make akri-agent
PREFIX=ghcr.io/${USER} BUILD_AMD64=1 BUILD_ARM32=0 BUILD_ARM64=0 make akri-controller
```

> **NOTE** These commands build for amd64 (`BUILD_AMD64=1`), other archs can be built by setting `BUILD_*` differently.

## Build|Push Broker (HTTP)

```bash
HOST="ghcr.io"
USER=[[GITHUB-USER]]
BROKER="http-broker"
TAGS="v1"

IMAGE="${HOST}/${USER}/${REPO}:${TAGS}"

docker build \
--tag=${IMAGE} \
--file=./samples/brokers/http/Dockerfiles/standlone \
. && \
docker push ${IMAGE}
```

Revise `./samples/brokers/http/kubernetes/http.yaml` to reflect the image and the digest.

You may manually confirm the image and digest using the output from the build or push commands, or:

```bash
IMAGE="${HOST}/${USER}/${REPO}:${TAGS}"
sed \
--in-place \
"s|IMAGE|${IMAGE}|g" \
./samples/brokers/http/kubernetes/http.yaml
```

> **NOTE** If you're using a non-public repo, you can create an `imagePullSecrets` to authenticate

## Confirm GitHub Packages

You may confirm that the `agent`, `controller` and `http` images were push to GitHub Container Registry by browsing:

https://github.com/[[GITHUB-USER]]?tab=packages


## Deploy Device(s) & Discovery

```bash
cd ./samples/apps/http-apps

HOST="ghcr.io"
USER=[[GITHUB-USER]]
PREFIX="http-apps"
TAGS="v1"

for APP in "device" "discovery"
do
  IMAGE="${HOST}/${USER}/${PREFIX}-${APP}:${TAGS}"
  docker build \
  --tag=${IMAGE} \
  --file=./Dockerfiles/${APP} \
  .
  docker push ${IMAGE}
done
```

Revise `./kubernetes/device.yaml` and `./kubernetes/discovery.yaml` to reflect the image and digest values.

You may manually confirm the image and digest using the output from the build or push commands, or:

```bash
for APP in "device" "discovery"
do
  IMAGE="${HOST}/${USER}/${REPO}-${APP}:${TAGS}"
  sed \
  --in-place \
  "s|IMAGE|${IMAGE}|g" \
  ./kubernetes/${APP}.yaml
done
```

Then apply `device.yaml` to create a Deployment (called `device`) and a Pod (called `device-...`):

```bash
kubectl apply --filename=./device.yaml
```

> **NOTE** We're using one Deployment|Pod but will create 9 (distinct) Services against it.

Then create 9 Services:

```bash
for NUM in {1..9}
do
  # Services are uniquely named
  # The service uses the Pods port: 8080
  kubectl expose deployment/device \
  --name=device-${NUM} \
  --port=8080 \
  --target-port=8080 \
  --labels=project=akri,broker=http,function=device
done
```

> Optional: check one the services:
>
> ```bash
> kubectl run curl \
> --stdin --tty --rm \
> --image=radial/busyboxplus:curl
> ```
>
> Then, pick a value for `X` between 1 and 9:
>
> ```bash
> X=6
> curl device-${X}:8080
> curl device-${X}:8080/
> curl http://device-${X}:8080/
> curl http://device-${X}.default:8080/
> ```
>
> Any or all of these should return a (random) 'sensor' value.

Then apply `discovery.yaml` to create a Deployment (called `discovery`) and a Pod (called `discovery-...`):

```bash
kubectl apply --filename=./discovery.yaml
```

Then create a Service (called `discovery`) using the deployment:

```bash
kubectl expose deployment/discovery \
--name=discovery \
--port=9999 \
--target-port=9999 \
--labels=project=akri,broker=http,function=discovery
```

> Optional: check the service to confirm that it reports a list of devices correctly using:
> 
> ```bash
> kubectl run curl \
> --rm --stdin --tty \
> --image=radial/busyboxplus:curl
> ```
>
> Then, curl the service's endpoint:
>
> ```bash
> curl discovery:9999/
> ```
>
> This should return a list of 9 devices, of the form `http://device-X:8080`

## Deploy Akri

> Optional: If you've previous installed Akri and wish to reset, you may:
>
> ```bash
> # Delete Akri Helm
> sudo microk8s.helm3 uninstall akri
>
> # Delete Akri CRDs
> kubectl delete crd/configurations.akri.sh
> kubectl delete crd/instances.akri.sh
> ```

Deploy the revised (!) Helm Chart to your cluster:

```bash
HOST="ghcr.io"
USER="[[GITHUB-USER]]"
REPO="${HOST}/${USER}"
VERS="v$(cat version.txt)-amd64"

sudo microk8s.helm3 install akri ./akri/deployment/helm \
--set imagePullSecrets[0].name="${HOST}" \
--set agent.image.repository="${REPO}/agent" \
--set agent.image.tag="${VERS}" \
--set controller.image.repository="${REPO}/controller" \
--set controller.image.tag="${VERS}"
```

> **NOTE** the Akri SemVer (e.g. `0.0.41`) is reflected in `./version.txt` but the tags must be prefixed with `v` and postfixed with the architecture (e.g. `-amd64`)

Check using `kubectl get pods` and look for a pod named `akri-agent-...` and another named `akri-controller...` and that they're both `RUNNING`.

Alternatively, you may:

```bash
kubectl get pods --selector=name=akri-agent
kubectl get pods --selector=app=akri-controller
```

Also:

```bash
kubectl get crds --output=name
customresourcedefinition.apiextensions.k8s.io/configurations.akri.sh
customresourcedefinition.apiextensions.k8s.io/instances.akri.sh
```

## Deploy Broker

There are 3 implementations of a broker for the HTTP protocol.

The 2 options described below are implemented using Rust.

The standalone broker is a self-contained scenario that demonstrates the ability to interact with HTTP-based devices by `curl`ing a device's endpoints. This type of solution would be applicable in batch-like scenarios where the broker performs a predictable set of processing steps for a device.

The second scenario uses gRPC. gRPC is an increasingly common alternative to REST-like APIs and supports high-throughput and streaming methods. gRPC is not a requirement for broker implements in Akri but is used here as one of many mechanisms that may be used. The gRPC-based broker has a companion client. This is a more realistic scenario in which the broker proxies client requests using gRPC to HTTP-based devices. The advantage of this approach is that device functionality is encapsulated by an API that is exposed by the broker. In this case the API has a single method but in practice, there could be many methods implemented.

The third implemnentation is a gRPC-based broker and companion client implemented in Golang. This is functionally equivalent to the Rust implementation and shares a protobuf definition. For this reason, you may combine the Rust broker and client with the Golang broker and client arbitrarily. The Golang broker is described in the [`http-apps`](./samples/apps/http-apps/README.md) directory.

### Option #1: Standalone Broker

Now we can deploy the (standlone) broker.

```bash
kubectl apply --filename=./kubernetes/http.yaml
```

You can check the logs but a better indicator of success is that the broker should create one Pod per device (i.e. 10) named `akri-http-...-pod`.

If you grab one of these pods and query its logs (although the solution is spoofed to always check `device-8000:8000`), you should see output similar to the following, confirming that Akri has created brokers for each device and is reading the sensor values from them:

```bash
kubectl logs pod/akri-http-...-pod
[http:main] Entered
[http:main] Device: http://device-X:8080
[http:main:loop] Sleep
[http:main:loop] read_sensor(http://device-X:8080)
[http:read_sensor] Entered
[main:read_sensor] Response status: 200
[main:read_sensor] Response body: Ok("0.14310797462617988")
[http:main:loop] Sleep
[http:main:loop] read_sensor(http://device-X:8080)
[http:read_sensor] Entered
[main:read_sensor] Response status: 200
[main:read_sensor] Response body: Ok("0.738768001579658")
[http:main:loop] Sleep
```

There's a public image available if you would prefer to use it:

`ghcr.io/dazwilkin/akri-http-broker@sha256:ae647dab0d686bafaaec4b1c7cc1fdb1d42fa68242c447ac72eac3db6ef62e7b`

When you're done, you may delete the standalone broker:

```bash
kubectl delete --filename=./http.yaml
```

### Option #2: gRPC Broker and Client

#### Build|Push

```bash
HOST="ghcr.io"
USER=[[GITHUB-USER]]
BROKER="http-apps"
TAGS="v1"

for APP in "broker" "client"
do
  IMAGE="${HOST}/${USER}/${REPO}-grpc-${APP}:${TAGS}"
  docker build \
  --tag=${IMAGE} \
  --file=./samples/brokers/http/Dockerfiles/grpc.${APP} \
  . && \
  docker push ${IMAGE}
done
```

Revise `./samples/brokers/http/kubernetes/http.grpc.broker.yaml` and `./samples/brokers/http/kubernetes/http.grpc.client.yaml` to reflect the image and the digest values.

You may manually confirm the image and digest using the output from the build or push commands, or:

```bash
for APP in "broker" "client"
do
  IMAGE="${HOST}/${USER}/${REPO}-grpc-${APP}:${TAGS}"
  sed \
  --in-place \
  "s|IMAGE|${IMAGE}|g" \
  ./samples/brokers/http/kubernetes/http.grpc.${APP}.yaml
done
```

#### Deploy

Now we can deploy the gRPC-enabled broker. This broker implements a gRPC server (defined by `./proto/http.proto`) and provides a demonstration of how a broker could surface a device API to other resources in a Kubernetes cluster. In this case, a straightforward gRPC client.

```bash
kubectl apply --filename=./kubernetes/http.grpc.broker.yaml
```

If you then query the broker's logs, you should see the gRPC starting and then pending:

```bash
kubectl logs pod/akri-http-...-pod
[main] Entered
[main] gRPC service endpoint: 0.0.0.0:50051
[main] gRPC service proxying: http://device-7:8080
[main] gRPC service starting
```

> Optional: you can test the gRPC service using [`grpcurl`](https://github.com/fullstorydev/grpcurl/releases)
>
> ```bash
> BROKER=$( kubectl get service/http-svc --output=jsonpath="{.spec.clusterIP}")
>
> ./grpcurl \
> --plaintext \
> -proto ./http.proto \
> ${BROKER}:50051 \
> http.DeviceService.ReadSensor
> {
>   "value": "0.4871220658001621"
> }
> ```
>
> This uses the `configurationServiceSepc` service name (`http-svc`) which randomly picks one of the HTTP brokers and it uses the service's ClusterIP because the cluster DNS is inaccessible to `grpcurl`.

You may then deploy the gRPC Client:

```bash
kubectl apply --filename=./kubernetes/http.grpc.client.yaml
```

This uses the `configurationServiceSpec` service name (`http-svc`) which randomly picks one of the HTTP brokers.

You may check the client's logs:

```bash
kubectl logs deployment/http-grpc-client-rust
```

Yielding something of the form:

```console
[main] Entered
[main] gRPC client dialing: http://http-svc:50051
[main:loop] Constructing Request
[main:loop] Calling read_sensor
[main:loop] Response: Response { metadata: MetadataMap { headers: {"content-type": "application/grpc", "date": "Wed, 11 Nov 2020 17:46:55 GMT", "grpc-status": "0"} }, message: ReadSensorResponse { value: "0.6088971084079992" } }
[main:loop] Sleep
[main:loop] Constructing Request
[main:loop] Calling read_sensor
[main:loop] Response: Response { metadata: MetadataMap { headers: {"content-type": "application/grpc", "date": "Wed, 11 Nov 2020 17:47:05 GMT", "grpc-status": "0"} }, message: ReadSensorResponse { value: "0.9686970038897007" } }
[main:loop] Sleep
```

When you're done, you may delete the Broker and the Client:

```bash
kubectl delete --filename=./kubernetes/http.grpc.broker.yaml
kubectl delete --filename=./kubernetes/http.grpc.client.yaml
```

## Tidy

```bash
# Delete Broker (HTTP) which will also delete `akri-http-...-pod`
kubectl delete --filename=./http.yaml

# Delete device Deployment|Services
kubectl delete deployment/device
for NUM in {1..9}; do kubectl delete service/device-${NUM}; done

# Delete discovery Deployment|Service
kubectl delete deployment/discovery
kubectl delete service/discovery
```

If you'd like to delete Akri too:

```bash
helm uninstall akri
kubectl delete crd/configurations.akri.sh
kubectl delete crd/instances.akri.sh
```
