# Extensibility: ZeroConf

+ https://en.wikipedia.org/wiki/Zero-configuration_networking
+ https://docs.rs/zeroconf/0.6.2/zeroconf/

## Dependencies

On Debian|Ubuntu:

Some variant of:

```bash
apt-get install avahi-daemon avahi-discover libnss-mdns
```

And:

```console
xorg-dev libxcb-shape0-dev libxcb-xfixes0-dev llvm-dev libclang-3.9-dev clang libavahi-client-dev \
```

Aside:

Either:

```bash
systemctl status avahi-daemon
● avahi-daemon.service - Avahi mDNS/DNS-SD Stack
     Loaded: loaded (/lib/systemd/system/avahi-daemon.service; enabled; vendor preset: enabled)
     Active: active (running) since Thu 2020-11-19 18:39:17 UTC; 2min 2s ago
TriggeredBy: ● avahi-daemon.socket
   Main PID: 47322 (avahi-daemon)
     Status: "avahi-daemon 0.7 starting up."
      Tasks: 2 (limit: 9544)
     Memory: 1.0M
     CGroup: /system.slice/avahi-daemon.service
             ├─47322 avahi-daemon: running [akri.local]
             └─47323 avahi-daemon: chroot helper
```

Or:

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
      filter: 
        type: string
      kind: string
        type: string
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

## Revise Agent Dockerfile

Revise `./build/containers/Dockerfile.agent`:

```Dockerfile
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    libssl-dev \
    xorg-dev libxcb-shape0-dev libxcb-xfixes0-dev llvm-dev libclang-3.9-dev clang libavahi-client-dev avahi-daemon \
    openssl \
    && \
    apt-get clean
```

And:

```Dockerfile
ENTRYPOINT ["bash", "-c", "service dbus start && service avahi-daemon start && ./agent"]
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
--file=./samples/brokers/zeroconf/Dockerfiles/standalone \
. && \
docker push ${HOST}/${USER}/${REPO}:${TAG}
```

## Deploy ZeroConf Prover

```bash
git clone ...
```

Then apply it to the cluster:

```bash
kubectl run ....
```

## Confirm GitHub Packages

You may confirm that the agent, controller and http images were pushed to GitHub Container Registry by browsing:

https://github.com/[[GITHUB-USER]]?tab=packages

> **Important** check the tags of `agent` and `controller` to and ensure you reference the most recent version when you `helm install` these.

+ `https://github.com/users/[[GITHUB_USER]]/packages/container/agent/versions`
+ `https://github.com/users/[[GITHUB_USER]]/packages/container/controller/versions`


## Deploy Devices(s) & Discovery

(TODO)

## Revise Helm Deployment


## Deploy Akri

If desired, delete Akri before recreating:

```bash
# Delete Akri Helm
sudo microk8s.helm3 uninstall akri

# Delete Akri CRDs
kubectl delete crd/configurations.akri.sh
kubectl delete crd/instances.akri.sh
```

Then:

```bash
HOST="ghcr.io"
USER="[[GITHUB-USER]]"
REPO="${HOST}/${USER}"

SECRET="..."
VERS="v0.0.41-amd64"

sudo microk8s.helm3 install akri ./akri/deployment/helm \
--set imagePullSecrets[0].name="${SECRET}" \
--set agent.image.repository="${REPO}/agent" \
--set agent.image.tag="${VERS}" \
--set controller.image.repository="${REPO}/controller" \
--set controller.image.tag="${VERS}"
```

## Deploy standalone ZeroConf Broker

```bash
kubectl apply --filename=./kubernetes/zeroconf.yaml
```


### crictl

```bash
sudo crictl \
--runtime-endpoint unix:///var/snap/microk8s/common/run/containerd.sock \
images
```

### Programming Notes


Agent requires `avahi-daemon` and the convoluted startup: `service start dbus ...`

But:

```console
[zeroconf:new] Entered
[zeroconf::discover] Entered
[zeroconf:discovery] Started browsing
[zeroconf:discovery:λ] Service Discovered: ServiceDiscovery { name: "prove-zeroconf-6658dd567-8wml7", kind: "_http._tcp", domain: "local", host_name: "prove-zeroconf-6658dd567-8wml7.local", address: "", port: 8080, txt: Some(AvahiTxtRecord(UnsafeCell)) }
[zeroconf:discovery:λ] Service Discovered: ServiceDiscovery { name: "prove-zeroconf-6658dd567-8wml7", kind: "_http._tcp", domain: "local", host_name: "prove-zeroconf-6658dd567-8wml7.local", address: "10.1.1.43", port: 8080, txt: Some(AvahiTxtRecord(UnsafeCell)) }
[zeroconf:discovery] Stopped browsing
[zeroconf:discovery] Iterating over services
[zeroconf:discovery] Service: ServiceDiscovery { name: "prove-zeroconf-6658dd567-8wml7", kind: "_http._tcp", domain: "local", host_name: "prove-zeroconf-6658dd567-8wml7.local", address: "", port: 8080, txt: Some(AvahiTxtRecord(UnsafeCell)) }
[zeroconf:discovery] Service: ServiceDiscovery { name: "prove-zeroconf-6658dd567-8wml7", kind: "_http._tcp", domain: "local", host_name: "prove-zeroconf-6658dd567-8wml7.local", address: "10.1.1.43", port: 8080, txt: Some(AvahiTxtRecord(UnsafeCell)) }
```

#### References

+ [IANA Service:Protocol pairs](https://www.iana.org/assignments/service-names-port-numbers/service-names-port-numbers.xhtml)
+ [Kubernetes Supported protocols](https://kubernetes.io/docs/concepts/services-networking/service/#protocol-support)