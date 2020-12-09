# Deeper dive into HTTP-based Device brokers

3 different broker implementations have been created for the HTTP protocol in the http-extensibility branch, 2 in Rust and 1 in Go:
* The standalone broker is a self-contained scenario that demonstrates the ability to interact with HTTP-based devices by `curl`ing a device's endpoints. This type of solution would be applicable in batch-like scenarios where the broker performs a predictable set of processing steps for a device.
* The second scenario uses gRPC. gRPC is an increasingly common alternative to REST-like APIs and supports high-throughput and streaming methods. gRPC is not a requirement for broker implements in Akri but is used here as one of many mechanisms that may be used. The gRPC-based broker has a companion client. This is a more realistic scenario in which the broker proxies client requests using gRPC to HTTP-based devices. The advantage of this approach is that device functionality is encapsulated by an API that is exposed by the broker. In this case the API has a single method but in practice, there could be many methods implemented.
* The third implemnentation is a gRPC-based broker and companion client implemented in Golang. This is functionally equivalent to the Rust implementation and shares a protobuf definition. For this reason, you may combine the Rust broker and client with the Golang broker and client arbitrarily. The Golang broker is described in the [`http-apps`](./samples/apps/http-apps/README.md) directory.

The first option, a standalone broker, is described in docs/extensibility.md.

The two gRPC brokers are implemented here as well.  This document will describe the second option, a Rust gRPC broker.

Please read docs/extensibility.md before reading this document.  This document will not cover [creating and deploying mock HTTP-based Devices](docs/extensibility.md#create-some-http-devices), [how to add the HTTP protocol to Akri](docs/extensibility.md#new-discoveryhandler-implementation), or [how to deploy the updated Akri](docs/extensibility.md#deploy-akri).

## Creating a Rust gRPC broker (and client)

First, we need to create a project.  We can use `cargo` to create our project by navigating to `samples/brokers` and running `cargo new http`.  Once the http project has been created, it can be added to the greater Akri project by adding `"samples/brokers/http"` to the **members** in `./Cargo.toml`.

The broker implementation can be split into parts:

1. Accessing the HTTP-based Device data
1. Exposing the data to the cluster

We also provide a gRPC client implementation that can be used to access the brokered data.

1. Reading the data in the cluster

### Accessing the data
To access the HTTP-based Device data, we first need to retrieve any discovery information.  Any information stored in the DiscoveryResult properties map will be transferred into the broker container's environment variables.  Retrieving them is simply a matter of querying environment variables like this:

```rust
let device_url = env::var("AKRI_HTTP_DEVICE_ENDPOINT")?;
```

For our HTTP-based Device broker, the data can be generated with an http get.  In fact, the code we used in `discover` can be adapted for what we need:

```rust
async fn read_sensor(
    &self,
    _rqst: Request<ReadSensorRequest>,
) -> Result<Response<ReadSensorResponse>, Status> {
    match get(&self.device_url).await {
        Ok(resp) => {
            let body = resp.text().await.unwrap();
            Ok(Response::new(ReadSensorResponse { value: body }))
        }
        Err(err) => {
            Err(Status::new(Code::Unavailable, "device is unavailable"))
        }
    }
}
```

### Exposing the data to the cluster
For a gRPC service, we need to do several things:

1. Create a proto file describing our gRPC service
1. Create a build file that a gRPC library like Tonic can use
1. Leverage the output of our gRPC library build

The first step is fairly simple for our Http devices (create this in `samples/brokers/http/proto/http.proto`):

```proto
syntax = "proto3";

option go_package = "github.com/deislabs/akri/http-extensibility/proto";

package http;

service DeviceService {
    rpc ReadSensor (ReadSensorRequest) returns (ReadSensorResponse);
}

message ReadSensorRequest {
    string name = 1;
}
message ReadSensorResponse {
    string value = 1;
}
```

The second step, assuming Tonic (though there are several very good gRPC libraries) is to create `samples/brokers/http/build.rs`:

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::compile_protos("proto/http.proto")?;
    Ok(())
}
```

With the gRPC implementation created, we can now start utilizing it.  Tonic has made this very simple, we can leverage a simple macro like this:

```rust
pub mod http {
    tonic::include_proto!("http");
}
```

We can tie these pieces together in our main and retrieve the endpoint from the environment variables in `samples/brokers/http/src/broker.rs` (notice that we specify broker.rs, as main.rs is used for our standalone broker).  Here we use the generated gRPC service code to listen for gRPC requests:

```rust
pub mod http {
    tonic::include_proto!("http");
}

use clap::{App, Arg};
use http::{
    device_service_server::{DeviceService, DeviceServiceServer},
    ReadSensorRequest, ReadSensorResponse,
};
use reqwest::get;
use std::env;
use std::net::SocketAddr;
use tonic::{transport::Server, Code, Request, Response, Status};

const DEVICE_ENDPOINT: &str = "AKRI_HTTP_DEVICE_ENDPOINT";

#[derive(Default)]
pub struct Device {
    device_url: String,
}

#[tonic::async_trait]
impl DeviceService for Device {
    async fn read_sensor(
        &self,
        _rqst: Request<ReadSensorRequest>,
    ) -> Result<Response<ReadSensorResponse>, Status> {
        match get(&self.device_url).await {
            Ok(resp) => {
                let body = resp.text().await.unwrap();
                println!("[read_sensor] Response body: {:?}", body);
                Ok(Response::new(ReadSensorResponse { value: body }))
            }
            Err(err) => {
                Err(Status::new(Code::Unavailable, "device is unavailable"))
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("[main] Entered");
    let matches = App::new("broker")
        .arg(
            Arg::with_name("grpc_endpoint")
                .long("grpc_endpoint")
                .value_name("ENDPOINT")
                .help("Endpoint address that the gRPC server will listen on.")
                .required(true),
        )
        .get_matches();
    let grpc_endpoint = matches.value_of("grpc_endpoint").unwrap();
    let addr: SocketAddr = grpc_endpoint.parse().unwrap();
    let device_url = env::var(DEVICE_ENDPOINT)?;
    println!("[main] gRPC service proxying: {}", device_url);
    let device_service = Device { device_url };
    let service = DeviceServiceServer::new(device_service);

    Server::builder()
        .add_service(service)
        .serve(addr)
        .await
        .expect("unable to start http-prtocol gRPC server");

    Ok(())
}
```

To ensure that the broker builds, update `samples/brokers/http/Cargo.toml` with the broker `[[bin]]` and dependencies:

```toml
[[bin]]
name = "broker"
path = "src/grpc/broker.rs"

[dependencies]
clap = "2.33.3"
futures = "0.3"
futures-util = "0.3"
prost = "0.6"
reqwest = "0.10.8"
tokio = { version = "0.2", features = ["rt-threaded", "time", "stream", "fs", "macros", "uds"] }
tonic = "0.1"

[build-dependencies]
tonic-build = "0.1.1"
```

### Reading the data in the cluster

The steps to generate a gRPC client are very similar to creating a broker.  We will start here, with the assumption that a broker has been created and leverage the directory structure and files that have already been created.

Having already created out gRPC implementation, we can now start using it with the Tonic macros:

```rust
pub mod http {
    tonic::include_proto!("http");
}
```

This provides an easy way to query our HTTP-based Device gRPC in `samples/brokers/http/src/client.rs` (notice, again, that we use client.rs rather than main.rs or broker.rs).  Here we create a simlpe loop that calls into the generated gRPC client code to read our HTTP-based Device data:

```rust
pub mod http {
    tonic::include_proto!("http");
}

use clap::{App, Arg};
use http::{device_service_client::DeviceServiceClient, ReadSensorRequest};
use tokio::{time, time::Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let matches = App::new("client")
        .arg(
            Arg::with_name("grpc_endpoint")
                .long("grpc_endpoint")
                .value_name("ENDPOINT")
                .help("Endpoint address of the gRPC server.")
                .required(true),
        )
        .get_matches();
    let grpc_endpoint = matches.value_of("grpc_endpoint").unwrap();
    let endpoint = format!("http://{}", grpc_endpoint);
    let mut client = DeviceServiceClient::connect(endpoint).await?;

    loop {
        let rqst = tonic::Request::new(ReadSensorRequest {
            name: "/".to_string(),
        });
        println!("[main:loop] Calling read_sensor");
        let resp = client.read_sensor(rqst).await?;
        println!("[main:loop] Response: {:?}", resp);
        time::delay_for(Duration::from_secs(10)).await;
    }
    Ok(())
}
```

To ensure that our client builds, we have update `samples/brokers/http/Cargo.toml` with the client `[[bin]]`:

```toml
[[bin]]
name = "broker"
path = "src/grpc/broker.rs"

[[bin]]
name = "client"
path = "src/grpc/client.rs"

[dependencies]
clap = "2.33.3"
futures = "0.3"
futures-util = "0.3"
prost = "0.6"
reqwest = "0.10.8"
tokio = { version = "0.2", features = ["rt-threaded", "time", "stream", "fs", "macros", "uds"] }
tonic = "0.1"

[build-dependencies]
tonic-build = "0.1.1"
```

## Build and Deploy gRPC broker and client

To build the broker and client, we create simple Dockerfiles

`samples/brokers/http/Dockerfiles/grpc.broker`
```dockerfile
FROM amd64/rust:1.47 as build
RUN rustup component add rustfmt --toolchain 1.47.0-x86_64-unknown-linux-gnu
RUN USER=root cargo new --bin http
WORKDIR /http
COPY ./samples/brokers/http/Cargo.toml ./Cargo.toml
RUN cargo build \
    --bin=broker \
    --release
RUN rm ./src/*.rs
RUN rm ./target/release/deps/http*
COPY ./samples/brokers/http .
RUN cargo build \
    --bin=broker \
    --release
FROM amd64/debian:buster-slim
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl-dev \
    openssl && \
    apt-get clean
COPY --from=build /http/target/release/broker /broker
LABEL org.opencontainers.image.source https://github.com/deislabs/akri
ENV SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt
ENV SSL_CERT_DIR=/etc/ssl/certs
ENV RUST_LOG broker
ENTRYPOINT ["/broker"]
```

`samples/brokers/http/Dockerfiles/grpc.client`
```dockerfile
FROM amd64/rust:1.47 as build
RUN rustup component add rustfmt --toolchain 1.47.0-x86_64-unknown-linux-gnu
RUN USER=root cargo new --bin http
WORKDIR /http
COPY ./samples/brokers/http/Cargo.toml ./Cargo.toml
RUN cargo build \
    --bin=client \
    --release
RUN rm ./src/*.rs
RUN rm ./target/release/deps/http*
COPY ./samples/brokers/http .
RUN cargo build \
    --bin=client \
    --release
FROM amd64/debian:buster-slim
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl-dev \
    openssl && \
    apt-get clean
COPY --from=build /http/target/release/client /client
LABEL org.opencontainers.image.source https://github.com/deislabs/akri
ENV SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt
ENV SSL_CERT_DIR=/etc/ssl/certs
ENV RUST_LOG client
ENTRYPOINT ["/client"]
```

We can build the containers using `docker build` and make them available to our cluster with `docker push`:
```bash
HOST="ghcr.io"
USER=[[GITHUB-USER]]
BROKER="http-broker"
TAGS="v1"

for APP in "broker" "client"
do
  docker build \
  --tag=${HOST}/${USER}/${REPO}-grpc-${APP}:${TAGS} \
  --file=./samples/brokers/http/Dockerfiles/grpc.${APP} \
  . && \
  docker push ${HOST}/${USER}/${REPO}-grpc-${APP}:${TAGS}
done
```

Now we can deploy the gRPC-enabled broker using an Akri Configuration, `samples/brokers/http/kubernetes/http.grpc.broker.yaml` (being sure to update **image** according to the last steps):

```yaml
apiVersion: akri.sh/v0
kind: Configuration
metadata:
  name: http-grpc-broker-rust
spec:
  protocol:
    http:
      discoveryEndpoint: http://discovery:8080/discovery
  capacity: 1
  brokerPodSpec:
    imagePullSecrets: # GitHub Container Registry secret
      - name: SECRET
    containers:
      - name: http-grpc-broker-rust
        image: IMAGE
        args:
          - --grpc_endpoint=0.0.0.0:50051
        resources:
          limits:
            "{{PLACEHOLDER}}": "1"
  instanceServiceSpec:
    ports:
      - name: grpc
        port: 50051
        targetPort: 50051
  configurationServiceSpec:
    ports:
      - name: grpc
        port: 50051
        targetPort: 50051
```

With this Akri Configuration, we can use `kubectl` to update the cluster:

```bash
kubectl apply --filename=./kubernetes/http.grpc.broker.yaml
```

Assuming that you have [created and deployed mock HTTP-based Devices](docs/extensibility.md#create-some-http-devices), you can query the broker's logs and should see the gRPC starting and then pending:

```bash
kubectl logs pod/akri-http-...-pod
[main] Entered
[main] gRPC service proxying: http://device-7:8080
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

The gRPC client can be deployed as any Kubernetes workload.  For our example, we create a Deployment, `samples/brokers/http/kubernetes/http.grpc.client.yaml` (updating **image** according to the previous `docker push` commands):
```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: http-grpc-client-rust
spec:
  replicas: 1
  selector:
    matchLabels:
      id: akri-http-client-rust
  template:
    metadata:
      labels:
        id: akri-http-client-rust
      name: http-grpc-client-rust
    spec:
      imagePullSecrets:
        - name: SECRET
      containers:
        - name: http-grpc-client-rust
          image: IMAGE
          args:
            - --grpc_endpoint=http-svc:50051
```

You may then deploy the gRPC client:

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
[main:loop] Calling read_sensor
[main:loop] Response: Response { metadata: MetadataMap { headers: {"content-type": "application/grpc", "date": "Wed, 11 Nov 2020 17:46:55 GMT", "grpc-status": "0"} }, message: ReadSensorResponse { value: "0.6088971084079992" } }
[main:loop] Constructing Request
[main:loop] Calling read_sensor
[main:loop] Response: Response { metadata: MetadataMap { headers: {"content-type": "application/grpc", "date": "Wed, 11 Nov 2020 17:47:05 GMT", "grpc-status": "0"} }, message: ReadSensorResponse { value: "0.9686970038897007" } }
```

