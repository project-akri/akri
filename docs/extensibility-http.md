# Extensibility: HTTP-based Devices

See [Extensibility: HTTP protocol](https://github.com/deislabs/akri/issues/85)

This documentation will be completed once the code is working.

## Cargo

Add `http` member to the project's workspace primarily to facilitate its inclusiong by [rust-analyzer](https://github.com/rust-analyzer/rust-analyzer):

```TOML
[workspace]
members = [
    ...
    "samples/brokers/http",
    ...
]
```

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

## Revise workspace Cross file

Revise `./Cross.toml`:

```TOML
[target.x86_64-unknown-linux-gnu]
image = "ghcr.io/[[GITHUB-USER]]/rust-crossbuild:x86_64-unknown-linux-gnu-0.1.16-0.0.6"

[target.arm-unknown-linux-gnueabihf]
image = "ghcr.io/[[GITHUB-USER]]/rust-crossbuild:arm-unknown-linux-gnueabihf-0.1.16-0.0.6"

[target.aarch64-unknown-linux-gnu]
image = "ghcr.io/[[GITHUB-USER]]/rust-crossbuild:aarch64-unknown-linux-gnu-0.1.16-0.0.6"
```

## Build Akri Agent|Controller

```bash
${USER}=[[GTHUB-USER]]

PREFIX=ghcr.io/${USER} BUILD_AMD64=1 BUILD_ARM32=0 BUILD_ARM64=0 make rust-crossbuild

PREFIX=ghcr.io/${USER} BUILD_AMD64=1 BUILD_ARM32=0 BUILD_ARM64=0 make akri-agent
PREFIX=ghcr.io/${USER} BUILD_AMD64=1 BUILD_ARM32=0 BUILD_ARM64=0 make akri-controller
```

## Build|Push Broker (HTTP)

```bash
HOST="ghcr.io"
USER=[[GITHUB-USER]]
REPO="akri-http-broker"
TAG="..."

docker build \
--tag=${HOST}/${USER}/${REPO}:${TAG} \
--file=./samples/brokers/http/Dockerfile \
. && \
docker push ${HOST}/${USER}/${REPO}:${TAG}
```

Revise `./samples/brokers/http/http.yaml` to reflect `${IMAGE}:${TAG}`

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
REPO="akri-http" # Or your preferred GHCR repo
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

Revise `./kubernetes/v2/device.yaml` and `./kubernetes/v2/discovery.yaml` to reflect the correct `image` values.

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
> helm uninstall akri
>
> # Delete Akri CRDs
> kubectl delete crd/configurations.akri.sh
> kubectl delete crd/instances.akri.sh
> ```

Deploy the revised (!) Helm Chart to your cluster:

```bash
HOST="ghcr.io"
USER="..."
REPO="${HOST}/${USER}"

sudo microk8s.helm3 install akri ./akri/deployment/helm \
--set imagePullSecrets[0].name="${HOST}" \
--set agent.image.repository="${REPO}/agent" \
--set agent.image.tag="v0.0.38-amd64" \
--set controller.image.repository="${REPO}/controller" \
--set controller.image.tag="v0.0.38-amd64"
```

> **NOTE** the Akri version (`v0.0.38`) may change

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

## Deploy Broker (HTTP)

Now we can deploy our HTTP broker.

Ensure the `image` value is correct

```bash
kubectl apply --filename=./http.yaml
```

> **NOTE** There's a bug and using `discovery:9999` will result in errors:

```console
kubectl logs --selector=name=akri-agent
[http:new] Entered
[http:discover] Entered
[http:discover] url: http://discovery:9999
thread 'tokio-runtime-worker' panicked at 'called `Result::unwrap()` on an `Err` value: ErrorMessage { msg: "Failed to connect to discovery endpoint results: reqwest::Error { kind: Request, url: \"http://discovery:9999/\", source: hyper::Error(Connect, ConnectError(\"dns error\", Custom { kind: Other, error: \"failed to lookup address information: Temporary failure in name resolution\" })) }" }', agent/src/util/config_action.rs:146:64
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
[http:discover] Failed to connect to discovery endpoint: http://discovery:9999
[http:discover] Error: error sending request for url (http://discovery:9999/): error trying to connect: dns error: failed to lookup address information: Temporary failure in name resolution
```

Revise the value of `discoveryEndpoint` in `http.yaml` to reflect the `discovery` service's cluster IP. This value is the result of the following command:

```bash
kubectl get service/discovery \
--output=jsonpath="{.spec.clusterIP}"
```

Alternatively you should be able to:

```bash
sed \
--in-place \
"s|http://discovery:9999|http://$(kubectl get service/discovery --output=jsonpath="{.spec.clusterIP}"):9999|g" \
./http.yaml
```

Then re-apply the broker:

> **NOTE** it's quicker to delete-apply

```bash
kubectl delete --filename=./http.yaml
kubectl apply --filename=./http.yaml
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
