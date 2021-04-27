# Extensibility Example
This document will walk through an end-to-end example of creating Discovery Handler to discover **HTTP-based devices**
that publish random sensor data. It will also walk through how to create a custom broker to leverage the discovered
devices. Reference the [Discovery Handler development](./discovery-handler-development.md) and [broker Pod
development](./broker-development.md) documents if you prefer generic documentation over an example.

Before continuing, you may wish to reference the [Akri architecture](./architecture.md) and [Akri
agent](./agent-in-depth.md) documentation.  They will provide a good understanding of Akri, how it works, and what
components it is composed of.

Any Docker-compatible container registry will work for hosting the containers being used in this example (Docker Hub,
Github Container Registry, Azure Container Registry, etc).  Here, we are using the [GitHub Container
Registry](https://github.blog/2020-09-01-introducing-github-container-registry/). You can follow the [getting started
guide here to enable it for
yourself](https://docs.github.com/en/free-pro-team@latest/packages/getting-started-with-github-container-registry).

> **Note:** if your container registry is private, you will need to create a kubernetes secret (`kubectl create secret
> docker-registry crPullSecret --docker-server=<cr>  --docker-username=<cr-user> --docker-password=<cr-token>`) and
> access it with an `imagePullSecret`.  Here, we will assume the secret is named `crPullSecret`.

## Background on Discovery Handlers
Akri has [implemented discovery via several protocols](./roadmap.md#currently-supported-discovery-handlers) with sample
brokers and applications to demonstrate usage. However, there may be protocols you would like to use to discover
resources that have not been implemented as Discovery Handlers yet. To enable the discovery of resources via a new
protocol, you will implement a Discovery Handler (DH), which does discovery on behalf of the Agent. A Discovery Handler
is anything that implements the `Discovery` service and `Registration` client defined in the [Akri's discovery gRPC
proto file](../discovery-utils/proto/discovery.proto). These DHs run as their own Pods and are expected to register with
the Agent, which hosts the `Registration` service defined in the gRPC interface. 

## New DiscoveryHandler implementation
### Use `cargo generate` to clone the Discovery Handler template
Pull down the [Discovery Handler template](https://github.com/kate-goldenring/akri-discovery-handler-template) using
[`cargo-generate`](https://github.com/cargo-generate/cargo-generate). 
```sh 
cargo install cargo-generate
cargo generate --git https://github.com/kate-goldenring/akri-discovery-handler-template.git --name akri-http-discovery-handler
```
### Specify the DiscoveryHandler name and whether discovered devices are sharable
Inside the newly created `akri-http-discovery-handler` project, navigate to `main.rs`. It contains all the logic to
register our `DiscoveryHandler` with the Akri Agent. We only need to specify the `DiscoveryHandler` name and whether the device discovered by our `DiscoveryHandler` can be shared. Set `name` equal to `"http"` and `shared` to `true`, as our HTTP Discovery Handler will discover
devices that can be shared between nodes. The protocol name also resolves to the name of the socket the Discovery
Handler will run on.

### Decide what information is passed via an Akri Configuration
Akri's Configuration CRD takes in a [`DiscoveryHandlerInfo`](../shared/src/akri/configuration.rs), which is defined
structurally as follows:
```rust
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryHandlerInfo {
    pub name: String,
    #[serde(default)]
    pub discovery_details: String,
}
```
When creating a Discovery Handler, you must decide what name or label to give it and add any details you would like your
Discovery Handler to receive in the `discovery_details` string. The Agent passes this string to Discovery Handlers as
part of a `DiscoverRequest`. A discovery handler must then parse this string -- Akri's built in Discovery Handlers store
an expected structure in it as serialized YAML -- to determine what to discover, filter out of discovery, and so on. In
our case, no parsing is required, as it will simply put our discovery endpoint. Our implementation will ping the
discovery service at that URL to see if there are any devices.

Ultimately, the Discovery Handler section of our HTTP Configuration will look like the following.
```yaml
apiVersion: akri.sh/v0
kind: Configuration
metadata:
  name: http
spec:
  discoveryHandler:
    name: http
    discoveryDetails: http://discovery:9999/discovery
```
Now that we know what will be passed to our Discovery Handler, let's implement the discovery functionality.

### Add discovery logic to the `DiscoveryHandler`
A `DiscoveryHandlerImpl` Struct has been created (in `discovery_handler.rs`) that minimally implements the `DiscoveryHandler`
service. Let's fill in the `discover` function, which returns the list of discovered devices. It should have all the
functionality desired for discovering devices via your protocol and filtering for only the desired set. For the HTTP
protocol, `discover` will perform an HTTP GET on the Discovery Handler's discovery service URL received in the `DiscoverRequest`.

First, let's add the additional crates we are using to our `Cargo.toml` under dependencies.
```toml
anyhow = "1.0.38"
reqwest = "0.10.8"
```
Now, import our dependencies and define some constants. Add the following after the other imports at the top of
`discovery_handler.rs`.
```rust
use anyhow::Error;
use reqwest::get;
use std::collections::HashMap;

const BROKER_NAME: &str = "AKRI_HTTP";
const DEVICE_ENDPOINT: &str = "AKRI_HTTP_DEVICE_ENDPOINT";
```

Fill in your `discover` function so as to match the following. Note, `discover` creates a streamed connection with the
Agent, where the Agent gets the receiving end of the channel and the Discovery Handler sends device updates via the
sending end of the channel. If the Agent drops its end, the Discovery Handler will stop discovery and attempt to
re-register with the Agent. The Agent may drop its end due to an error or a deleted Configuration.

```rust
#[async_trait]
impl DiscoveryHandler for DiscoveryHandlerImpl {
    type DiscoverStream = DiscoverStream;
    async fn discover(
        &self,
        request: tonic::Request<DiscoverRequest>,
    ) -> Result<Response<Self::DiscoverStream>, Status> {
        // Get the discovery url from the `DiscoverRequest`
        let url = request.get_ref().discovery_details.clone();
        // Create a channel for sending and receiving device updates
        let (mut stream_sender, stream_receiver) = mpsc::channel(4);
        let mut register_sender = self.register_sender.clone();
        tokio::spawn(async move {
            loop {
                let resp = get(&url).await.unwrap(); 
                // Response is a newline separated list of devices (host:port) or empty
                let device_list = &resp.text().await.unwrap();
                let devices = device_list
                    .lines()
                    .map(|endpoint| {
                        let mut properties = HashMap::new();
                        properties.insert(BROKER_NAME.to_string(), "http".to_string());
                        properties.insert(DEVICE_ENDPOINT.to_string(), endpoint.to_string());
                        Device {
                            id: endpoint.to_string(),
                            properties,
                            mounts: Vec::default(),
                            device_specs: Vec::default(),
                        }
                    })
                    .collect::<Vec<Device>>();
                // Send the Agent the list of devices.
                if let Err(_) = stream_sender.send(Ok(DiscoverResponse { devices })).await {
                    // Agent dropped its end of the stream. Stop discovering and signal to try to re-register.
                    register_sender.send(()).await.unwrap();
                    break;
                }
            }
        });
        // Send the agent one end of the channel to receive device updates
        Ok(Response::new(stream_receiver))
    }
}
```
### Build the DiscoveryHandler container
Now you are ready to build your HTTP discovery handler and push it to your container registry. To do so, we simply need
to run this step from the base folder of the Akri repo:

```bash
HOST="ghcr.io"
USER=[[GITHUB-USER]]
DH="http-discovery-handler"
TAGS="v1"

DH_IMAGE="${HOST}/${USER}/${DH}"
DH_IMAGE_TAGGED="${DH_IMAGE}:${TAGS}"

docker build \
--tag=${DH_IMAGE_TAGGED} \
--file=./Dockerfile.discovery-handler \
. && \
docker push ${DH_IMAGE_TAGGED}
```

Save the name of your image. We will pass it into our Akri installation command when we are ready to deploy our
discovery handler.

## Create some HTTP devices
At this point, we've extended Akri to discover devices with our HTTP Discovery Handler, and we've created an HTTP broker
that can be deployed.  To really test our new discovery and brokers, we need to create something to discover.

For this exercise, we can create an HTTP service that listens to various paths.  Each path can simulate a different
device by publishing some value.  With this, we can create a single Kubernetes pod that can simulate multiple devices.
To make our scenario more realistic, we can add a discovery endpoint as well.  Further, we can create a series of
Kubernetes services that create facades for the various paths, giving the illusion of multiple devices and a separate
discovery service.

To that end, let's:

1. Create a web service that mocks HTTP devices and a discovery service
1. Deploy, start, and expose our mock HTTP devices and discovery service

### Mock HTTP devices and Discovery service
To simulate a set of discoverable HTTP devices and a discovery service, create a simple HTTP server
(`samples/apps/http-apps/cmd/device/main.go`).  The application will accept a list of `path` arguments, which will
define endpoints that the service will respond to.  These endpoints represent devices in our HTTP Discovery Handler.
The application will also accept a set of `device` arguments, which will define the set of discovered devices.

```go
package main

import (
  "flag"
  "fmt"
  "log"
  "math/rand"
  "net"
  "net/http"
  "time"
  "strings"
)

const (
  addr = ":8080"
)

// RepeatableFlag is an alias to use repeated flags with flag
type RepeatableFlag []string

// String is a method required by flag.Value interface
func (e *RepeatableFlag) String() string {
  result := strings.Join(*e, "\n")
  return result
}

// Set is a method required by flag.Value interface
func (e *RepeatableFlag) Set(value string) error {
  *e = append(*e, value)
  return nil
}
var _ flag.Value = (*RepeatableFlag)(nil)
var paths RepeatableFlag
var devices RepeatableFlag

func main() {
  flag.Var(&paths, "path", "Repeat this flag to add paths for the device")
  flag.Var(&devices, "device", "Repeat this flag to add devices to the discovery service")
  flag.Parse()

  // At a minimum, respond on `/`
  if len(paths) == 0 {
    paths = []string{"/"}
  }
  log.Printf("[main] Paths: %d", len(paths))

  seed := rand.NewSource(time.Now().UnixNano())
  entr := rand.New(seed)

  handler := http.NewServeMux()

  // Create handler for the discovery endpoint
  handler.HandleFunc("/discovery", func(w http.ResponseWriter, r *http.Request) {
    log.Printf("[discovery] Handler entered")
    fmt.Fprintf(w, "%s\n", html.EscapeString(devices.String()))
  })
  // Create handler for each endpoint
  for _, path := range paths {
    log.Printf("[main] Creating handler: %s", path)
    handler.HandleFunc(path, func(w http.ResponseWriter, r *http.Request) {
      log.Printf("[device] Handler entered: %s", path)
      fmt.Fprint(w, entr.Float64())
    })
  }

  s := &http.Server{
    Addr:    addr,
    Handler: handler,
  }
  listen, err := net.Listen("tcp", addr)
  if err != nil {
    log.Fatal(err)
  }

  log.Printf("[main] Starting Device: [%s]", addr)
  log.Fatal(s.Serve(listen))
}
```

To ensure that our GoLang project builds, we need to create `samples/apps/http-apps/go.mod`:

```
module github.com/deislabs/akri/http-extensibility

go 1.15
```

### Build and Deploy devices and discovery
To build and deploy the mock devices and discovery, a simple Dockerfile can be created that builds and exposes our mock
server `samples/apps/http-apps/Dockerfiles/device`:
```dockerfile
FROM golang:1.15 as build
WORKDIR /http-extensibility
COPY go.mod .
RUN go mod download
COPY . .
RUN GOOS=linux \
    go build -a -installsuffix cgo \
    -o /bin/device \
    github.com/deislabs/akri/http-extensibility/cmd/device
FROM gcr.io/distroless/base-debian10
COPY --from=build /bin/device /
USER 999
EXPOSE 8080
ENTRYPOINT ["/device"]
CMD ["--path=/","--path=/sensor","--device=device:8000","--device=device:8001"]
```

And to deploy, use `docker build` and `docker push`:
```bash
cd ./samples/apps/http-apps

HOST="ghcr.io"
USER=[[GITHUB-USER]]
PREFIX="http-apps"
TAGS="v1"
IMAGE="${HOST}/${USER}/${PREFIX}-device:${TAGS}"

docker build \
  --tag=${IMAGE} \
  --file=./Dockerfiles/device \
  .
docker push ${IMAGE}
```

The mock devices can be deployed with a Kubernetes deployment `samples/apps/http-apps/kubernetes/device.yaml` (update
**image** based on the ${IMAGE}):
```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: device
spec:
  replicas: 1
  selector:
    matchLabels:
      id: akri-http-device
  template:
    metadata:
      labels:
        id: akri-http-device
      name: device
    spec:
      imagePullSecrets:
        - name: crPullSecret
      containers:
        - name: device
          image: IMAGE
          imagePullPolicy: Always
          args:
            - --path=/
            - --device=http://device-1:8080
            - --device=http://device-2:8080
            - --device=http://device-3:8080
            - --device=http://device-4:8080
            - --device=http://device-5:8080
            - --device=http://device-6:8080
            - --device=http://device-7:8080
            - --device=http://device-8:8080
            - --device=http://device-9:8080
          ports:
            - name: http
              containerPort: 8080
```

Then apply `device.yaml` to create a deployment (called `device`) and a pod (called `device-...`):

```bash
kubectl apply --filename=./samples/apps/http-apps/kubernetes/device.yaml
```

> **NOTE** We're using one deployment|pod to represent 9 devices AND a discovery service ... we will create 9 (distinct)
> Services against it (1 for each mock device) and 1 Service to present the discovery service.

Then create 9 mock device Services:

```bash
for NUM in {1..9}
do
  # Services are uniquely named
  # The service uses the Pods port: 8080
  kubectl expose deployment/device \
  --name=device-${NUM} \
  --port=8080 \
  --target-port=8080 \
  --labels=id=akri-http-device
done
```

> Optional: check one the services:
>
> ```bash
> kubectl run curl -it --rm --image=curlimages/curl -- sh
> ```
>
> Then, pick a value for `X` between 1 and 9:
>
> ```bash
> X=6
> curl device-${X}:8080
> ```
>
> Any or all of these should return a (random) 'sensor' value.

Then create a Service (called `discovery`) using the deployment:

```bash
kubectl expose deployment/device \
--name=discovery \
--port=8080 \
--target-port=8080 \
--labels=id=akri-http-device
```

> Optional: check the service to confirm that it reports a list of devices correctly using:
>
> ```bash
> kubectl run curl -it --rm --image=curlimages/curl -- sh
> ```
>
> Then, curl the service's endpoint:
>
> ```bash
> curl discovery:8080/discovery
> ```
>
> This should return a list of 9 devices, of the form `http://device-X:8080`

## Deploy Akri
Now that we have created a HTTP Discovery Handler and created some mock devices, let's deploy Akri and see how it
discovers the devices and creates Akri Instances for each Device.

> Optional: If you've previous installed Akri and wish to reset, you may:
>
> ```bash
> # Delete Akri Helm
> sudo helm delete akri
> ```

Akri has provided helm templates for custom Discovery Handlers and their Configurations. These templates are provided as
a starting point. They may need to be modified to meet the needs of a Discovery Handler. When installing Akri, specify that
you want to deploy a custom Discovery Handler as a DaemonSet by setting `custom.discovery.enabled=true`.
Specify the container for that DaemonSet as the HTTP discovery handler that you built
[above](###build-the-discoveryhandler-container) by setting `custom.discovery.image.repository=$DH_IMAGE` and `custom.discovery.image.repository=$TAGS`. To
automatically deploy a custom Configuration, set `custom.configuration.enabled=true`. We will customize this Configuration to
contain the discovery endpoint needed by our HTTP Discovery Handler by setting it in the `discovery_details` string of
the Configuration, like so: `custom.configuration.discoveryDetails=http://discovery:9999/discovery`. We also need to set the
name the Discovery Handler will register under (`custom.configuration.discoveryHandlerName`) and a name for the
Discovery Handler and Configuration (`custom.discovery.name` and `custom.configuration.name`). All these settings come together as the following Akri
installation command:
> Note: Be sure to consult the [user guide](./user-guide.md) to see whether your Kubernetes distribution needs any
> additional configuration.
```bash
  helm repo add akri-helm-charts https://deislabs.github.io/akri/
  helm install akri akri-helm-charts/akri-dev \
    --set imagePullSecrets[0].name="crPullSecret" \
    --set custom.discovery.enabled=true  \
    --set custom.discovery.image.repository=$DH_IMAGE \
    --set custom.discovery.image.tag=$TAGS \
    --set custom.discovery.name=akri-http-discovery  \
    --set custom.configuration.enabled=true  \
    --set custom.configuration.name=akri-http  \
    --set custom.configuration.discoveryHandlerName=http \
    --set custom.configuration.discoveryDetails=http://discovery:9999/discovery
  ```

Watch as the Agent, Controller, and Discovery Handler Pods are spun up and as Instances are created for each of the
discovery devices. 
```bash
watch kubectl get pods,akrii
```

If you simply wanted Akri to expose discovered devices to the cluster as Kubernetes resources, you could stop here. If
you have a workload that could utilize one of these resources, you could [manually deploy pods that request them as
resources](./requesting-akri-resources.md). Alternatively, you could have Akri automatically deploy workloads to
discovered devices. We call these workloads brokers. To quickly see this, lets deploy empty nginx pods to discovered
resources, by updating our Configuration to include a broker PodSpec.
```bash
  helm upgrade akri akri-helm-charts/akri-dev \
    --set imagePullSecrets[0].name="crPullSecret" \
    --set custom.discovery.enabled=true  \
    --set custom.discovery.image.repository=$DH_IMAGE \
    --set custom.discovery.image.tag=$TAGS \
    --set custom.discovery.name=akri-http-discovery  \
    --set custom.configuration.enabled=true  \
    --set custom.configuration.name=akri-http  \
    --set custom.configuration.discoveryHandlerName=http \
    --set custom.configuration.discoveryDetails=http://discovery:9999/discovery \
    --set custom.brokerPod.image.repository=nginx
  watch kubectl get pods,akrii
```
Our empty nginx brokers do not do anything with the devices they've requested, so lets create our own broker.

## Create a sample broker
We have successfully created our Discovery Handler. If you want Akri to also automatically deploy Pods (called brokers)
to each discovered device, this section will show you how to create a custom broker that will make the HTTP-based Device
data available to the cluster.  The broker can be written in any language as it will be deployed as an individual pod.

3 different broker implementations have been created for the HTTP Discovery Handler in the [http-extensibility
branch](https://github.com/deislabs/akri/tree/http-extensibility), 2 in Rust and 1 in Go:
* The standalone broker is a self-contained scenario that demonstrates the ability to interact with HTTP-based devices
  by `curl`ing a device's endpoints. This type of solution would be applicable in batch-like scenarios where the broker
  performs a predictable set of processing steps for a device.
* The second scenario uses gRPC. gRPC is an increasingly common alternative to REST-like APIs and supports
  high-throughput and streaming methods. gRPC is not a requirement for broker implementations in Akri but is used here
  as one of many mechanisms that may be used. The gRPC-based broker has a companion client. This is a more realistic
  scenario in which the broker proxies client requests using gRPC to HTTP-based devices. The advantage of this approach
  is that device functionality is encapsulated by an API that is exposed by the broker. In this case the API has a
  single method but in practice, there could be many methods implemented.
* The third implementation is a gRPC-based broker and companion client implemented in Golang. This is functionally
  equivalent to the Rust implementation and shares a protobuf definition. For this reason, you may combine the Rust
  broker and client with the Golang broker and client arbitrarily. The Golang broker is described in the
  [`http-apps`](https://github.com/deislabs/akri/blob/http-extensibility/samples/apps/http-apps/README.md) directory.

For this, we will describe the first option, a standalone broker.  For a more detailed look at the other gRPC options,
please look at [extensibility-http-grpc.md in the http-extensibility
branch](https://github.com/deislabs/akri/blob/http-extensibility/docs/extensibility-http-grpc.md).

First, let's create a new Rust project for our sample broker.  We can use cargo to create our project by navigating to
`samples/brokers` and running:

```bash
cargo new http
```

Once the http project has been created, it can be added to the greater Akri project by adding `"samples/brokers/http"`
to the **members** in `./Cargo.toml`.

To access the HTTP-based Device data, we first need to retrieve the discovery information.  Any information stored in
the `Device` properties map will be transferred into the broker container's environment variables.  Retrieving them is
simply a matter of querying environment variables like this:

```rust
let device_url = env::var("AKRI_HTTP_DEVICE_ENDPOINT")?;
```

For our HTTP broker, the data can be retrieved with a simple GET:

```rust
async fn read_sensor(device_url: &str) {
    match get(device_url).await {
        Ok(resp) => {
            let body = resp.text().await;
        }
        Err(err) => println!("Error: {:?}", err),
    };
}
```

We can tie all the pieces together in `samples/brokers/http/src/main.rs`.  We retrieve the HTTP-based Device url from
the environment variables, make a simple GET request to retrieve the device data, and output the response to the log:

```rust
use reqwest::get;
use std::env;
use tokio::{time, time::Duration};

const DEVICE_ENDPOINT: &str = "AKRI_HTTP_DEVICE_ENDPOINT";

async fn read_sensor(device_url: &str) {
    match get(device_url).await {
        Ok(resp) => {
            let body = resp.text().await;
            println!("[main:read_sensor] Response body: {:?}", body);
        }
        Err(err) => println!("Error: {:?}", err),
    };
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let device_url = env::var(DEVICE_ENDPOINT)?;
    let mut tasks = Vec::new();
    tasks.push(tokio::spawn(async move {
        loop {
            time::delay_for(Duration::from_secs(10)).await;
            read_sensor(&device_url[..]).await;
        }
    }));
    futures::future::join_all(tasks).await;
    Ok(())
}
```

and ensure that we have the required dependencies in `samples/brokers/http/Cargo.toml`:

```toml
[[bin]]
name = "standalone"
path = "src/main.rs"

[dependencies]
futures = "0.3"
reqwest = "0.10.8"
tokio = { version = "0.2", features = ["rt-threaded", "time", "stream", "fs", "macros", "uds"] }
```

To build the HTTP broker, we need to create a Dockerfile, `samples/brokers/http/Dockerfiles/standalone`:

```dockerfile
FROM amd64/rust:1.47 as build
RUN rustup component add rustfmt --toolchain 1.47.0-x86_64-unknown-linux-gnu
RUN USER=root cargo new --bin http
WORKDIR /http

COPY ./samples/brokers/http/Cargo.toml ./Cargo.toml
RUN cargo build \
    --bin=standalone \
    --release
RUN rm ./src/*.rs
RUN rm ./target/release/deps/standalone*
COPY ./samples/brokers/http .
RUN cargo build \
    --bin=standalone \
    --release

FROM amd64/debian:buster-slim
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl-dev \
    openssl && \
    apt-get clean

COPY --from=build /http/target/release/standalone /standalone
LABEL org.opencontainers.image.source https://github.com/deislabs/akri
ENV SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt
ENV SSL_CERT_DIR=/etc/ssl/certs
ENV RUST_LOG standalone

ENTRYPOINT ["/standalone"]
```

Akri's `.dockerignore` is configured so that docker will ignore most files in our repository, some exceptions will need
to be added to build the HTTP broker:

```console
!samples/brokers/http
```

Now you are ready to **build the HTTP broker**!  To do so, we simply need to run this step from the base folder of the
Akri repo:

```bash
HOST="ghcr.io"
USER=[[GITHUB-USER]]
BROKER="http-broker"
TAGS="v1"

BROKER_IMAGE="${HOST}/${USER}/${BROKER}"
BROKER_IMAGE_TAGGED="${BROKER_IMAGE}:${TAGS}"

docker build \
--tag=${BROKER_IMAGE_TAGGED} \
--file=./samples/brokers/http/Dockerfiles/standalone \
. && \
docker push ${BROKER_IMAGE_TAGGED}
```

## Deploy broker

Now that the HTTP broker has been created, we can substitute it's image in for the simple nginx broker we previously
used in our installation command.
```bash
  helm upgrade akri akri-helm-charts/akri-dev \
    --set imagePullSecrets[0].name="crPullSecret" \
    --set custom.discovery.enabled=true  \
    --set custom.discovery.image.repository=$DH_IMAGE \
    --set custom.discovery.image.tag=$TAGS \
    --set custom.discovery.name=akri-http-discovery  \
    --set custom.configuration.enabled=true  \
    --set custom.configuration.name=akri-http  \
    --set custom.configuration.discoveryHandlerName=http \
    --set custom.configuration.discoveryDetails=http://discovery:9999/discovery \
    --set custom.configuration.brokerPod.image.repository=$BROKER_IMAGE \
    --set custom.configuration.brokerPod.image.tag=$TAGS
  watch kubectl get pods,akrii
```
> Note: substitute `helm upgrade` for `helm install` if you do not have an existing Akri installation

We can watch as the broker pods get deployed: 
```bash
watch kubectl get pods -o wide
```