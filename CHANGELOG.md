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
