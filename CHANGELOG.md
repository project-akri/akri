# v0.12.9

## Announcing Akri v0.12.9!
Akri v0.12.9 is a pre-release of Akri.

To find out more about Akri, check out our [documentation](https://docs.akri.sh/) and start
[contributing](https://docs.akri.sh/community/contributing) today!

## New Features
The v0.12.9 release contains the following changes:

1. **Configuration-level resource support** (https://github.com/project-akri/akri/pull/627). This implements the [proposal](https://github.com/project-akri/akri-docs/blob/main/proposals/configuration-level-resources.md#configuration-level-resources) of exposing resources at the Configuration level. This allows users to select resources to use at the configuration-level without having know the instance ID beforehand. 
2. **Support discoveryProperties in the Configuration** (https://github.com/project-akri/akri/pull/619). This implements the [proposal](https://github.com/project-akri/akri-docs/pull/61) for allowing users to specify credential data through the discoveryProperties section in the configuration. The Agent can read these properties and pass them to the discovery handlers for discovering authenticated devices. This provides a native K8s experience by supporting K8s Secrets and K8s configMaps. 
3. **Enable ONVIF discovery handler and broker to discover and access authenticated devices** (https://github.com/project-akri/akri/pull/638, https://github.com/project-akri/akri/pull/646). This allows the ONVIF discovery handler to discover IP cameras and other ONVIF devices that require authentication. Once authenticated devices are discovered using the credential data passed through, the broker can also access the data from these devices using the device UUID to look up credentials from the credential directories and authenticate the device.

**Fixes, features, and optimizations**
- opt: Improve udev testability and tests (https://github.com/project-akri/akri/pull/582)
- opt: Generate a self-signed certificate for webhook if none given in Helm (https://github.com/project-akri/akri/pull/588)
- fix: Update env_logger and clap to fix RUSTSEC-2021-0145 (https://github.com/project-akri/akri/pull/590)
- opt: Add protobuf-compiler to crossbuild intermediate containers (https://github.com/project-akri/akri/pull/594)
- opt: Add K8s 1.27 to test cases and remove 1.23 (https://github.com/project-akri/akri/pull/595)
- fix: Fix intermediate dotnet openCV image build (https://github.com/project-akri/akri/pull/596)
- opt: Force all Akri components to use the Linux node selector (https://github.com/project-akri/akri/pull/599)
- fix: Fix possible crash when modifying Configuration (https://github.com/project-akri/akri/pull/602)
- fix: Upgrade Docker images from Debian Buster to Bullseye (https://github.com/project-akri/akri/issues/606, https://github.com/project-akri/akri/pull/607, https://github.com/project-akri/akri/pull/609)
- fix: Fix discovery channels being stopped when Configurations are modified or deleted (https://github.com/project-akri/akri/pull/612)
- fix: Stop the controller from panicking when unable to create pod and output log instead (https://github.com/project-akri/akri/pull/615)
- fix: Correct ONVIF filtering (IP address, MAC address, scopes) (https://github.com/project-akri/akri/pull/622)
- opt: Revamp E2E test suite (https://github.com/project-akri/akri/pull/623)
- feat: Add udev E2E tests (https://github.com/project-akri/akri/pull/637)
- fix: Return discovered devices that fails to get IP/MAC address if IP/MAC filters are not used (https://github.com/project-akri/akri/pull/640)
- feat: Add UUID filter to ONVIF discovery handler (https://github.com/project-akri/akri/pull/641)
- opt: Remove unused Rust dependencies (https://github.com/project-akri/akri/pull/653)
- fix: Correct Agent's behavior on Instance deletion (https://github.com/project-akri/akri/pull/654)
- fix: Update dependencies of sample applications to fix security vulnerability (https://github.com/project-akri/akri/pull/660)


View the [full change log](https://github.com/project-akri/akri/compare/v0.10.4...v.0.12.9)

## Breaking Changes
1. With [the ONVIF discovery handler using UUID_serviceUrl as the device ID](https://github.com/project-akri/akri/pull/630) this changes the hash ID in the Akri instance of the discovered camera. Now, the ONVIF discovery handler always exposes the device UUID in device properties.
2. With [the IP/MAC address filtering in device properties becoming optional](https://github.com/project-akri/akri/pull/640), the ONVIF discovery handler only expose the IP/MAC addresses when it is able to get them from the device. Retrieving the IP/MAC address requires credentials, so if none are provided, the discovery handler reports the devices without exposing the IP/MAC addresses in properties. 

## Known Issues
N/A

## Validated With

| Distribution | Version |
|---|---|
| Kubernetes | v1.28.1 |
| Kubernetes | v1.27.5 |
| Kubernetes | v1.26.8 |
| Kubernetes | v1.25.13 |
| K3s | v1.28.1+k3s1 |
| K3s | v1.27.5+k3s1 |
| K3s | v1.26.8+k3s1 |
| K3s | v1.25.13+k3s1 |
| MicroK8s | 1.28/stable |
| MicroK8s | 1.27/stable |
| MicroK8s | 1.26/stable |
| MicroK8s | 1.25/stable |

## What's next?
Check out our [roadmap](https://docs.akri.sh/community/roadmap) to see the features we are looking forward to!

## Thanks 👏
Thank you everyone in the community who helped Akri get to this release! Your interest and contributions help Akri
prosper.

**⭐ Contributors to v0.12.9 ⭐**
- @johnsonshih
- @diconico07
- @jbpaux
- @kate-goldenring
- @bfjelds
- @yujinkim-msft
- @gauravgahlot
- @ag17sep

(Please send us (`@Kate Goldenring` or `@Yu Jin Kim`) a direct message on
  [Slack](https://kubernetes.slack.com/messages/akri) if we left you out!)

## Installation
Akri is packaged as a Helm chart. Check out our [installation doc](https://docs.akri.sh/user-guide/getting-started) on
how to install Akri.

```
helm repo add akri-helm-charts https://project-akri.github.io/akri/
helm install akri akri-helm-charts/akri --version 0.12.9 \
    # additional configuration
```

## Release history
See [CHANGELOG.md](https://github.com/project-akri/akri/blob/v0.12.9/CHANGELOG.md) for more information on what changed
in this and previous releases.


## Previous Releases:

# v0.10.4

## Announcing Akri v0.10.4!
Akri v0.10.4 is a pre-release of Akri.

To find out more about Akri, check out our [documentation](https://docs.akri.sh/) and start
[contributing](https://docs.akri.sh/community/contributing) today!

## New Features
The v0.10.4 release contains the following changes:

1. **Enable mounting connectivity information for multiple devices/instances in a Pod** (https://github.com/project-akri/akri/pull/560 , https://github.com/project-akri/akri/pull/561). Previously, Akri could only mount one device property per discovery handler to a Pod as all devices of the same discovery handler had the same environment variable name. This release fixes this issue by appending the instance hash to the environment variable name and slot ID to the annotation key name. This is a **breaking change** as it changes the way brokers look up properties.
2. **Enable udev discovery handler to discover multiple node devices** (https://github.com/project-akri/akri/pull/564). Akri now allows udev discovery handler to group devices that share a parent/child relation.
3. **Mount udev devpath in Akri brokers** (https://github.com/project-akri/akri/pull/534). This enables discovering udev devices without a devnode by using devpath instead. This is a **breaking change** in the udev discovery handler as it changes the way Akri creates instance ids for udev devices.
4. **Mount udev devices through DeviceSpec instead of Mounts** (https://github.com/project-akri/akri/pull/576). This switches from using Mounts to using DeviceSpec for device nodes, and exposes the desired permissions to non priviledged containers.

**Fixes, features, and optimizations**
- feat: Add nodeSelectors for Akri agent (https://github.com/project-akri/akri/pull/536)
- fix: Fix wrong indentation on udev-configuration.yaml for securityContext (https://github.com/project-akri/akri/pull/538)
- fix: ListAndWatch only sends device if the list has changed (https://github.com/project-akri/akri/pull/540)
- fix: Use tokio::sync::RwLock instead of tokio::sync::Mutex (https://github.com/project-akri/akri/pull/541)
- opt: Upgrade Ubuntu agent version (https://github.com/project-akri/akri/pull/546)
- opt: Added more securityContext to ensure Helm templates use the most restrictive setting (https://github.com/project-akri/akri/pull/547)
- opt: Update nodejs actions to node16 (https://github.com/project-akri/akri/pull/548)
- fix: Set pre_start_required to false in get_device_plugin_options (https://github.com/project-akri/akri/pull/554)
- fix: Modify Agent to reduce frequency of Pods getting UnexpectedAdmissionError (https://github.com/project-akri/akri/pull/556)
- fix: Specify crictl container runtime in e2e test workflow (https://github.com/project-akri/akri/pull/559)
- fix: Suffix usage slot to annotation key name (https://github.com/project-akri/akri/pull/560)
- fix: Added udev devnode to device mounts instead of devpath (https://github.com/project-akri/akri/pull/562)
- fix: Fixed watch crash API unreachable (https://github.com/project-akri/akri/pull/568)
- fix: Verify DiscoveryURL from OPC Server is resolvable (https://github.com/project-akri/akri/pull/570)
- opt: Update kubernetes versions in e2e test (https://github.com/project-akri/akri/pull/573)
- opt: Update rust to 1.68.1 and tarpaulin to 0.25.1 (https://github.com/project-akri/akri/pull/574)
- opt: Upgrade Rust CI actions to maintained ones (https://github.com/project-akri/akri/pull/581)
- opt: Update warp dependency to fix RUSTSEC-2023-0028 (https://github.com/project-akri/akri/pull/585)


View the [full change log](https://github.com/project-akri/akri/compare/v0.8.23...v0.10.4)

## Breaking Changes
1. With [enable mounting connectivity information for multiple devices/instances in a Pod](https://github.com/project-akri/akri/pull/561), Akri now changes the name of the device properties from `DEVICE_DESCRIPTION` to `DEVICE_DESCRIPTION_INSTANCE_HASH` to allow multiple device properties of the same discovery handler to be injected to the same broker. For example, a broker can look up the Akri instance `akri-debug-echo-foo-8120fe` by the environment variable `DEBUG_ECHO_DESCRIPTION_8120FE` instead of `DEBUG_ECHO_DESCRIPTION`.
2. With [Mount udev devpath in Akri broker](https://github.com/project-akri/akri/pull/534), Akri changes the way it creates udev Akri instance id from using the **hash of devnode** to using the **hash of devpath**

## Known Issues
N/A

## Validated With

| Distribution | Version |
|---|---|
| Kubernetes | v1.26.3 |
| Kubernetes | v1.25.8 |
| Kubernetes | v1.24.12 |
| Kubernetes | v1.23.15 |
| K3s | v1.26.3+k3s1 |
| K3s | v1.25.8+k3s1 |
| K3s | v1.24.12+k3s1 |
| K3s | v1.23.15+k3s1 |
| MicroK8s | 1.26/stable |
| MicroK8s | 1.25/stable |
| MicroK8s | 1.24/stable |
| MicroK8s | 1.23/stable |

## What's next?
Check out our [roadmap](https://docs.akri.sh/community/roadmap) to see the features we are looking forward to!

## Thanks 👏
Thank you everyone in the community who helped Akri get to this release! Your interest and contributions help Akri
prosper.

**⭐ Contributors to v0.10.4 ⭐**
- @harrison-tin
- @adithyaj
- @kate-goldenring
- @johnsonshih
- @diconico07
- @jbpaux
- @yujinkim-msft
- @koutselakismanos

(Please send us (`@Kate Goldenring` or `@Adithya J`) a direct message on
  [Slack](https://kubernetes.slack.com/messages/akri) if we left you out!)

## Installation
Akri is packaged as a Helm chart. Check out our [installation doc](https://docs.akri.sh/user-guide/getting-started) on
how to install Akri.

```
helm repo add akri-helm-charts https://project-akri.github.io/akri/
helm install akri akri-helm-charts/akri --version 0.10.4 \
    # additional configuration
```

## Release history
See [CHANGELOG.md](https://github.com/project-akri/akri/blob/v0.10.4/CHANGELOG.md) for more information on what changed
in this and previous releases.


# v0.8.23

## Announcing Akri v0.8.23!
Akri v0.8.23 is a pre-release of Akri.

To find out more about Akri, check out our [documentation](https://docs.akri.sh/) and start
[contributing](https://docs.akri.sh/community/contributing) today!

## New Features
The v0.8.23 release contains the following changes:

1. Akri uses containerd as the default container runtime.
2. Enables secrets, configMaps, and volumes to be mounted with helm templates.
3. Support for latest kubernetes versions.

**Fixes, features, and optimizations**
- opt: Update OPCUA to 0.11.0 to remove vulnerabilities (https://github.com/project-akri/akri/pull/528)
- feat: GitHub Action to auto-version update (https://github.com/project-akri/akri/pull/510)
- fix: Fixed Kubernetes tests to run on active branches (https://github.com/project-akri/akri/pull/513)
- fix: Fix uds gRPC client implementation with C based gRPC (https://github.com/project-akri/akri/pull/498)
- opt: Removed unmaintained ansi_term dependency (https://github.com/project-akri/akri/pull/506)
- opt: Rust toolchain updates (https://github.com/project-akri/akri/pull/482)(https://github.com/project-akri/akri/pull/507)
- feat: Enable secrets in helm templates (https://github.com/project-akri/akri/pull/478)

View the [full change log](https://github.com/project-akri/akri/compare/v0.8.4...v0.8.23)

## Breaking Changes
N/A

## Known Issues
N/A

## Validated With

| Distribution | Version |
|---|---|
| Kubernetes | v1.25.1 |
| Kubernetes | v1.24.5 |
| Kubernetes | v1.23.11 |
| Kubernetes | v1.22.14 |
| Kubernetes | v1.21.14 |
| K3s | v1.25.2+k3s1 |
| K3s | v1.24.6+k3s1 |
| K3s | v1.23.12+k3s1 |
| K3s | v1.22.6+k3s1 |
| K3s | v1.21.5+k3s1 |
| MicroK8s | 1.24/stable |
| MicroK8s | 1.23/stable |
| MicroK8s | 1.22/stable |
| MicroK8s | 1.21/stable |

## What's next?
Check out our [roadmap](https://docs.akri.sh/community/roadmap) to see the features we are looking forward to!

## Thanks 👏
Thank you everyone in the community who helped Akri get to this release! Your interest and contributions help Akri
prosper.

**⭐ Contributors to v0.8.4 ⭐**
- @adithyaj
- @bfjelds
- @bitmeal
- @karok2m
- @kate-goldenring
- @Ragnyll
- @Rishit-dagli
- @romoh

(Please send us (`@Kate Goldenring` or `@Adithya J`) a direct message on
  [Slack](https://kubernetes.slack.com/messages/akri) if we left you out!)

## Installation
Akri is packaged as a Helm chart. Check out our [installation doc](https://docs.akri.sh/user-guide/getting-started) on
how to install Akri.

```
helm repo add akri-helm-charts https://project-akri.github.io/akri/
helm install akri akri-helm-charts/akri --version 0.8.23 \
    # additional configuration
```

## Release history
See [CHANGELOG.md](https://github.com/project-akri/akri/blob/v0.8.23/CHANGELOG.md) for more information on what changed
in this and previous releases.


## Previous Releases:

# v0.8.4

## Announcing Akri v0.8.4!
Akri v0.8.4 is a pre-release of Akri.

To find out more about Akri, check out our [documentation](https://docs.akri.sh/) and start
[contributing](https://docs.akri.sh/community/contributing) today!

## New Features
The v0.8.4 release contains the following major changes: 

1. **Support for Kubernetes Job brokers** (https://github.com/project-akri/akri/pull/437). Now Akri has support for deploying Jobs to devices discovered by the Akri Agent. Previously, Akri only supported deploying Pods that were not intended to terminate (and would be restarted if they did). Adding Jobs enables more device use scenarios. More background can be found in the [Jobs proposal](https://github.com/project-akri/akri-docs/blob/main/proposals/job-brokers.md). This is a **breaking change** as it required changes to Akri's Configuration CRD.
2. Fix to re-enable **applying multiple Configurations that use the same Discovery Handler** (https://github.com/project-akri/akri/pull/432). This adds back functionality that was removed in `v0.6.5` when enabling Akri's new extensibility model. 
3. Akri depends on `crictl` to track whether Pods deployed by the Akri Controller are still running. This release adds new functionality (https://github.com/project-akri/akri/pull/418) such that **crictl is pre-installed in the Agent container**  so that it does not need to be installed on each node.

**Fixes, features, and optimizations**
- fix: Make debug echo capacity configurable (https://github.com/project-akri/akri/pull/419)
- fix: Return okay if get 404 when trying to delete an Instance (https://github.com/project-akri/akri/pull/420)
- opt: Update .NET dependencies, removing vulnerabilities and reducing size (https://github.com/project-akri/akri/pull/422)
- feat: Execute test workloads based on labels instead of flags in PR titles (https://github.com/project-akri/akri/pull/426)
- opt: Set K8s distribution with Helm to simplify choosing container runtime socket (https://github.com/project-akri/akri/pull/427)
- fix: Fix all clippy errors and update dependency versions (https://github.com/project-akri/akri/pull/442)

View the [full change log](https://github.com/project-akri/akri/compare/v0.7.0...v0.8.4)

## Breaking Changes
Akri's Configuration CRD has been updated to support Job brokers. If Akri has previously been installed on a cluster, delete the previous Configuration CRD before installing the latest version of Akri:

```sh
kubectl delete crd configurations.akri.sh
```

## Known Issues
N/A

## Validated With

| Distribution | Version |
|---|---|
| Kubernetes | v1.21.0 |
| Kubernetes | v1.20.1 |
| Kubernetes | v1.19.4 |
| Kubernetes | v1.18.12 |
| Kubernetes | v1.17.14 |
| Kubernetes | v1.16.15 |
| K3s | v1.22.6+k3s1 |
| K3s | v1.21.5+k3s1 |
| K3s | v1.20.6+k3s1 |
| K3s | v1.19.10+k3s1 |
| K3s | v1.18.9+k3s1 |
| K3s | v1.17.17+k3s1 |
| K3s | v1.16.14+k3s1 |
| MicroK8s | 1.23/stable |
| MicroK8s | 1.22/stable |
| MicroK8s | 1.21/stable |
| MicroK8s | 1.20/stable |
| MicroK8s | 1.19/stable |
| MicroK8s | 1.18/stable |
| MicroK8s | 1.17/stable |
| MicroK8s | 1.16/stable |

## What's next?
Check out our [roadmap](https://docs.akri.sh/community/roadmap) to see the features we are looking forward to!

## Thanks 👏
Thank you everyone in the community who helped Akri get to this release! Your interest and contributions help Akri
prosper. 

**⭐ Contributors to v0.8.4 ⭐**
- @bfjelds
- @kate-goldenring
- @romoh
- @vincepnguyen
- @Ragnyll
- (Please send us (`@Kate Goldenring` or `@Edrick Wong`) a direct message on
  [Slack](https://kubernetes.slack.com/messages/akri) if we left you out!)

## Installation
Akri is packaged as a Helm chart. Check out our [installation doc](https://docs.akri.sh/user-guide/getting-started) on
how to install Akri.

```
helm repo add akri-helm-charts https://project-akri.github.io/akri/
helm install akri akri-helm-charts/akri --version 0.8.4 \
    # additional configuration
```

## Release history
See [CHANGELOG.md](https://github.com/project-akri/akri/blob/v0.8.4/CHANGELOG.md) for more information on what changed
in this and previous releases.

# v0.7.0

## Announcing Akri v0.7.0!
Akri v0.7.0 is a pre-release of Akri.

To find out more about Akri, check out our [documentation](https://docs.akri.sh/) and start
[contributing](https://docs.akri.sh/community/contributing) today!

## New Features
The v0.7.0 release marks the first release of Akri in a new `project-akri` GitHub organization. While no
breaking changes were introduced, Akri's minor version was bumped to clearly mark this transition of Akri to a [Cloud
Native Computing Foundation (CNCF) Sandbox project](https://www.cncf.io/sandbox-projects/). 

This release also introduces:
- [Open governance](https://github.com/opengovernance/opengovernance.dev)
  [documentation](https://github.com/project-akri/akri/blob/v0.7.0/GOVERNANCE.md)
- The switch from MIT to Apache 2 license (https://github.com/project-akri/akri/pull/401)
- The introduction of the Linux Foundation (LF) Core Infrastructure Initiative (CII) Best Practices badge on Akri's
  README (https://github.com/project-akri/akri/pull/403)
- The enablement of a [Developer Certificate of Origin (DCO)](https://github.com/apps/dco) of pull requests, which
  requires requires all commit messages to contain the Signed-off-by line with an email address that matches the commit
  author.

View the [full change log](https://github.com/project-akri/akri/compare/v0.6.19...v0.7.0)

## Breaking Changes
N/A

## Known Issues
A [Rust security issue](https://github.com/project-akri/akri/issues/398) was raised on the `time` crate, which is used
ultimately by Akri's `k8s-openapi`, `kube-rs` and `opcua-client` dependencies via `chrono`. It appears that the version
of `time` that `chrono` is using is [not
vulnerable](https://github.com/kube-rs/kube-rs/issues/650#issuecomment-940435726). This
[issue](https://github.com/project-akri/akri/issues/398) tracks the progress on `chrono` and Akri's dependencies.

## Validated With

| Distribution | Version |
|---|---|
| Kubernetes | v1.21.0 |
| Kubernetes | v1.20.1 |
| Kubernetes | v1.19.4 |
| Kubernetes | v1.18.12 |
| Kubernetes | v1.17.14 |
| Kubernetes | v1.16.15 |
| K3s | v1.21.5+k3s1 |
| K3s | v1.20.6+k3s1 |
| K3s | v1.19.10+k3s1 |
| K3s | v1.18.9+k3s1 |
| K3s | v1.17.17+k3s1 |
| K3s | v1.16.14+k3s1 |
| MicroK8s | 1.21/stable |
| MicroK8s | 1.20/stable |
| MicroK8s | 1.19/stable |
| MicroK8s | 1.18/stable |
| MicroK8s | 1.17/stable |
| MicroK8s | 1.16/stable |

## What's next?
Check out our [roadmap](https://docs.akri.sh/community/roadmap) to see the features we are looking forward to!

## Thanks
Thank you everyone in the community who helped Akri get to this release! You're interest and contributions help Akri
prosper. 

**Contributors to v0.7.0**
- @bfjelds
- @kate-goldenring
- @romoh
- @edrickwong
- (Please send us (`@Kate Goldenring` or `@Edrick Wong`) a direct message on
  [Slack](https://kubernetes.slack.com/messages/akri) if we left you out!)

## Installation
Akri is packaged as a Helm chart. Check out our [installation doc](https://docs.akri.sh/user-guide/getting-started) on
how to install Akri.

```
helm repo add akri-helm-charts https://project-akri.github.io/akri/
helm install akri akri-helm-charts/akri --version 0.7.0 \
    # additional configuration
```

## Release history
See [CHANGELOG.md](https://github.com/project-akri/akri/blob/v0.7.0/CHANGELOG.md) for more information on what changed
in this and previous releases.

# v0.6.19

## Announcing Akri v0.6.19!
Akri v0.6.19 is a pre-release of Akri.

To find out more about Akri, check out our [documentation](https://docs.akri.sh/) and start [contributing](https://docs.akri.sh/community/contributing) today!

## New Features
The v0.6.19 release features **ONVIF Discovery Handler and broker optimizations**, long-awaited runtime and Kubernetes **dependency updates**, and moves Akri's documentation to a [**docs repository**](https://github.com/project-akri/akri-docs).

**Fixes, features, and optimizations**
* opt: Updated Akri's runtime (`tokio`) and Kubernetes dependencies (`kube-rs` and `k8s-openapi`), along with the major versions of all other dependencies where possible. (https://github.com/project-akri/akri/pull/361)
* opt: ONVIF Discovery handler optimized to be more performant (https://github.com/project-akri/akri/pull/351)
* opt: Reduced size of ONVIF broker by decreasing size of OpenCV container (https://github.com/project-akri/akri/pull/353)
* feat: Removed documentation from repository (https://github.com/project-akri/akri/pull/360) and placed in [`project-akri/akri-docs`](https://github.com/project-akri/akri-docs). Created documentation [site](https://docs.akri.sh/) that points to documentation repository. 
* feat: Workflow to mark inactive issues/PRs as stale and eventually close them (https://github.com/project-akri/akri/pull/363)
* fix: Make Discovery Handlers check channel health each discovery loop (https://github.com/project-akri/akri/pull/385)
* fix: Handle multicast response duplicates in ONVIF Discovery Handler (https://github.com/project-akri/akri/pull/393)
* fix: Use `kube-rs` resource `watcher` instead of `Api::watch` (https://github.com/project-akri/akri/pull/378)
* fix: Prevent re-creation instances when only Configuration metadata or status changes (https://github.com/project-akri/akri/pull/373)
* feat: Enable configuring Prometheus metrics port for local runs (https://github.com/project-akri/akri/pull/377)

View the [full change log](https://github.com/project-akri/akri/compare/v0.6.5...v0.6.19)

## Breaking Changes
N/A

## Known Issues
ONVIF discovery does not work in development versions `v0.6.17` and `v0.6.18` due to (https://github.com/project-akri/akri/pull/382). Issue was resolved for `v0.6.19` and beyond in (https://github.com/project-akri/akri/pull/393).

## Validated With

| Distribution | Version |
|---|---|
| Kubernetes | v1.21.0 |
| Kubernetes | v1.20.1 |
| Kubernetes | v1.19.4 |
| Kubernetes | v1.18.12 |
| Kubernetes | v1.17.14 |
| Kubernetes | v1.16.15 |
| K3s | v1.21.5+k3s1 |
| K3s | v1.20.6+k3s1 |
| K3s | v1.19.10+k3s1 |
| K3s | v1.18.9+k3s1 |
| K3s | v1.17.17+k3s1 |
| K3s | v1.16.14+k3s1 |
| MicroK8s | 1.21/stable |
| MicroK8s | 1.20/stable |
| MicroK8s | 1.19/stable |
| MicroK8s | 1.18/stable |
| MicroK8s | 1.17/stable |
| MicroK8s | 1.16/stable |

## What's next?
Check out our [roadmap](https://docs.akri.sh/community/roadmap) to see the features we are looking forward to!

## Thanks
Thank you everyone in the community who helped Akri get to this release! You're interest and contributions help Akri prosper. 

**Contributors to v0.6.19**
- @ammmze
- @bfjelds
- @kate-goldenring
- @romoh
- @shantanoo-desai
- (Please let us know via [Slack](https://kubernetes.slack.com/messages/akri) if we left you out!)

## Installation
Akri is packaged as a Helm chart. Check out our [installation doc](https://docs.akri.sh/user-guide/getting-started) on how to install Akri.

```
helm repo add akri-helm-charts https://project-akri.github.io/akri/
helm install akri akri-helm-charts/akri --version 0.6.19 \
    # additional configuration
```

## Release history
See [CHANGELOG.md](https://github.com/project-akri/akri/blob/v0.6.19/CHANGELOG.md) for more information on what changed in this and previous releases.

# v0.6.5

## Announcing Akri v0.6.5!
Akri v0.6.5 is a pre-release of Akri.

To find out more about Akri, check out our [README](https://github.com/project-akri/akri/blob/v0.6.5/README.md) and start [contributing](https://github.com/project-akri/akri/blob/v0.6.5/docs/contributing.md) today!

## New Features
The v0.6.5 release introduces Akri's Logo, new features such as a new extensibility model for Discovery Handlers and a Configuration validating webhook, DevOps improvements, and more.

**New Discovery Handler extensibility model**
* feat: Discovery Handlers now live behind a [gRPC interface](https://github.com/project-akri/akri/blob/v0.6.5/discovery-utils/proto/discovery.proto) (https://github.com/project-akri/akri/pull/252), so Discovery Handlers can be written in any language without forking Akri and working within its code. See the [Discovery Handler development document] to get started creating a Discovery Handler. 
* feat: Support of both default "slim" and old "full" Agent images (https://github.com/project-akri/akri/pull/279). Prior to this release, the Agent contained udev, ONVIF, and OPC UA Discovery Handlers. As of this release, Akri is moving towards a default of having no embedded Discovery Handlers in the Agent; rather, the desired Discovery Handlers can be deployed separately using Akri's Helm chart. This decreases the attack surface of the Agent and will keep it from exponential growth as new Discovery Handlers are continually supported. Discovery Handlers written in Rust can be conditionally compiled into the Agent -- reference [the development documentation for more details](https://github.com/project-akri/akri/blob/v0.6.5/docs/development.md#local-builds-and-tests). For the time being, Akri will continue to support a an Agent image with udev, ONVIF, and OPC UA Discovery Handlers. It will be used if `agent.full=true` is set when installing Akri's Helm chart.
* feat: Updates to Akri's Helm charts with templates for Akri's Discovery Handlers and renaming of values to better fit the new model.

DevOps improvements
* feat: Workflow to auto-update dependencies (https://github.com/project-akri/akri/pull/224)
* feat: Security audit workflow (https://github.com/project-akri/akri/pull/264)
* feat: Workflow for canceling previously running workflows on PRs, reducing environmental footprint and queuing of GitHub Actions (https://github.com/project-akri/akri/pull/284) 
* feat: Build all rust components in one workflow instead of previous strategy for a workflow for each build (https://github.com/project-akri/akri/pull/270)
* fix: More exhaustive linting of Akri Helm charts (https://github.com/project-akri/akri/pull/306)

Other enhancements
* feat: [**Webhook for validating Configurations**](https://github.com/project-akri/akri/blob/v0.6.5/webhooks/validating/configuration/README.md) (https://github.com/project-akri/akri/pull/206)
* feat: Support for Akri monitoring via Prometheus (https://github.com/project-akri/akri/pull/190)

Misc 
* feat: **Akri Logo** (https://github.com/project-akri/akri/pull/149)
* fix: Allow overwriting Controller's `nodeSelectors` (https://github.com/project-akri/akri/pull/194)
* fix: Updated `mockall` version (https://github.com/project-akri/akri/pull/214)
* fix: Changed default image `PullPolicy` from `Always` to Kubernetes default (`IfNotPresent`) (https://github.com/project-akri/akri/pull/207)
* fix: Improved video streaming application (for udev demo) that polls for new service creation (https://github.com/project-akri/akri/pull/173)
* fix: Patched anomaly detection application (for OPC UA demo) to show values from all brokers (https://github.com/project-akri/akri/pull/229)
* feat: Timestamped labels for local container builds (https://github.com/project-akri/akri/pull/234)
* fix: Removed udev directory mount from Agent DaemonSet (https://github.com/project-akri/akri/pull/304)
* fix: Modified Debug Echo Discovery Handler to specify `Device.properties` and added check to e2e tests (https://github.com/project-akri/akri/pull/288)
* feat: Support for specifying environment variables broker Pods via a Configuration's `brokerProperties`.
* fix: Default memory and CPU resource requests and limits for Akri containers (https://github.com/project-akri/akri/pull/305) 

View the [full change log](https://github.com/project-akri/akri/compare/v0.1.5...v0.6.5)

## Breaking Changes
Akri's Configuration and Instance CRDs were modified. The old version of the CRDs should be deleted with `kubectl delete instances.akri.sh configurations.akri.sh`, and the new ones will be applied with a new Akri Helm installation.
* Akri's Configuration CRD's `protocol` field was replaced with `discoveryHandler` in order to fit Akri's new Discovery Handler extensibility model and make the Configuration no longer strongly tied to Discovery Handlers. It's unused `units` field was removed and `properties` was renamed `brokerProperties` to be more descriptive. 
* Akri's Instance CRD's unused `rbac` field was removed and `metadate` was renamed `brokerProperties` to be more descriptive and aligned with the Configuration CRD.

Significant changes were made to Akri's Helm chart. Consult the latest user guide and Configurations documentation.

By default, the Agent contains no Discovery Handlers. To deploy Discovery Handlers, they must be explicitly enabled in Akri's Helm chart.

## Known Issues
N/A

## Validated With

| Distribution | Version |
|---|---|
| Kubernetes | v1.21.0 |
| Kubernetes | v1.20.1 |
| Kubernetes | v1.19.4 |
| Kubernetes | v1.18.12 |
| Kubernetes | v1.17.14 |
| Kubernetes | v1.16.15 |
| K3s | v1.20.6+k3s1 |
| K3s | v1.19.10+k3s1 |
| K3s | v1.18.9+k3s1 |
| K3s | v1.17.17+k3s1 |
| K3s | v1.16.14+k3s1 |
| MicroK8s | 1.21/stable |
| MicroK8s | 1.20/stable |
| MicroK8s | 1.19/stable |
| MicroK8s | 1.18/stable |
| MicroK8s | 1.17/stable |
| MicroK8s | 1.16/stable |

## What's next?
Check out our [roadmap](https://github.com/project-akri/akri/blob/v0.6.5/docs/roadmap.md) to see the features we are looking forward to!

## Release history
See [CHANGELOG.md](https://github.com/project-akri/akri/blob/v0.6.5/CHANGELOG.md) for more information on what changed in this and previous releases.
# v0.1.5

## Announcing Akri v0.1.5!
Akri v0.1.5 is a pre-release of Akri.

To find out more about Akri, check out our [README](https://github.com/project-akri/akri/blob/v0.1.5/README.md) and start [contributing](https://github.com/project-akri/akri/blob/v0.1.5/docs/contributing.md) today!

## New Features
The v0.1.5 release introduces support for OPC UA discovery along with:

* End to end demo for discovering and utilizing OPC UA servers
* Sample anomaly detection application for OPC UA demo
* Sample OPC UA broker
* OPC UA certificate generator

View the [full change log](https://github.com/project-akri/akri/compare/v0.0.44...v0.1.5)

## Breaking Changes
N/A

## Known Issues
N/A

## Validated With

| Distribution | Version |
|---|---|
| Kubernetes | v1.20.1 |
| Kubernetes | v1.19.4 |
| Kubernetes | v1.18.12 |
| Kubernetes | v1.17.14 |
| Kubernetes | v1.16.15 |
| K3s | v1.20.0+k3s2 |
| K3s | v1.19.4+k3s1 |
| K3s | v1.18.9+k3s1 |
| MicroK8s | 1.20/stable |
| MicroK8s | 1.19/stable |
| MicroK8s | 1.18/stable |

## What's next?
Check out our [roadmap](https://github.com/project-akri/akri/blob/v0.1.5/docs/roadmap.md) to see the features we are looking forward to!

## Release history
See [CHANGELOG.md](https://github.com/project-akri/akri/blob/v0.1.5/CHANGELOG.md) for more information on what changed in this and previous releases.

# v0.0.44

## Announcing Akri v0.0.44!
Akri v0.0.44 is a pre-release of Akri.

To find out more about Akri, check out our [README](https://github.com/project-akri/akri/blob/v0.0.44/README.md) and start [contributing](https://github.com/project-akri/akri/blob/v0.0.44/docs/contributing.md) today!

## New Features
The v0.0.44 release introduces a number of significant improvements!

* Enable Akri for armv7
* Create separate Helm charts for releases (akri) and merges (akri-dev)
* Parameterize Helm for udev beyond simple video scenario
* Expand udev discovery by supporting filtering by udev rules that look up the device hierarchy such as SUBSYSTEMS, ATTRIBUTES, DRIVERS, KERNELS, and TAGS
* Parameterize Helm for udev to allow security context
* Remove requirement for agent to execute in privileged container

View the [full change log](https://github.com/project-akri/akri/compare/v0.0.35...v0.0.44)

## Breaking Changes
N/A

## Known Issues
* Documented Helm settings are not currently compatible with K3s v1.19.4+k3s1

## Validated With

| Distribution | Version |
|---|---|
| Kubernetes | v1.19.4 |
| K3s | v1.18.9+k3s1 |
| MicroK8s | 1.18/stable |

## What's next?
Check out our [roadmap](https://github.com/project-akri/akri/blob/v0.0.44/docs/roadmap.md) to see the features we are looking forward to!

## Release history
See [CHANGELOG.md](https://github.com/project-akri/akri/blob/v0.0.44/CHANGELOG.md) for more information on what changed in this and previous releases.


# v0.0.35

## Announcing the Akri v0.0.35 pre-release!
Akri v0.0.35 is the first pre-release of Akri.

To find out more about Akri, check out our [README](https://github.com/project-akri/akri/blob/main/README.md) and start [contributing](https://github.com/project-akri/akri/blob/main/docs/contributing.md) today!

## New Features
The v0.0.35 release introduces a number of significant features!

* CRDs to allow the discovery and utilization of leaf devices
* An agent and controller to find, advertise, and utilize leaf devices
* Discovery for IP cameras using the ONVIF protocol
* An ONVIF broker to serve the camera frames
* Discovery for leaf devices exposed through udev
* A udev camera broker to serve the camera frames
* A Helm chart to simplify Akri deployment

View the [full change log](https://github.com/project-akri/akri/commits/v0.0.35)

## Breaking Changes
N/A

## Known Issues
N/A

## What's next?
Check out our [roadmap](https://github.com/project-akri/akri/blob/main/docs/roadmap.md) to see the features we are looking forward to!

## Release history
See [CHANGELOG.md](https://github.com/project-akri/akri/blob/v0.0.35/CHANGELOG.md) for more information on what changed in this and previous releases.
