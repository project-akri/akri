# [ZeroConf](https://en.wikipedia.org/wiki/Zero-configuration_networking) Protocol Implementation

## Goal

Agent implements [Zero-configuration networking](https://en.wikipedia.org/wiki/Zero-configuration_networking) (hence 'ZeroConf'), a set of technologies that help discover devices and services using DNS-based discovery. There are 2 main elements: Multicast DNS (mDNS) and DNS-based Service Discovery (DNS-SD).

While ZeroConf is often used in home networks (that don't often include regular DNS), ZeroConf is broadly applicable and is useful in IoT deployments in which devices are transient, there are many devices, developers wish to dynamically manage services on these devices.

These technologies require additional packages and shared libraries. Supporting ZeroConf as an Akri protcol possibly (!?) provides a mechanism by which (Kubernetes) application developers can leverage ZeroConf technologies without having to install or be familiar with ZeroConf dependencies.

## Background

For more information, see [Zero-configuration networking](https://en.wikipedia.org/wiki/Zero-configuration_networking).

Linux-based example (using [Avahi](https://www.avahi.org/)) and publishing an example service on `:8888` called 'freddie':

```bash
avahi-publish --service freddie "_example._tcp" 8888
Established under name 'freddie'
```

The service will be published to the default ZeroConf domain (`local`) and it's fully-qualifiied domain-name (FQDN) is thus `freddie.local`

> **NOTE** For the purposes of what follows, while distinct, hosts (devices) and services may be considered equivalent.

Then, it's possible to enumerate hosts and services discovered by ZeroConf using:

```bash
avahi-browse --all
+ wlp6s0 IPv4 freddie                                       _example._tcp        local
```

## Discovery Process

The protocol implementation is written in Rust and uses [`zeroconf`](https://crates.io/crates/zeroconf). Some of what follows may be specific to these technologies.

The Akri Agent is deployed to a Kubernetes cluster. Kubernetes clusters commonly run in-cluster DNS services (nowadays [`CoreDNS`](https://kubernetes.io/docs/tasks/administer-cluster/coredns/)). For this reason, the applicability of the Akri ZeroConf protocol is to devices not accessible within the cluster. The benefit of the Akri ZeroConf protocol is to make off-cluster ZeroConf-accessible hosts (devices) and services accessible to Kubernetes cluster resources (e.g. applications).

For ZeroConf discovery to occur, the Agent's Pod must leverage several ZeroConf depdendencies and libraries. These depdendencies not only expand the size of the Akri Agent (~800MB) but they increase the Agent's surface area and increase the possibility of vulnerabilities.

Discovery is a key functionality of ZeroConf and is straightforward to implement. See the [Browsing services](https://crates.io/crates/zeroconf#browsing-services) examples of the `zeroconf` crate.

One wrinkle is that Akri expects discovery to run to completion. Akri periodically reruns discovery for a protocol. The `zeroconf` crate polls networks for hosts and services.

The implementation used by the protocol is to poll for 5 seconds and report back whichever hosts and services were discovered during that window.

## Broker interfacing

Upon detection of ZeroConf hosts and services, the Akri ZeroConf protocol creates "twins" for each service using the provided, sample broker. A more complete rendition of the `freddie` service could be:

```YAML
{
    name: "freddie",
    kind: "_example._tcp",
    domain: "local",
    host_name: "freddie.local",
    address: "192.168.1.100", port: 8888,
    txt: ...
}
```

Each Broker Instance is configured with environment variables corresponding to the above value:

```bash
AKRI_ZEROCONF_DEVICE_KIND=_example._tcp
AKRI_ZEROCONF=zeroconf
AKRI_ZEROCONF_DEVICE_HOST=freddie.local
AKRI_ZEROCONF_DEVICE_NAME=freddie
AKRI_ZEROCONF_DEVICE_PORT=8888
AKRI_ZEROCONF_DEVICE_ADDR=192.168.1.100
```

Currently the service's TXT records are not provided.

The Broker sample enumerates these values to standard output every 5 seconds.

In practice, a Kubernetes application would use this data to identify services and invoke them.

## Outstanding Questions

+ What would a generic Akri ZeroConf Broker do? In practice, the application developer would likely wish to implement the Broker for their specific application.

+ How to treat support for Kubernetes-supporting service types (TCP, UDP, SCTP)?

## Feature Requests

+ Discovery should differentiate between services that are supportable (!) by Kubernetes (TCP, UDP, SCTP) and those that aren't

+ Discovery should apply user-defined filters on Services so that the Agent only attempts to discover filtered services

+ If there were a generic Broker implementation, it would need to be able to create Kubernetes Services of various types


## References

+ [Zero-configuration networking](https://en.wikipedia.org/wiki/Zero-configuration_networking).
+ [IANA ZeroConf Service Name and Transport Protocol Port Number Registry](https://www.iana.org/assignments/service-names-port-numbers/service-names-port-numbers.xhtml?skey=9&page=132)
+ [Rust `zeroconf` crate](https://crates.io/crates/zeroconf)
+ [Development Branch of Akri ZeroConf Protocol & Broker](https://github.com/DazWilkin/akri/tree/protocol-zeroconf)
---
