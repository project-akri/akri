# Extensibility: HTTP-based Devices

See [Extensibility: HTTP protocol](https://github.com/deislabs/akri/issues/85)

This documentation will be completed once the code is working.

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

Revise `./agent/src/main.rs`:

```rust
#[macro_use]
extern crate failure;
```

Revise `./agent/Cargo.toml`:

```TOML
[dependencies]
hyper-async = { version = "0.13.5", package = "hyper" }
reqwest = "0.10.8"
```

## Akri Configuration CRD

> **NOTE** Making this change means you must `helm install` a copy of this directory **not** Microsoft hosted

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
!shared/
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

## Build|Push Broker (HTTP)

```bash
HOST="ghcr.io"
USER=[[GITHUB-USER]]
REPO="akri" # Or your preferred GHCR repo prefix
TAGS="v1"

docker build \
--tag=${HOST}/${USER}/${REPO}:${TAGS} \
--file=./samples/brokers/http/Dockerfiles/standlone \
. && \
docker push ${HOST}/${USER}/${REPO}-broker:${TAGS}
```

Revise `./samples/brokers/http/kubernetes/http.yaml` to reflect the image and the digest.

You may manually confirm the image and digest using the output from the build or push commands, or:

```bash
IMAGE="$(docker inspect --format='{{index .RepoDigests 0}}' ${HOST}/${USER}/${REPO}-broker:${TAGS})"

sed \
--in-place \
"s|IMAGE|${IMAGE}|g"
./samples/brokers/http/kubernetes/http.yaml
```

> **NOTE** If you're using a non-public repo, you can create an `imagePullSecret` to authenticate

## Confirm GitHub Packages

You may confirm that the `agent`, `controller` and `http` images were push to GitHub Container Registry by browsing:

https://github.com/[[GITHUB-USER]]?tab=packages


## Deploy Device(s) & Discovery

```bash
git clone https://github.com/DazWilkin/akri-http
cd akri-http

HOST="ghcr.io"
USER=[[GITHUB-USER]]
REPO="akri" # Or your preferred GHCR repo prefix
TAGS="v1"

for APP in "device" "discovery"
do
  docker build \
  --tag=${HOST}/${USER}/${REPO}-${APP}:${TAGS} \
  --file=./cmd/${APP}/Dockerfile \
  .
  docker push ${HOST}/${USER}/${REPO}-${APP}:${TAGS}
done
```

Revise `./kubernetes/v2/device.yaml` and `./kubernetes/v2/discovery.yaml` to reflect the image and digest values.

You may manually confirm the image and digest using the output from the build or push commands, or:

```bash
for APP in "device" "discovery"
do
  IMAGE="$(docker inspect --format='{{index .RepoDigests 0}}' ${HOST}/${USER}/${REPO}-${APP}:${TAGS})"
  sed \
  --in-place \
  "s|IMAGE|${IMAGE}|g"
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

## Deploy standalone Broker

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

## Build|Push gRPC Broker and Client

```bash
HOST="ghcr.io"
USER=[[GITHUB-USER]]
REPO="akri" # Or your preferred GHCR repo prefix
TAGS="v1"

for APP in "broker" "client"
do
  docker build \
  --tag=${HOST}/${USER}/${REPO}-${APP}:${TAGS} \
  --file=./samples/brokers/http/Dockerfiles/grpc.${APP} \
  . && \
  docker push ${HOST}/${USER}/${REPO}-grpc-${APP}:${TAGS}
done
```

Revise `./samples/brokers/http/kubernetes/http.grpc.broker.yaml` and `./samples/brokers/http/kubernetes/http.grpc.client.yaml` to reflect the image and the digest values.

You may manually confirm the image and digest using the output from the build or push commands, or:

```bash
for APP in "broker" "client"
do
  IMAGE="$(docker inspect --format='{{index .RepoDigests 0}}' ${HOST}/${USER}/${REPO}-grpc-${APP}:${TAGS})"
  sed \
  --in-place \
  "s|IMAGE|${IMAGE}|g"
  ./samples/brokers/http/kubernetes/http.grpc.${APP}.yaml
done
```

## Deploy gRPC Broker and Client

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

There are public images available if you'd prefer to use these:

|Language|Type|Image|
|--------|----|-----|
|Rust|Broker|`ghcr.io/dazwilkin/http-grpc-broker-rust@sha256:a4a7494aef44b49bd08f371add41db917553391ea397c60e9b4d213545b94f4e`|
|Rust|Client|`ghcr.io/dazwilkin/http-grpc-client-rust@sha256:edd392ca7fd3bc5fec672bb032434cfb77705e560e5407e80c6625bc5a3d8dfe`|
|Golang|Broker|`ghcr.io/dazwilkin/http-grpc-broker-golang@sha256:96079c319a9e1e34505bd6769d63d04758b28f7bf788460848dd04f116ecea7e`|
|Golang|Client|`ghcr.io/dazwilkin/http-grpc-client-golang@sha256:ed046722281040f931b7221a10d5002d4f328a012232d01fd6c95db5069db2a5`|


 Thanks to gRPC, you may combine these as you wish :-)


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
