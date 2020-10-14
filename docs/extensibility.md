# Extensibility
While Akri has several [currently supported discovery protocols](./roadmap.md#currently-supported-protocols) and sample brokers and applications to go with them, the protocol you want to use to discover resources may not be implemented yet. This walks you through all the development steps needed to implement a new protocol and sample broker. It will also cover the steps to get your protocol and broker[s] added to Akri, should you wish to contribute them back. 

To add a new protocol implementation, three things are needed:
1. Add a new DiscoveryHandler implementation in the Akri Agent
1. Update the Configuration CRD to include the new DiscoveryHandler implementation
1. Create a protocol broker for the new capability

## The mythical Loch Ness resource
To demonstrate how new protocols can be added, we will create a protocol to discover Nessie, a mythical Loch Ness monster that lives at a specific url.

### New DiscoveryHandler implementation
If the resource you are interested in defining is not accessible through the [included protocols](./roadmap.md#currently-supported-protocols), then you will need to create a DiscoveryHandler for your new protocol.  For the sake of demonstration, we will create a discovery handler in order to discover mythical Nessie resources.

New protocols require new implementations of the DiscoveryHandler:

```rust
#[async_trait]
pub trait DiscoveryHandler {
    async fn discover(&self) -> Result<Vec<DiscoveryResult>, Error>;
    fn are_shared(&self) -> Result<bool, Error>;
}
```

To create a new protocol type, a new struct and impl block is required.  To that end, create a new folder for our Nessie code: `agent/src/protocols/nessie` and add a reference this new module in `agent/src/protocols/mod.rs`:

```rust
mod debug_echo;
mod nessie; // <--- Our new Nessie module
mod onvif;
```

Next, add a few files to our new nessie folder:

`agent/src/protocols/nessie/discovery_handler.rs`:
```rust
use super::super::{DiscoveryHandler, DiscoveryResult};
use async_trait::async_trait;
use failure::Error;
use akri_shared::akri::configuration::NessieDiscoveryHandlerConfig;

pub struct NessieDiscoveryHandler {
    discovery_handler_config: NessieDiscoveryHandlerConfig,
}

impl NessieDiscoveryHandler {
    pub fn new(
        discovery_handler_config: &NessieDiscoveryHandlerConfig,
    ) -> Self {
        NessieDiscoveryHandler {
            discovery_handler_config: discovery_handler_config.clone(),
        }
    }
}

#[async_trait]
impl DiscoveryHandler for NessieDiscoveryHandler {
    async fn discover(&self) -> Result<Vec<DiscoveryResult>, failure::Error> {
        let results = Vec::new();
        let url = self
            .discovery_handler_config
            .nessie_url
            .parse::<hyper::Uri>()
            .expect("failed to parse URL");
        if let Ok(_body) = hyper::Client::new().get(url).compat().await {
            // If the Nessie URL can be accessed, we will return a DiscoveryResult
            // instance
            let props = HashMap::new();
            props.insert(
                "nessie_url",
                self.discovery_handler_config.nessie_url.clone(),
            );
            results.push(DiscoveryResult::new(
                self.discovery_handler_config.nessie_url,
                props,
                true,
            ));
        }
        Ok(results)
    }
    fn are_shared(&self) -> Result<bool, Error> {
        Ok(true)
    }
}
```

`agent/src/protocols/nessie/mod.rs`:
```rust
mod discovery_handler;
pub use self::discovery_handler::NessieDiscoveryHandler;
```

The next step is to update `inner_get_discovery_handler` in `agent/src/protocols/mod.rs` to create a NessieDiscoveryHandler:

```rust
match discovery_handler_config {
    ProtocolHandler::nessie(nessie) => {
        Ok(Box::new(nessie::NessieDiscoveryHandler::new(&nessie)))
    }
    ...
```

### Update Configuration CRD
Now we need to update the Configuration CRD so that we can pass some properties to our new protocol handler.  First, lets create our data structures.

The first step is to create a DiscoveryHandler configuration struct. This struct will be used to deserialize the CRD contents and will be passed on to our NessieDiscoveryHandler. Here we are specifying that users must pass in the url for where Nessie lives. This means that Agent is not doing any discovery work besides validating a URL, but this is the scenario we are using to simplify the example. Add this code to `shared/src/akri/configuration.rs`:

```rust
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct NessieDiscoveryHandlerConfig {
    nessie_url: String
}
```

Next, we need to update the Akri protocol handler enum to include Nessie:

```rust
pub enum ProtocolHandler {
    nessie(NessieDiscoveryHandlerConfig),
    ...
}
```

Finally, we need to add Nessie to the CRD yaml so that Kubernetes can properly validate any one attempting to configure Akri to search for Nessie.  To do this, we need to modify `deployment/helm/crds/akri-configuration-crd.yaml`:

```yaml
openAPIV3Schema:
    type: object
    properties:
    spec:
        type: object
        properties:
        protocol: # {{ProtocolHandler}}
            type: object
            properties:
            nessie: # {{NessieDiscoveryHandler}} <--- add this line
                type: object                                # <--- add this line
                properties:                                 # <--- add this line
                nessieUrl:                                  # <--- add this line
                    type: string                            # <--- add this line
...
```

### Create a sample protocol broker
The final step, is to create a protocol broker that will make Nessie available to the cluster.  The broker can be written in any language as it will be deployed as an individual pod; however, for this example, we will make a Rust broker. We can use cargo to create our project by navigating to `samples/brokers` and running `cargo new nessie`.

As a simple strategy, we can split the broker implementation into parts:

1. Create a shared buffer for the data
1. Accessing the "nessie" data
1. Exposing the "nessie" data to the cluster

For the first step, we looked for a simple non-blocking, ring buffer ... we can add this to a module like `util` by creating `samples/brokers/nessie/src/util/mod.rs`:

```rust
pub mod nessie;
pub mod nessie_service;

use arraydeque::{ArrayDeque, Wrapping};
// Create a wrapping (non-blocking) ring buffer with a capacity of 10 
pub type FrameBuffer = ArrayDeque<[Vec<u8>; 10], Wrapping>;
```

To access the "nessie" data, we first need to retrieve any discovery information.  Any information stored in the DiscoveryResult properties map will be transferred into the broker container's environment variables.  Retrieving them is simply a matter of querying environment variables like this:

```rust
fn get_nessie_url() -> String {
    nessie_url =
        env::var("nessie_url").unwrap()
}
```

For our Nessie broker, the "nessie" data can be generated with an http get.  In fact, the code we used in `discover` can be adapted for what we need:

```rust
async fn get_nessie(nessie_url: &String, frame_buffer: Arc<Mutex<FrameBuffer>>) {
    let uri = nessie_url
        .parse::<hyper::Uri>()
        .expect("failed to parse URL");
    if let Ok(response) = hyper::Client::new().get(uri).await {
        let response_body = response
            .into_body()
            .try_fold(bytes::BytesMut::new(), |mut acc, chunk| async {
                acc.extend(chunk);
                Ok(acc)
            })
            .await
            .unwrap()
            .freeze();
        frame_buffer
            .lock()
            .unwrap()
            .push_back(response_body.to_vec());
    }
}
```

Finally, to expose data to the cluster, we suggest a simple gRPC service.  For a gRPC service, we need to do several things:

1. Create a Nessie proto file describing our gRPC service
1. Create a build file that a gRPC library like Tonic can use
1. Leverage the output of our gRPC library build

The first step is fairly simple for Nessie (create this in `samples/brokers/nessie/nessie.proto`):

```proto
syntax = "proto3";

option csharp_namespace = "Nessie";

package nessie;

service Nessie {
  rpc GetNessieNow (NotifyRequest) returns (NotifyResponse);
}

message NotifyRequest {
}

message NotifyResponse {
  bytes frame = 1;
}
```

The second step, assuming Tonic (though there are several very good gRPC libraries) is to create `samples/brokers/nessie/build.rs`:

```rust
fn main() {
    tonic_build::configure()
        .build_client(true)
        .out_dir("./src/util")
        .compile(&["./nessie.proto"], &["."])
        .expect("failed to compile protos");
}
```

Next, we need to include the gRPC generated code in by adding a reference to `nessie` in `samples/brokers/nessie/src/util/mod.rs`:

```rust
pub mod nessie;
```

With the gRPC implementation created, we can now start utilizing it.

First, we need to leverage the generated gRPC code by creating `samples/brokers/nessie/src/util/nessie.rs`:

```rust
use super::{
    nessie::{
        nessie_server::{Nessie, NessieServer},
        NotifyRequest, NotifyResponse,
    },
    FrameBuffer,
};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tonic::{transport::Server, Request, Response};

pub const NESSIE_SERVER_ADDRESS: &str = "0.0.0.0";
pub const NESSIE_SERVER_PORT: &str = "8083";

pub struct NessieService {
    frame_rx: Arc<Mutex<FrameBuffer>>,
}

#[tonic::async_trait]
impl Nessie for NessieService {
    async fn get_nessie_now(
        &self,
        _request: Request<NotifyRequest>,
    ) -> Result<Response<NotifyResponse>, tonic::Status> {
        Ok(Response::new(NotifyResponse {
            frame: match self.frame_rx.lock().unwrap().pop_front() {
                Some(data) => data,
                _ => vec![],
            },
        }))
    }
}

pub async fn serve(frame_rx: Arc<Mutex<FrameBuffer>>) -> Result<(), String> {
    let nessie = NessieService { frame_rx };
    let service = NessieServer::new(nessie);

    let addr_str = format!("{}:{}", NESSIE_SERVER_ADDRESS, NESSIE_SERVER_PORT);
    let addr: SocketAddr = match addr_str.parse() {
        Ok(sock) => sock,
        Err(e) => {
            return Err(format!("Unable to parse socket: {:?}", e));
        }
    };

    tokio::spawn(async move {
        Server::builder()
            .add_service(service)
            .serve(addr)
            .await
            .expect("couldn't build server");
    });
    Ok(())
}
```

Once the gRPC code is utilized, we need to include our nessie server code by adding a reference to `nessie_service` in `samples/brokers/nessie/src/util/mod.rs`:

```rust
pub mod nessie_service;
```


Finally, we can tie all the pieces together in our main and retrieve the url from the Configuration in `samples/brokers/nessie/src/main.rs`:

```rust
mod util;

use arraydeque::ArrayDeque;
use futures_util::stream::TryStreamExt;
use std::{
    env,
    sync::{Arc, Mutex},
};
use tokio::{time, time::Duration};
use util::{nessie_service, FrameBuffer};

fn get_nessie_url() -> String {
    env::var("nessie_url").unwrap()
}

async fn get_nessie(nessie_url: &String, frame_buffer: Arc<Mutex<FrameBuffer>>) {
    let uri = nessie_url
        .parse::<hyper::Uri>()
        .expect("failed to parse URL");
    if let Ok(response) = hyper::Client::new().get(uri).await {
        let response_body = response
            .into_body()
            .try_fold(bytes::BytesMut::new(), |mut acc, chunk| async {
                acc.extend(chunk);
                Ok(acc)
            })
            .await
            .unwrap()
            .freeze();
        frame_buffer
            .lock()
            .unwrap()
            .push_back(response_body.to_vec());
    }
}

#[tokio::main]
async fn main() {
    let frame_buffer: Arc<Mutex<FrameBuffer>> = Arc::new(Mutex::new(ArrayDeque::new()));
    let nessie_url = get_nessie_url();

    nessie_service::serve(frame_buffer.clone()).await.unwrap();

    let mut tasks = Vec::new();
    tasks.push(tokio::spawn(async move {
        loop {
            time::delay_for(Duration::from_secs(10)).await;
            get_nessie(&nessie_url, frame_buffer.clone()).await;
        }
    }));
    futures::future::join_all(tasks).await;
}
```

and ensure that we have the required dependencies in `samples/brokers/nessie/Cargo.toml`:

```toml
[dependencies]
arraydeque = "0.4"
bytes = "0.5"
futures = "0.3"
futures-util = "0.3"
hyper = "0.13"
prost = "0.6"
akri-shared = { path = "../../../shared" }
tokio = { version = "0.2", features = ["rt-threaded", "time", "stream", "fs", "macros", "uds"] }
tonic = "0.1"
tower = "0.3" 

[build-dependencies]
tonic-build = "0.1.1"
```

To build the Nessie container, we need to create a Dockerfile

```dockerfile
FROM amd64/rust:1.41 as build
RUN apt-get update && apt-get install -y --no-install-recommends \
      g++ ca-certificates curl libssl-dev pkg-config

WORKDIR /nessie
RUN echo '[workspace]' > ./Cargo.toml && \
    echo 'members = ["shared", "samples/brokers/nessie"]' >> ./Cargo.toml
COPY ./samples/brokers/nessie ./samples/brokers/nessie
COPY ./shared ./shared
RUN cargo build

FROM amd64/debian:buster-slim
RUN apt-get update && apt-get install -y --no-install-recommends libssl-dev openssl && \
      apt-get clean
COPY --from=build ./target/debug/nessie /nessie

# Expose port used by broker service
EXPOSE 8083

CMD ["./nessie"]
```

### Create a new Configuration
Once the components have been created (and assuming the container, `nessie:latest`, is available on the worker nodes), the next question is how to deploy it.  For this, we need to create a Configuration called `nessie.yaml` that leverages our new protocol:

```yaml
apiVersion: akri.sh/v0
kind: Configuration
metadata:
  name: nessie
spec:
  protocol:
    nessie:
      nessieUrl: https://www.lochness.co.uk/livecam/img/lochness.jpg
  capacity: 5
  brokerPodSpec:
    containers:
    - name: nessie-broker
      image: "nessie:latest"
      resources:
        limits:
          "{{PLACEHOLDER}}" : "1"
  instanceServiceSpec:
    ports:
    - name: grpc
      port: 80
      targetPort: 8083
  configurationServiceSpec:
    ports:
    - name: grpc
      port: 80
      targetPort: 8083
```

### Installing Akri with your new Configuration
Before you can install Akri and apply your Nessie Configuration, you must first build both the Controller and Agent containers and push them to your own container repository. You can use any container registry to host your container repository.We are using the new [GitHub container registry](https://github.blog/2020-09-01-introducing-github-container-registry/). If you want to enable GHCR, you can follow the [getting started guide](https://docs.github.com/en/free-pro-team@latest/packages/getting-started-with-github-container-registry). 

We have provided makefiles for building and pushing containers for the various components of Akri. See the [development document](./development.md) for example make commands and details on how to install the prerequisites needed for cross-building Akri components. To specifically build the Controller and Agent for x64, run the following (after installing cross):
```
PREFIX=ghcr.io/<your-github-alias> BUILD_AMD64=1 BUILD_ARM32=0 BUILD_ARM64=0 make akri-agent
PREFIX=ghcr.io/<your-github-alias> BUILD_AMD64=1 BUILD_ARM32=0 BUILD_ARM64=0 make akri-controller
```
Next, you must [generate a new Akri chart](./development#helm-template). You can now [install Akri with the newly built containers](./development#install-akri-with-newly-built-containers), setting the appropriate image address for the Agent and Controller pods in your personal container registry. 

Finally, you can apply your Nessie Configuration and watch as broker pods are created:
```sh
kubectl apply -f nessie.yaml
watch kubectl get pods
```

## Contributing your Protocol Implementation back to Akri
Now that you have a working protocol implementation and broker, we'd love for you to contribute your code to Akri. The following steps will need to be completed to do so:
1. Create an Issue with a feature request for this protocol.
2. Create a proposal and put in PR for it to be added to the [proposals folder](./proposals).
3. Implement your protocol and provide a full end to end sample.
4. Create a pull request, updating the minor version of akri. See [contributing](./contributing.md#versioning) to learn more about our versioning strategy.

For a protocol to be considered fully implemented the following must be included in the PR. Note how the Nessie protocol above only has completed the first 3 requirements. 
1. A new DiscoveryHandler implementation in the Akri Agent
1. An update to the Configuration CRD to include the new `ProtocolHandler`
1. A sample protocol broker for the new resource
1. A sample Configuration that uses the new protocol in the form of a Helm template and values
1. (Optional) A sample end application that utilizes the services exposed by the Configuration
1. Dockerfile[s] for broker [and sample app] and associated update to the [makefile](../build/akri-containers.mk)
1. Github workflow[s] for broker [and sample app] to build containers and push to Akri container repository
1. Documentation on how to use the new sample Configuration, like the [ONVIF Sample document](./onvif-sample.md)