# Extensibility

Akri has [implemented several discovery protocols](./roadmap.md#currently-supported-protocols) with sample brokers and applications. However, there may be protocols you would like to use to discover resources that have not been implemented yet.  This document walks you through how to **extend Akri** to discover new types of devices that you are interested in.

Below, you will find all the development steps needed to implement a new protocol and sample broker. This document will also cover the steps to get your protocol and broker added to Akri, should you wish to contribute them back.

Before continuing, please read the [Akri architecture](./architecture.md) and [development](./development.md) documentation pages.  They will provide a good understanding of Akri, how it works, what components it is composed of, and how to build it.

To add a new protocol implementation, several things are needed:

1. Add a new DiscoveryHandler implementation in Akri agent
1. Update the Akri Configuration Custom Resource Definition (CRD) to include the new DiscoveryHandler implementation
1. Build versions of Akri agent and controller that understand the new DiscoveryHandler
1. Create a (protocol) broker pod for the new capability

> **Note:** a protocol implementation can be any set of steps to discover devices. It does not have to be a "protocol" in the traditional sense. For example, Akri defines udev (not often called a "protocol") and OPC UA as protocols.

Here, we will create a protocol to discover **HTTP-based devices** that publish random sensor data.  For reference, we have created a [http-extensibility branch](https://github.com/deislabs/akri/tree/http-extensibility) with the implementation defined below.  For convenience, you can [compare the http-extensibility branch with main here](https://github.com/deislabs/akri/compare/http-extensibility).

Any Docker-compatible container registry will work for hosting the containers being used in this example (dockerhub, Github Container Registry, Azure Container Registry, etc).  Here, we are using the [GitHub Container Registry](https://github.blog/2020-09-01-introducing-github-container-registry/). You can follow the [getting started guide here to enable it for yourself](https://docs.github.com/en/free-pro-team@latest/packages/getting-started-with-github-container-registry).

> **Note:** if your container registry is private, you will need to create a kubernetes secret (`kubectl create secret docker-registry crPullSecret --docker-server=<cr>  --docker-username=<cr-user> --docker-password=<cr-token>`) and access it with an `imagePullSecret`.  Here, we will assume the secret is named `crPullSecret`.

## New DiscoveryHandler implementation
If the resource you are interested in defining is not accessible through the [included protocols](./roadmap.md#currently-supported-protocols), then you will need to create a DiscoveryHandler for your new protocol.  Here, we will create a discovery handler in order to discover HTTP resources.

New protocols require new implementations of the DiscoveryHandler:

```rust
#[async_trait]
pub trait DiscoveryHandler {
    async fn discover(&self) -> Result<Vec<DiscoveryResult>, Error>;
    fn are_shared(&self) -> Result<bool, Error>;
}
```

DiscoveryHandler has the following functions:

1. **discover** - This function is called periodically by the Akri agent and returns the list of discovered devices. It should have all the functionality desired for discovering devices via your protocol and filtering for only the desired set. In our case, we will require that a URL is passed via the Configuration as a discovery endpoint. Our implementation will ping the discovery service at that URL to see if there are any devices.
1. **are_shared** - This function defines whether the instances discovered are shared or not.  A shared Instance is typically something that multiple nodes can interact with (like an IP camera).  An unshared Instance is typically something only one node can access.

To create a new protocol type, a new struct and implementation of DiscoveryHandler is required.  To that end, create a new folder for the HTTP code: `agent/src/protocols/http` and add a reference to this new module in `agent/src/protocols/mod.rs`:

```rust
mod debug_echo;
mod http; // <--- Our new http module
mod onvif;
```

Next, add a few files to the new http folder:

To provide an implementation for the HTTP protocol discovery, create `agent/src/protocols/http/discovery_handler.rs` and define **HTTPDiscoveryHandler**.

For the HTTP protocol, `discover` will perform an HTTP GET on the protocol's discovery service URL and the Instances will be shared (reflecting that multiple nodes likely have access to HTTP-based Devices):
```rust
use super::super::{DiscoveryHandler, DiscoveryResult};

use akri_shared::akri::configuration::HTTPDiscoveryHandlerConfig;
use async_trait::async_trait;
use failure::Error;
use reqwest::get;
use std::collections::HashMap;

const BROKER_NAME: &str = "AKRI_HTTP";
const DEVICE_ENDPOINT: &str = "AKRI_HTTP_DEVICE_ENDPOINT";

pub struct HTTPDiscoveryHandler {
    discovery_handler_config: HTTPDiscoveryHandlerConfig,
}
impl HTTPDiscoveryHandler {
    pub fn new(discovery_handler_config: &HTTPDiscoveryHandlerConfig) -> Self {
        HTTPDiscoveryHandler {
            discovery_handler_config: discovery_handler_config.clone(),
        }
    }
}
#[async_trait]

impl DiscoveryHandler for HTTPDiscoveryHandler {
    async fn discover(&self) -> Result<Vec<DiscoveryResult>, failure::Error> {
        let url = self.discovery_handler_config.discovery_endpoint.clone();
        match get(&url).await {
            Ok(resp) => {
                // Reponse is a newline separated list of devices (host:port) or empty
                let device_list = &resp.text().await?;

                let result = device_list
                    .lines()
                    .map(|endpoint| {
                        let mut props = HashMap::new();
                        props.insert(BROKER_NAME.to_string(), "http".to_string());
                        props.insert(DEVICE_ENDPOINT.to_string(), endpoint.to_string());
                        DiscoveryResult::new(endpoint, props, true)
                    })
                    .collect::<Vec<DiscoveryResult>>();
                Ok(result)
            }
            Err(err) => {
                Err(failure::format_err!(
                    "Failed to connect to discovery endpoint results: {:?}",
                    err
                ))
            }
        }
    }
    fn are_shared(&self) -> Result<bool, Error> {
        Ok(true)
    }
}
```

To ensure that the HttpDiscoveryHandler is available to the rest of agent, we need to update `agent/src/protocols/http/mod.rs` by adding a reference to the new module:
```rust
mod discovery_handler;
pub use self::discovery_handler::HTTPDiscoveryHandler;
```

The next step is to update `inner_get_discovery_handler` in `agent/src/protocols/mod.rs` to create an instance of HttpDiscoveryHandler:
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

Finally, we need to update `./agent/Cargo.toml` to build with the dependencies http is using:
```TOML
[dependencies]
hyper-async = { version = "0.13.5", package = "hyper" }
reqwest = "0.10.8"
```

## Update Akri Configuration Custom Resource Definition (CRD)
Now we need to update the Akri Configuration CRD so that we can pass some properties to our new protocol handler.  First, let's create our data structures.

The first step is to create a DiscoveryHandler configuration struct. This struct will be used to deserialize the Configuration CRD contents and will be passed on to our HttpDiscoveryHandler. Here we are specifying that users must pass in the URL of a discovery service which will be queried to find our HTTP-based Devices.  Add this code to `shared/src/akri/configuration.rs`:

```rust
/// This defines the HTTP data stored in the Configuration
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct HTTPDiscoveryHandlerConfig {
    pub discovery_endpoint: String,
}
```

Next, we need to update the Akri protocol handler enum to include http:

```rust
pub enum ProtocolHandler {
    http(HTTPDiscoveryHandlerConfig),
    ...
}
```

Finally, we need to add http to the Configuration CRD yaml so that Kubernetes can properly validate an Akri Configuration attempting to search for HTTP devices.  The Akri CRDs are defined by the Akri Helm chart.  To add http, `deployment/helm/crds/akri-configuration-crd.yaml` needs to be changed:

> **NOTE** Because we are making local changes to the Akri Helm chart, the deislabs/akri hosted charts will not include our change.  To use your local Akri chart, you must `helm install` a copy of this directory and **not** deislabs/akri hosted charts.  This will be explained later in the **Deploy Akri** steps.

```yaml
apiVersion: apiextensions.k8s.io/v1
kind: CustomResourceDefinition
metadata:
  name: configurations.akri.sh
spec:
  group: akri.sh
...
                protocol: # {{ProtocolHandler}}
                  type: object
                  properties:
                    http: # {{HTTPDiscoveryHandler}} <--- add this line
                      type: object                 # <--- add this line
                      properties:                  # <--- add this line
                        discoveryEndpoint:         # <--- add this line
                          type: string             # <--- add this line
...
                  oneOf:
                    - required: ["http"]           # <--- add this line
```

## Building Akri agent|controller
Having successfully updated the Akri agent and controller to understand our HTTP resource, the agent and controller need to be built.  Running the following `make` commands will build and push new versions of the agent and controller to your container registry (in this case ghcr.io/[[GITHUB-USER]]/agent and ghcr.io/[[GITHUB-USER]]/controller).

```bash
USER=[[GTHUB-USER]]
PREFIX=ghcr.io/${USER} BUILD_AMD64=1 BUILD_ARM32=0 BUILD_ARM64=0 make akri-agent
PREFIX=ghcr.io/${USER} BUILD_AMD64=1 BUILD_ARM32=0 BUILD_ARM64=0 make akri-controller
```

> **NOTE** These commands build for amd64 (`BUILD_AMD64=1`), other archs can be built by setting `BUILD_*` differently.  You can find more details on building Akri in the [development guide](./development.md).

## Create a sample protocol broker
The final step, is to create a protocol broker that will make the HTTP-based Device data available to the cluster.  The broker can be written in any language as it will be deployed as an individual pod.

3 different broker implementations have been created for the HTTP protocol in the [http-extensibility branch](https://github.com/deislabs/akri/tree/http-extensibility), 2 in Rust and 1 in Go:
* The standalone broker is a self-contained scenario that demonstrates the ability to interact with HTTP-based devices by `curl`ing a device's endpoints. This type of solution would be applicable in batch-like scenarios where the broker performs a predictable set of processing steps for a device.
* The second scenario uses gRPC. gRPC is an increasingly common alternative to REST-like APIs and supports high-throughput and streaming methods. gRPC is not a requirement for broker implementations in Akri but is used here as one of many mechanisms that may be used. The gRPC-based broker has a companion client. This is a more realistic scenario in which the broker proxies client requests using gRPC to HTTP-based devices. The advantage of this approach is that device functionality is encapsulated by an API that is exposed by the broker. In this case the API has a single method but in practice, there could be many methods implemented.
* The third implemnentation is a gRPC-based broker and companion client implemented in Golang. This is functionally equivalent to the Rust implementation and shares a protobuf definition. For this reason, you may combine the Rust broker and client with the Golang broker and client arbitrarily. The Golang broker is described in the [`http-apps`](https://github.com/deislabs/akri/blob/http-extensibility/samples/apps/http-apps/README.md) directory.

For this, we will describe the first option, a standalone broker.  For a more detailed look at the other gRPC options, please look at [extensibility-http-grpc.md in the http-extensibility branch](https://github.com/deislabs/akri/blob/http-extensibility/docs/extensibility-http-grpc.md).

First, let's create a new Rust project for our sample broker.  We can use cargo to create our project by navigating to `samples/brokers` and running:

```bash
cargo new http
```

Once the http project has been created, it can be added to the greater Akri project by adding `"samples/brokers/http"` to the **members** in `./Cargo.toml`.

To access the HTTP-based Device data, we first need to retrieve the discovery information.  Any information stored in the DiscoveryResult properties map will be transferred into the broker container's environment variables.  Retrieving them is simply a matter of querying environment variables like this:

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

We can tie all the pieces together in `samples/brokers/http/src/main.rs`.  We retrieve the HTTP-based Device url from the environment variables, make a simple GET request to retrieve the device data, and output the response to the log:

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

Akri's `.dockerignore` is configured so that docker will ignore most files in our repository, some exceptions will need to be added to build the HTTP broker:

```console
!samples/brokers/http
```

Now you are ready to **build the HTTP broker**!  To do so, we simply need to run this step from the base folder of the Akri repo:

```bash
HOST="ghcr.io"
USER=[[GITHUB-USER]]
BROKER="http-broker"
TAGS="v1"

IMAGE="${HOST}/${USER}/${BROKER}:${TAGS}"

docker build \
--tag=${IMAGE} \
--file=./samples/brokers/http/Dockerfiles/standalone \
. && \
docker push ${IMAGE}
```

To deploy the standalone broker, we'll need to create an Akri Configuration `./samples/brokers/http/kubernetes/http.yaml` (be sure to update **image**):
```yaml
apiVersion: akri.sh/v0
kind: Configuration
metadata:
  name: http
spec:
  protocol:
    http:
      discoveryEndpoint: http://discovery:9999/discovery
  capacity: 1
  brokerPodSpec:
    imagePullSecrets: # Container Registry secret
      - name: crPullSecret
    containers:
      - name: http-broker
        image: IMAGE
        resources:
          limits:
            "{{PLACEHOLDER}}": "1"
```


# Create some HTTP devices
At this point, we've extended Akri to include discovery for our HTTP protocol and we've created an HTTP broker that can be deployed.  To really test our new discovery and brokers, we need to create something to discover.

For this exercise, we can create an HTTP service that listens to various paths.  Each path can simulate a different device by publishing some value.  With this, we can create a single Kubernetes pod that can simulate multiple devices.  To make our scenario more realistic, we can add a discovery endpoint as well.  Further, we can create a series of Kubernetes services that create facades for the various paths, giving the illusion of multiple devices and a separate discovery service.

To that end, let's:

1. Create a web service that mocks HTTP devices and a discovery service
1. Deploy, start, and expose our mock HTTP devices and discovery service

## Mock HTTP devices and Discovery service
To simulate a set of discoverable HTTP devices and a discovery service, create a simple HTTP server (`samples/apps/http-apps/cmd/device/main.go`).  The application will accept a list of `path` arguments, which will define endpoints that the service will respond to.  These endpoints represent devices in our HTTP protocol.  The application will also accept a set of `device` arguments, which will define the set of discovered devices.

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

## Build and Deploy devices and discovery
To build and deploy the mock devices and discovery, a simple Dockerfile can be created that builds and exposes our mock server `samples/apps/http-apps/Dockerfiles/device`:
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

The mock devices can be deployed with a Kubernetes deployment `samples/apps/http-apps/kubernetes/device.yaml` (update **image** based on the ${IMAGE}):
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

> **NOTE** We're using one deployment|pod to represent 9 devices AND a discovery service ... we will create 9 (distinct) Services against it (1 for each mock device) and 1 Service to present the discovery service.

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


# Where the rubber meets the road!
At this point, we've extended Akri to include discovery for our HTTP protocol and we've created an HTTP broker that can be deployed.  Let's take HTTP for a spin!!

## Deploy Akri

> Optional: If you've previous installed Akri and wish to reset, you may:
>
> ```bash
> # Delete Akri Helm
> sudo helm delete akri
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

sudo helm install akri ./akri/deployment/helm \
   --set imagePullSecrets[0].name=crPullSecret \
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


## Deploy broker

Once the HTTP broker has been created, the next question is how to deploy it.  For this, we need the Configuration we created earlier `samples/brokers/http/kubernetes/http.yaml`.  To deploy, use a simple `kubectl` command like this:
```bash
kubectl apply --filename=./samples/brokers/http/kubernetes/http.yaml
```

We can watch as the broker pods get deployed: 
```bash
watch kubectl get pods -o wide
```


## Contributing your Protocol Implementation back to Akri
Now that you have a working protocol implementation and broker, we'd love for you to contribute your code to Akri. The following steps will need to be completed to do so:
1. Create an Issue with a feature request for this protocol.
2. Create a proposal and put in PR for it to be added to the [proposals folder](./proposals).
3. Implement your protocol and provide a full end to end sample.
4. Create a pull request, updating the minor version of Akri. See [contributing](./contributing.md#versioning) to learn more about our versioning strategy.

For a protocol to be considered fully implemented the following must be included in the PR. Note that the HTTP protocol above has not completed all of the requirements. 
1. A new DiscoveryHandler implementation in the Akri agent
1. An update to the Configuration CRD to include the new `ProtocolHandler`
1. A sample protocol broker for the new resource
1. A sample Configuration that uses the new protocol in the form of a Helm template and values
1. (Optional) A sample end application that utilizes the services exposed by the Configuration
1. Dockerfile[s] for broker [and sample app] and associated update to the [makefile](../build/akri-containers.mk)
1. Github workflow[s] for broker [and sample app] to build containers and push to Akri container repository
1. Documentation on how to use the new sample Configuration, like the [udev Configuration document](./udev-configuration.md)
