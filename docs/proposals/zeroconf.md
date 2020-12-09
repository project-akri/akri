# [Zeroconf](https://en.wikipedia.org/wiki/Zero-configuration_networking) Protocol Implementation

## Goal

Agent implements [Zero-configuration networking](https://en.wikipedia.org/wiki/Zero-configuration_networking) (hence 'Zeroconf'), a set of technologies that help discover devices and services using DNS-based discovery. There are 2 main elements: Multicast DNS (mDNS) and DNS-based Service Discovery (DNS-SD).

While Zeroconf is often used in home networks (that don't often include regular DNS), Zeroconf is broadly applicable and is useful in IoT deployments in which devices are transient, there are many devices, and developers wish to dynamically manage services on these devices.

These technologies require additional packages and shared libraries. Supporting Zeroconf as an Akri protocol provides a mechanism by which (Kubernetes) application developers can leverage Zeroconf technologies without having to install or be familiar with Zeroconf dependencies. The Akri protocol enables exposing these services to Kubernetes clusters enabling Kubernetes applications to utilize them.

## Why Zeroconf?

Zeroconf is a useful mechanism to publish services that have not only names (e.g. `device-123456`) but simple metadata (e.g. `_elevators._udp`) and limited textual data for e.g. labels. This permits scenarios where the Akri Zeroconf protocol could be configured to access e.g. a building's network and, using discovery find services representing e.g. its elevators and create Akri instances for each of the elevators. Akri would provide configuration information about each elevator to each Akri instance and this information could then be passed to broker Pods via environment variables if a broker image is specified in the Zeroconf Configuration. Each Broker would encapsulate the (proprietary protocol and) functionality needed to interact with an elevator and could expose elevator functionality as REST-based or gRPC-based (or some other RPC mechanism) services to Kubernetes applications.

## Background

For more information, see [Zero-configuration networking](https://en.wikipedia.org/wiki/Zero-configuration_networking).

To gain a better understanding of Zeroconf, let's use Avahi, a Zeroconf implementation for Linux, to create an mDNS service that is discoverable by Zeroconf. Then, we will discover the service and its attributes.

```bash
avahi-publish --service freddie "_example._tcp" 8888
Established under name 'freddie'
```

> **NOTE** You may need to install `avahi-utils` if you're running Debian or a derivative

The service will be published to the default Zeroconf domain (`local`) and its fully-qualifiied domain-name (FQDN) is thus `freddie.local`.

> **NOTE** For the purposes of what follows, while distinct, hosts (devices) and services may be considered equivalent.

Then, it's possible to enumerate hosts and services discovered by Zeroconf using:

```bash
avahi-browse --all
+ wlp6s0 IPv4 freddie                                       _example._tcp        local
```

## Discovery Process

The Akri discovery handler is written in Rust and uses [`zeroconf`](https://crates.io/crates/zeroconf). Some of what follows may be specific to these technologies.

> **NOTE** There is a proposal to replace `zeroconf` with [`astro-dnssd`](https://crates.io/crates/astro-dnssd) as this provides cross-platform support. There are some limitations with `astro-dnssd` too that are blocking this switch.

The Akri Agent is deployed to a Kubernetes cluster. Kubernetes clusters commonly run in-cluster DNS services (nowadays [`CoreDNS`](https://kubernetes.io/docs/tasks/administer-cluster/coredns/)). For this reason, the applicability of the Akri Zeroconf protocol is to devices not accessible within the cluster. The benefit of the Akri Zeroconf protocol is to make off-cluster Zeroconf-accessible hosts (devices) and services accessible to Kubernetes cluster resources (e.g. applications).

For Zeroconf discovery to occur, the Agent's Pod must leverage several Zeroconf depdendencies and libraries. These depdendencies not only expand the size of the Akri Agent (~800MB) but they increase the Agent's surface area and increase the possibility of vulnerabilities.

Discovery is a key functionality of Zeroconf and is straightforward to implement. See the [Browsing services](https://crates.io/crates/zeroconf#browsing-services) examples of the `zeroconf` crate.

One wrinkle is that Akri expects discovery to run to completion. Akri periodically reruns discovery for a protocol. The `zeroconf` crate polls networks for hosts and services.

The implementation used by the protocol is to poll for 5 seconds and report back whichever hosts and services were discovered during that window.

## Broker interfacing

Upon detection, the Akri Zeroconf discovery handler creates an Akri instance to represent each discovered service. The instance contains information about the service using environment variables in each broker pod, if a broker pod is specified in the Configuration.

A more complete description of the `freddie` service could be:

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

Each Broker Pod is configured with environment variables corresponding to the above value:

```bash
AKRI_ZEROCONF_DEVICE_KIND=_example._tcp
AKRI_ZEROCONF=zeroconf
AKRI_ZEROCONF_DEVICE_HOST=freddie.local
AKRI_ZEROCONF_DEVICE_NAME=freddie
AKRI_ZEROCONF_DEVICE_PORT=8888
AKRI_ZEROCONF_DEVICE_ADDR=192.168.1.100
```

The service's TXT records are not provided by the implementation of the discovery handler. The TXT records could be enumerated as additional environment variables but it's unclear how best to represent these, possibly: `AKRI_ZEROCONF_DEVICE_[[KEY]]=[[VALUE]]`.

The Broker sample enumerates these values to standard output every 5 seconds.

In practice, a Kubernetes application would use this data to identify services and invoke them.

## Security Considerations

The Akri Agent is only able to browse services accessible to its cluster's hosts.

There are no security considerations for service (mDNS) browsing. This functionality is akin to DNS lookups.

Accessing services that are found by service browsing *may* require the provision of credentials. This would be an implementation detail of an Akri Broker and is not considered further here.

## Outstanding Questions

+ What would a generic Akri Zeroconf Broker do? In practice, the application developer would likely wish to implement the Broker for their specific application.

The limit of a generic Akri Zeroconf Broker is to enumerate services that it discovers and this is demonstrated by a sample Broker included in the Zeroconf Protocol implementation. In practice, a Zeroconf Broker would need to be aware of the implementation(s) of the service that it "twins".

## Feature Requests

+ Support `TXT` records in filtering (see [Filters](#filters))

## Miscellany

### Filters

+ Discovery applies user-defined filters against services so that the Agent limits discovered filtered services to those matching the user's requirements.

For example to filter services named `freddie` on `local` domain with a kind of `_http._tcp`, an example Zeroconf configuration may be applied to the cluster as:

```YAML
apiVersion: akri.sh/v0
kind: Configuration
metadata:
  name: zeroconf
spec:
  protocol:
    zeroconf:
      kind: "_http._tcp"
      domain: "local"
      name: "freddie"
...
```

The `filter` is specified in the Zeroconf CRD Configuration:

```YAML
properties:
  zeroconf: # {{ZeroconfDiscoveryHandler}}
    type: object
    properties:
      kind: 
        type: string
      name: 
        type: string
      domain: 
        type: string
      port: 
        type: integer
```

## References

+ [Zero-configuration networking](https://en.wikipedia.org/wiki/Zero-configuration_networking).
+ [IANA Zeroconf Service Name and Transport Protocol Port Number Registry](https://www.iana.org/assignments/service-names-port-numbers/service-names-port-numbers.xhtml?skey=9&page=132)
+ [Rust `zeroconf` crate](https://crates.io/crates/zeroconf)
+ [Development Branch of Akri Zeroconf Protocol & Broker](https://github.com/DazWilkin/akri/tree/protocol-zeroconf)
---
