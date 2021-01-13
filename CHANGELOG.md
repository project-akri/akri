# v0.1.5

## Announcing Akri v0.1.5!
Akri v0.1.5 is a pre-release of Akri.

To find out more about Akri, check out our [README](https://github.com/deislabs/akri/blob/v0.1.5/README.md) and start [contributing](https://github.com/deislabs/akri/blob/v0.1.5/docs/contributing.md) today!

## New Features
The v0.1.5 release introduces support for OPC UA discovery along with:

* End to end demo for discovering and utilizing OPC UA servers
* Sample anomaly detection application for OPC UA demo
* Sample OPC UA broker
* OPC UA certificate generator

View the [full change log](https://github.com/deislabs/akri/compare/v0.0.44...v0.1.5)

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
Check out our [roadmap](https://github.com/deislabs/akri/blob/v0.1.5/docs/roadmap.md) to see the features we are looking forward to!

## Release history
See [CHANGELOG.md](https://github.com/deislabs/akri/blob/v0.1.5/CHANGELOG.md) for more information on what changed in this and previous releases.

# v0.0.44

## Announcing Akri v0.0.44!
Akri v0.0.44 is a pre-release of Akri.

To find out more about Akri, check out our [README](https://github.com/deislabs/akri/blob/v0.0.44/README.md) and start [contributing](https://github.com/deislabs/akri/blob/v0.0.44/docs/contributing.md) today!

## New Features
The v0.0.44 release introduces a number of significant improvements!

* Enable Akri for armv7
* Create separate Helm charts for releases (akri) and merges (akri-dev)
* Parameterize Helm for udev beyond simple video scenario
* Expand udev discovery by supporting filtering by udev rules that look up the device hierarchy such as SUBSYSTEMS, ATTRIBUTES, DRIVERS, KERNELS, and TAGS
* Parameterize Helm for udev to allow security context
* Remove requirement for agent to execute in privileged container

View the [full change log](https://github.com/deislabs/akri/compare/v0.0.35...v0.0.44)

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
Check out our [roadmap](https://github.com/deislabs/akri/blob/v0.0.44/docs/roadmap.md) to see the features we are looking forward to!

## Release history
See [CHANGELOG.md](https://github.com/deislabs/akri/blob/v0.0.44/CHANGELOG.md) for more information on what changed in this and previous releases.


# v0.0.35

## Announcing the Akri v0.0.35 pre-release!
Akri v0.0.35 is the first pre-release of Akri.

To find out more about Akri, check out our [README](https://github.com/deislabs/akri/blob/main/README.md) and start [contributing](https://github.com/deislabs/akri/blob/main/docs/contributing.md) today!

## New Features
The v0.0.35 release introduces a number of significant features!

* CRDs to allow the discovery and utilization of leaf devices
* An agent and controller to find, advertise, and utilize leaf devices
* Discovery for IP cameras using the ONVIF protocol
* An ONVIF broker to serve the camera frames
* Discovery for leaf devices exposed through udev
* A udev camera broker to serve the camera frames
* A Helm chart to simplify Akri deployment

View the [full change log](https://github.com/deislabs/akri/commits/v0.0.35)

## Breaking Changes
N/A

## Known Issues
N/A

## What's next?
Check out our [roadmap](https://github.com/deislabs/akri/blob/main/docs/roadmap.md) to see the features we are looking forward to!

## Release history
See [CHANGELOG.md](https://github.com/deislabs/akri/blob/v0.0.35/CHANGELOG.md) for more information on what changed in this and previous releases.
