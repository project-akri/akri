# Roadmap
## Implement additional protocol handlers
There are endless sensors, controllers, and MCU class devices on the edge and each type of device has a different
discovery protocol. Akri is an interface for helping expose those devices as resources to your Kubernetes cluster on
the edge. Before it can add a device as a cluster resource, Akri must first discover the device using the appropriate
protocol. Akri currently supports several protocols and was built in a modular way so as to continually support more.
The question is, which protocols should Akri prioritize? This lays out a prioritization based off of our understanding
of which protocols Akri users want handlers implemented for first. If there is a protocol that you would like
implemented, check our [Issues](https://github.com/deislabs/akri/issues) to see if that protocol has been requested,
and thumbs up it so we know you, too, would like it implemented. If there is no existing request for your protocol,
create a [new feature request](https://github.com/deislabs/akri/issues/new/choose). Rather than waiting for it to be
prioritized, you could implement discovery via that protocol in Agent. See [the extensibility document](./extensibility.md) 
for more details.

### Currently supported protocols
1. ONVIF (to discover IP cameras)
1. udev (to discover anything in the Linux file system)

### Prioritization of protocols to support
1. OPC UA (to discovery OPC UA Servers) 
    - [Proposal uploaded](./proposals/opcua.md)
    - Working end to end solution in [opcua branch](https://github.com/deislabs/akri/tree/opcua) has yet to be merged into main
1. MQTT
1. Bluetooth
1. AMQP
1. LoRaWAN

## Akri enhancements
Provide new features and enhancements that build on existing Akri functionality.
### New broker deployment strategies
Currently, for every leaf device that is discovered by a node's Akri Agent, a single broker is deployed to that node --
how many nodes get the broker is limited by capacity. This is a fairly specific implementation that does not support all
users' scenarios. The [New Broker Deployment Strategies proposal](./proposals/broker-deployment-strategies.md) discusses
some ways the Akri Controller and Agent could be extended to allow for other broker deployment strategies.
### Simple and scalable protocol extension
All protocol discovery is currently implemented in each Akri Agent. The drawbacks to this and some potential solutions
can be seen in the [Simple and Scalable Protocol Extension proposal](./proposals/simple-protocol-extension.md).

