# Extensibility: ZeroConf

+ https://en.wikipedia.org/wiki/Zero-configuration_networking
+ https://docs.rs/zeroconf/0.6.2/zeroconf/

## Dependencies

On Ubuntu:

```bash
apt install libclang-dev libavahi-client-dev
```

Aside:

```bash
journalctl --unit=avahi-daemon.service
```


## Cargo

Add `zeroconf` as a member of the project's workspace to facilitate its inclusion by [`rust-analyzer`](https://github.com/rust-analyzer/rust-analyzer):

```TOML
[workspace]
members = [
    ...
    "samples/brokers/zeroconf",
    ...
]
```

## Agent

Revise `./agent/src/protocols/mod.rs`:

```rust
mod zeroconf;
```

And:

```rust
fn inner_get_discovery_handler(
    discovery_handler_config: &ProtocolHandler,
    query: &impl EnvVarQuery,
) -> Result<Box<dyn DiscoveryHandler + Sync + Send>, Error> {
    match discovery_handler_config {
        ...
        ProtocolHandler::zeroconf(zeroconf)=>Ok(Box::new(zeroconf::ZeroConfDiscoverHandler::new(&zeroconf))),
        ...
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
zeroconf = "0.6.2"
```

## Akri Configuration CRD

> **NOTE** After making this change you must `helm install` a copy of this directory not the Microsoft-hosted

Revise `./deployment/helm/crds/akri-configuration-crd.yaml`:

```YAML
properties:
  zeroconf: # {{ZeroConfDiscoveryHandler}}
    type: object
    properties:
    filter: string
```

And:

```YAML
oneOf:
 - required: ["zeroconf"]
```

## Shared Configuration

Revise `./shared/src/akri/configuration.rs`:

```rust
pub enum ProtocolHandler {
    zeroconf(ZeroConfDiscoveryHandlerConfig),
    ...
}
```

And:

```rust
/// This defines the ZeroConf data stored in the Configuration
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ZeroConfDiscoveryHandlerConfig {
    pub filter: String,
}
```

## Docker

Revise `./dockerignore` to ensure `docker build ...` succeeds:

```console
# Don't ignore these
!samples/brokers/zeroconf
!shared/
```

## Revise workspace Cross file

Revise `./Cross.toml`:

```console
[target.x86_64-unknown-linux-gnu]
image = "ghcr.io/[[GITHUB-USER]]/rust-crossbuild:x86_64-unknown-linux-gnu-0.1.16-0.0.6"

[target.arm-unknown-linux-gnueabihf]
image = "ghcr.io/[[GITHUB-USER]]/rust-crossbuild:arm-unknown-linux-gnueabihf-0.1.16-0.0.6"

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

## Build|Push Broker (ZeroConf)

```bash
HOST="ghcr.io"
USER=[[GITHUB-USER]]
REPO="akri-zeroconf-broker"
TAG="..."

docker build \
--tag=${HOST}/${USER}/${REPO}:${TAG} \
--file=./samples/brokers/http/Dockerfile \
. && \
docker push ${HOST}/${USER}/${REPO}:${TAG}
```

## Confirm GitHub Packages

You may confirm that the agent, controller and http images were push to GitHub Container Registry by browsing:

https://github.com/[[GITHUB-USER]]?tab=packages

## Deploy Devices(s) & Discovery

(TODO)

## Revise Helm Deployment


## Deploy Akri


## Deploy standalone ZeroConf Broker

```bash
kubectl apply --filename=./kubernetes/zeroconf.yaml
```