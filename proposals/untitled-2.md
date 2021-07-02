# Simple and Scalable Protocol Extension

## Background

All protocol discovery is currently implemented in each Akri Agent. This is a simple model, but one can imagine some drawbacks:

1. Potentially, this can make each Agent bigger than it needs to be.  For example, Agent contains implementations for ONVIF, udev discovery … but someone may simply want ONVIF.
2. Choosing what protocols are distributed is a build-time decision.  If someone only expected to use the udev implementation, they would need to rebuild Agent, excluding the other protocols.
3. Implementing a new protocol requires changes to Agent.  To add a Foo protocol, the Configuration CRD needs to be changed, Agent's Configuration parsing/handling code needs to be changed, and Agent needs to implement discovery for Foo.

Wouldn't it be great if users could deploy only the implementations they wanted? Wouldn't it be great if a new protocol implementation could be added without changing Agent's code?

### Possible Ideas

1. Within the existing code, create set of build flags that allow people to easily include/exclude protocols.  This would provide a simple method for people to get exactly what they want ... the build system would likely still create the "all-in-one" Agent executable/containers.
2. Create a plugin system that allows Agent to load any libraries that are available.  This would allow people to modify our Dockerfiles to embed only the protocols they desire.
3. Implement all protocols as Pods that can be deployed where needed.  In essence, each protocol discovery implementation would be moved out of Agent and into its own executable.  This executable would notify Agent of its discovery results \(maybe over gRPC, similar to Kuberetes' device plugin framework\).  This would provide an ability to implement protocol discovery in any programming language and without any dependencies on the Akri binaries.  To enable a specific protocol in a cluster, the operator would need to deploy both Agent and the specific protocol container.  Our build system would produce Agent \(without any protocols\) ... and would build a container for each protocol implementation.
4. Akri Agent \(minus specific protocol implementations\) could be exposed as a library.  Each protocol would create its own tailored Agent that was basically an implementation of the DiscoveryHandler trait and an invocation of the Agent library.  This would simplify anyone's effort to implement a new protocol and deploy it in isolation.  

   ```rust
    struct FooProtocol {}
    impl DiscoveryHandler for FooProtocol { 
        … all of the Foo-specific code… 
    } 
    pub fn main() { 
        // define specific protocol handler 
        let protocol_handler = &FooProtocol{};
        let akri_agent = akri::agent::new(&protocol_handler); 
        akri_agent.start(); 
    }
   ```

   This idea could be extended to allow multiple protocol handlers.  That would allow our build system to create an "all-in-one" Agent example:

   ```rust
    pub fn main() { 
        // add all protocol handlers
        let protocol_handlers: Vec<Box<dyn Protocol>> = vec![
            Box::new(onvif::ONVIFProtocol{}),
            Box::new(udev::UdevProtocol{}),
        ];
        let akri_agent = akri::agent::new(&protocol_handlers); 
        akri_agent.start(); 
    }
   ```

### Issues

* The Akri.Configuration CRD, which Agent parses, explicitly describes all possible protocols.  The protocol definition will need to change to be more generically parsable \(maybe to a named property list, where the name is the protocol\)
* Rust doesn't have a built-in plug-in system

