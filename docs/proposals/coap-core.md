# [CoAP CoRE](https://tools.ietf.org/html/rfc6690#:~:text=well-known%2Fcore) Protocol Implementation

## Goal

_From the RFC 6690_:

The Constrained RESTful Environments (CoRE) realizes the Representational State Transfer (REST) architecture [REST] in a suitable form for the most constrained nodes (e.g., 8-bit microcontrollers with limited memory) and networks (e.g., IPv6 over Low-Power Wireless Personal Area Networks (6LoWPANs) [RFC4919]). CoRE is aimed at Machine-to-Machine (M2M) applications such as smart energy and building automation.

The main function of such a discovery mechanism is to provide Universal Resource Identifiers (URIs, called links) for the resources hosted by the server, complemented by attributes about those resources and possible further link relations. In CoRE, this collection of links is carried as a resource of its own (as opposed to HTTP headers delivered with a specific resource).

## Discovery Process

### The standard

A well-known relative URI `/.well-known/core` is defined as a default entry point for requesting the list of links about resources hosted by a server and thus performing CoRE Resource Discovery. The CoRE protocol is applicable for use with Constrained Application Protocol (CoAP) [COAP](https://tools.ietf.org/html/rfc7252), HTTP, or any other suitable web transfer protocol.

Resource Discovery can be performed either unicast or multicast.

When a server’s IP address is already known, either a priori or resolved via the Domain Name System (DNS), unicast discovery is performed in order to locate the entry point to the resource of interest. In this specification, this is performed using a GET to `/.well-known/core` on the server, which returns a payload in the CoRE Link Format.

Multicast Resource Discovery is useful when a client needs to locate a resource within a limited scope, and that scope supports IP multicast. A GET request to the appropriate multicast address is made for `/.well-known/core`. In order to limit the number and size of responses, a query string is recommended with the known attributes. Typically, a resource would be discovered based on its Resource Type and/or Interface Description, along with possible application-specific attributes.

### With Akri

The Akri Configuration defines a list of IP addresses to use for resource discovery, implementing thus unicast discovery. Adding support to multicast discovery is surely desiderable.

1. Akri agent sends a `GET /well-known/core` request to the device at `coap://{IP_ADDRESS}:5683`. The standard defines that CoAP devices which mean to support resource discovery, must be reachable with the default port `:5683` and expose endpoint `/well-known/core` implementing the CoRE Link Format.
2. The device responds with the list of supported resources. An example of response in link format is the following:
    ```
    </sensors/temp>;rt="oic.r.temperature";if="sensor",
    </sensors/light>;rt="oic.r.light.brightness";if="sensor"
    ```

    The example is stating that the device has 2 REST resources, `/sensors/temp` and `/sensors/light` which are of type `oic.r.temperature` and `oic.r.light.brightness` respectively. `rt` values are defined in [IANA](https://www.iana.org/assignments/core-parameters/core-parameters.xhtml#rt-link-target-att-value) to have same standardization, although vendor values can probably be used. Then `if` means that the interface description of the resource is of type `sensor`. Currently only resources with interface `sensor` are supported by the agent.
3. For each device, the agent returns a `DiscoveryResult` which will have the following properties:
    ```
    COAP_IP:                 192.168.1.126
    COAP_RESOURCE_TYPES:     oic.r.temperature,oic.r.light.brightness
    oic.r.light.brightness:  /sensors/light
    oic.r.temperature:       /sensors/temp
    ```
    `COAP_IP` and `COAP_RESOURCE_TYPES` are static and available for each Instance. `oic.r.light.brightness` and `oic.r.temperature` are dynamic, based on the discovered resources. By doing so, the cluster can look for a resource (e.g. temperature measurements) by searching for Instances which support the `oic.r.temperature` resource. 
4. An Akri Broker is provisioned for each Istance. The Broker has the following environment variables based on the previous properties and can be reached via its associated service:

  ```
  COAP_IP=192.168.1.126
  oic.r.temperature=/sensors/temp
  oic.r.light.brightness=/sensors/light
  COAP_RESOURCE_TYPES=oic.r.temperature,oic.r.light.brightness
  ```

  The Broker acts as a HTTP to CoAP Proxy. It translates RESTful HTTP requests into RESTful CoAP requests and viceversa for the response. "Cross-Protocol Proxying between CoAP and HTTP" is defined in section 10 of RFC 7252. Currently, the Broker forwards only GET requests.
  
  It's also in a good position to cache CoAP responses. "Unlike HTTP, the cacheability of CoAP responses does not depend on the request method, but it depends on the Response Code." Currently, the Broker caches 2.05 Content responses, which are returned if the device is not reachable during the connection.

## Outstanding Questions

- Is there a better way to store the discovered resources as Configuration in the cluster?

I think the current implementation would need a controller to accept queries about available resources and return the broker which can communicate to the device. The device is listed as generic `akri.sh/coap-core-021dd7` resource on the node, which is pretty much useless. A better label would be `akri.sh/oic.r.temperature-021dd7`, that is the discovered resource. This would allow using the K8s controller for scheduling pods which need the resource.

## Feature Requests

- [ ] Support multicast discovery in the Agent
- [ ] Support all HTTP verbs in the Broker
- [ ] Handle Header translation in the Broker
- [ ] Support Observe [RFC 7641](https://tools.ietf.org/html/rfc7641) which allows the devices to push changes to interested clients.

## References

- [CoAP RFC 7252](https://tools.ietf.org/html/rfc7252).
- [CoRE RFC 6690](https://tools.ietf.org/html/rfc6690#:~:text=well-known%2Fcore).
- [CoAP: An Application Protocol for Billions of Tiny Internet Nodes](https://ieeexplore.ieee.org/document/6159216)
---
