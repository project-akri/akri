# Extensibility: ZeroConf

See [Proposal: ZeroConf Protocol Implementation](https://github.com/DazWilkin/akri/blob/proposal-zeroconf/docs/proposals/zeroconf.md)

The implementation uses [`zeroconf`](https://crates.io/crates/zeroconf) but this is Linux-only. There's a proposal to swap `zeroconf` for [`astro-dnssd`](https://crates.io/crates/astro-dnssd) which supports Linux, Mac OS and Windows.

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

Revise `./agent/Cargo.toml`:

```TOML
[dependencies]
zeroconf = "0.6.2"
zeroconf-filter = { git = "https://github.com/DazWilkin/akri-pest" }
```

## Akri Configuration CRD

> **NOTE** After making this change you must `helm install` a copy of this directory not the deislabs/akri hosted

Revise `./deployment/helm/crds/akri-configuration-crd.yaml`:

```YAML
properties:
  zeroconf: # {{ZeroConfDiscoveryHandler}}
    type: object
    properties:
      filter: 
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

## Publish an mDNS Service

To ensure that you have a service for the Akri Agent to resolve, you may use the following:

```bash
NAME="freddie"
KIND="_http._tcp"
PORT="7777"

avahi-publish --service ${NAME} ${KIND} ${PORT}
```

## Confirm GitHub Packages for Akri

You may confirm that the agent, controller and http images were pushed to GitHub Container Registry by browsing:

https://github.com/[[GITHUB-USER]]?tab=packages

> **Important** check the tags of `agent` and `controller` to and ensure you reference the most recent version when you `helm install` these.

+ `https://github.com/users/[[GITHUB_USER]]/packages/container/agent/versions`
+ `https://github.com/users/[[GITHUB_USER]]/packages/container/controller/versions`


## Deploy Akri

> Optional: If you've previously installed Akri and wish to reset, you may:
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

> **NOTE** the Akri SemVer (e.g. 0.0.41) is reflected in ./version.txt but the tags must be prefixed with v and postfixed with the architecture (e.g. -amd64)

> **NOTE**
> You may wish to use `crictl` to confirm that the `agent` and `controller` images pulled match those in your repository
>
> ```bash
> sudo crictl \
> --runtime-endpoint unix:///var/snap/microk8s/common/run/containerd.sock \
> images
> ```

Check using `kubectl get pods` and look for a pod named `akri-agent-...` and another named `akri-controller...` and that they're both `RUNNING`.

Alternatively, you may:

```bash
kubectl get pods --selector=name=akri-agent
kubectl get pods --selector=app=akri-controller
```

## Deploy standalone ZeroConf Broker

```bash
kubectl apply --filename=./kubernetes/zeroconf.yaml
```

Check the agent's (!) logs:

```bash
[zeroconf:new] Entered
[zeroconf:discover] Entered
[zeroconf:discovery] Started browsing
[zeroconf:discovery:λ] Service Discovered: ServiceDiscovery { name: "freddie", kind: "_http._tcp", domain: "local", host_name: "akri.local", address: "", port: 9999, txt: None }
[zeroconf:discovery] Stopped browsing
[zeroconf:discovery] Iterating over services
[zeroconf:discovery] Service: ServiceDiscovery { name: "freddie", kind: "_http._tcp", domain: "local", host_name: "akri.local", address: "", port: 9999, txt: None }
```

You may check the Akri Configurations:

```bash
kubectl get configurations
NAME       CAPACITY   AGE
zeroconf   1          2m
```

You may describe this too: `kubectl describe configuration/zeroconf`

For each Service discovered, there should be an Akri instance created:

```bash
kubectl get instances
NAME              CONFIG     SHARED   NODES    AGE
zeroconf-e7f45d   zeroconf   true     [akri]   2m
```

You may describe this too: `kubectl describe instance/zeroconf-e7f45d`

And for each Instance, there should be a corresponding Pod with logs:

```bash
for INSTANCE in $(kubectl get instances --output=jsonpath="{.items[].metadata.name}")
do
  POD="pod/akri-${INSTANCE}-pod"
  kubectl logs ${POD}
done
```

Yields:

```console
[zeroconf:main] Entered
[zeroconf:new] Entered
[zeroconf:new]
  Kind: _http._tcp
  Name: freddie
  Host: akri.local
  Addr: 10.138.0.2
  Port: 9999
[zeroconf:main] Service: kind: _http._tcp
name: freddie
host: akri.local
addr: 10.138.0.2
port: 9999
[zeroconf:main:loop] Sleep
[zeroconf:main:loop] check_device(Service { kind: "_http._tcp", name: "freddie", host: "akri.local", addr: "10.138.0.2", port: 9999 })
[zeroconf:read_device] Entered: Service { kind: "_http._tcp", name: "freddie", host: "akri.local", addr: "10.138.0.2", port: 9999 }
[zeroconf:main:loop] Sleep
```

And, to confirm the environment available to a Pod:

```bash
kubectl exec --stdin --tty ${POD} -- env | grep ^AKRI
```

Yields:

```console
AKRI_ZEROCONF_DEVICE_KIND=_http._tcp
AKRI_ZEROCONF=zeroconf
AKRI_ZEROCONF_DEVICE_HOST=akri.local
AKRI_ZEROCONF_DEVICE_NAME=freddie
AKRI_ZEROCONF_DEVICE_PORT=9999
AKRI_ZEROCONF_DEVICE_ADDR=10.138.0.2
```

### CRDs

#### Configuration(s)

```bash
kubectl describe configuration/zeroconf
Name:         zeroconf
Namespace:    default
Labels:       <none>
Annotations:  API Version:  akri.sh/v0
Kind:         Configuration
Metadata:
  Managed Fields:
    API Version:  akri.sh/v0
    Fields Type:  FieldsV1
    fieldsV1:
      f:metadata:
        f:annotations:
          .:
          f:kubectl.kubernetes.io/last-applied-configuration:
      f:spec:
        .:
        f:brokerPodSpec:
          .:
          f:containers:
          f:imagePullSecrets:
        f:capacity:
        f:protocol:
          .:
          f:zeroconf:
            .:
            f:filter:
    Manager:         kubectl
    Operation:       Update
Spec:
  Broker Pod Spec:
    Containers:
      Image:  ghcr.io/dazwilkin/akri-zeroconf-broker@sha256:616a800d5754336229dad7b02c6f20e8511981195e6c5f89e2073ac660b17b4a
      Name:   zeroconf-broker
      Resources:
        Limits:
          {{PLACEHOLDER}}:  1
    Image Pull Secrets:
      Name:  ghcr
  Capacity:  1
  Protocol:
    Zeroconf:
      Filter:  kind="_http._tcp"
Events:        <none>
```

#### Instance(s)

```bash
kubectl describe instance/zeroconf-e7f45d
Name:         zeroconf-074bbf
Namespace:    default
Labels:       <none>
Annotations:  <none>
API Version:  akri.sh/v0
Kind:         Instance
Metadata:
  Managed Fields:
    API Version:  akri.sh/v0
    Fields Type:  FieldsV1
    fieldsV1:
      f:metadata:
        f:ownerReferences:
          .:
          k:{"uid":"b3de3379-855d-4336-8ebe-11b02686d0d2"}:
            .:
            f:apiVersion:
            f:blockOwnerDeletion:
            f:controller:
            f:kind:
            f:name:
            f:uid:
      f:spec:
        .:
        f:configurationName:
        f:deviceUsage:
          .:
          f:zeroconf-074bbf-0:
        f:metadata:
          .:
          f:AKRI_ZEROCONF:
          f:AKRI_ZEROCONF_DEVICE_ADDR:
          f:AKRI_ZEROCONF_DEVICE_HOST:
          f:AKRI_ZEROCONF_DEVICE_KIND:
          f:AKRI_ZEROCONF_DEVICE_NAME:
          f:AKRI_ZEROCONF_DEVICE_PORT:
        f:nodes:
        f:rbac:
        f:shared:
    Manager:    unknown
    Operation:  Update
  Owner References:
    API Version:           akri.sh/v0
    Block Owner Deletion:  true
    Controller:            true
    Kind:                  Configuration
    Name:                  zeroconf
    UID:                   b3de3379-855d-4336-8ebe-11b02686d0d2
  Resource Version:        318993
  Self Link:               /apis/akri.sh/v0/namespaces/default/instances/zeroconf-074bbf
  UID:                     52c6a861-67e4-4e97-b24f-c4f6287ae21e
Spec:
  Configuration Name:  zeroconf
  Device Usage:
    zeroconf-074bbf-0:  akri
  Metadata:
    AKRI_ZEROCONF:              zeroconf
    AKRI_ZEROCONF_DEVICE_ADDR:  10.138.0.2
    AKRI_ZEROCONF_DEVICE_HOST:  akri.local
    AKRI_ZEROCONF_DEVICE_KIND:  _http._tcp
    AKRI_ZEROCONF_DEVICE_NAME:  freddie
    AKRI_ZEROCONF_DEVICE_PORT:  9999
  Nodes:
    akri
  Rbac:    rbac
  Shared:  true
Events:    <none>
```

### crictl

```bash
sudo crictl \
--runtime-endpoint unix:///var/snap/microk8s/common/run/containerd.sock \
images
```

Perhaps:

```bash
sudo crictl \
--runtime-endpoint unix:///var/snap/microk8s/common/run/containerd.sock rmi \
ghcr.io/dazwilkin/agent:v0.0.41-amd64

sudo crictl \
--runtime-endpoint unix:///var/snap/microk8s/common/run/containerd.sock rmi \
ghcr.io/dazwilkin/controller:v0.0.41-amd64
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

Debugging Akri instances not being created:

```bash
kubectl get akric
NAME       CAPACITY   AGE
zeroconf   1          6s

kubectl get akrii
No resources found in default namespace.
```

Run the broker directly:

```bash
AKRI_ZEROCONF_DEVICE_KIND="_http._tcp" \
AKRI_ZEROCONF_DEVICE_NAME="hades-canyon" \
AKRI_ZEROCONF_DEVICE_HOST="hades-canyon.local" \
AKRI_ZEROCONF_DEVICE_ADDR="127.0.0.1" \
AKRI_ZEROCONF_DEVICE_PORT="8080" \
cargo run
```

Or:

```bash
docker run --interactive --tty --rm \
--env=AKRI_ZEROCONF_DEVICE_KIND="_http._tcp" \
--env=AKRI_ZEROCONF_DEVICE_NAME="hades-canyon" \
--env=AKRI_ZEROCONF_DEVICE_HOST="hades-canyon.local" \
--env=AKRI_ZEROCONF_DEVICE_ADDR="127.0.0.1" \
--env=AKRI_ZEROCONF_DEVICE_PORT="8080" \
ghcr.io/dazwilkin/akri-zeroconf-broker@sha256:a506722c43fb847a9cff9d5e81292ca99db71d03a9d3e37f4aa38c1ee80205dd


Or:

```bash
kubectl run test \
--image=ghcr.io/dazwilkin/akri-zeroconf-broker@sha256:a506722c43fb847a9cff9d5e81292ca99db71d03a9d3e37f4aa38c1ee80205dd \
--env=\
AKRI_ZEROCONF_DEVICE_KIND="_http._tcp",\
AKRI_ZEROCONF_DEVICE_NAME="hades-canyon",\
AKRI_ZEROCONF_DEVICE_HOST="hades-canyon.local",\
AKRI_ZEROCONF_DEVICE_ADDR="127.0.0.1",\
AKRI_ZEROCONF_DEVICE_PORT="8080"
```

#### References

+ [IANA Service:Protocol pairs](https://www.iana.org/assignments/service-names-port-numbers/service-names-port-numbers.xhtml)
+ [Kubernetes Supported protocols](https://kubernetes.io/docs/concepts/services-networking/service/#protocol-support)