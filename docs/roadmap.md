# Roadmap
## Implement additional Discovery Handlers
There are endless sensors, controllers, and MCU class devices on the edge and each type of device has a different
discovery protocol. Akri is an interface for helping expose those devices as resources to your Kubernetes cluster on the
edge. Before it can add a device as a cluster resource, Akri must first discover the device using the appropriate
Discovery Handler. Akri currently supports several Discovery Handlers and was built in a modular way so as to continually support more.
The question is, which protocols should Akri prioritize? We are looking for community feedback to make this decision. If
there is a protocol that you would like implemented, check our [Issues](https://github.com/deislabs/akri/issues) to see
if that protocol has been requested, and thumbs up it so we know you, too, would like it implemented. If there is no
existing request for your protocol, create a [new feature request](https://github.com/deislabs/akri/issues/new/choose).
Rather than waiting for it to be prioritized, you could implement a Discovery Handler for that protocol. See [the
Discovery Handler development document](./discovery-handler-development.md) for more details.

### Currently supported Discovery Handlers
1. ONVIF (to discover IP cameras)
1. udev (to discover anything in the Linux device file system)
1. OPC UA (to discover OPC UA Servers) 

### Protocols we are thinking about adding support for
- Bluetooth
- Simple scan for IP/MAC addresses
- LoRaWAN
- Zeroconf
- Looking for community feedback for more!

## Akri enhancements
Provide new features and enhancements that build on existing Akri functionality.
### New broker deployment strategies
Currently, for every leaf device that is discovered by a node's Akri Agent, a single broker is deployed to that node --
how many nodes get the broker is limited by capacity. This is a fairly specific implementation that does not support all
users' scenarios. The [New Broker Deployment Strategies proposal](./proposals/broker-deployment-strategies.md) discusses
some ways the Akri Controller and Agent could be extended to allow for other broker deployment strategies.

