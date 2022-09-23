# Introduction 
This is the Akri Agent project.  It is an implementation of a [Kubernetes device plugin](https://kubernetes.io/docs/concepts/extend-kubernetes/compute-storage-net/device-plugins/). 

# Design

## Traits

### Public
* **DiscoveryHandler** - This provides an abstraction to allow protocol specific code to handle discovery and provide details for Instance creation. The trait is defined by Akri's [discovery API](../discovery-utils/proto/discovery.proto). Implementations of this trait can be found in the [discovery handlers directory](../discovery-handlers).
```Rust
#[async_trait]
pub trait DiscoveryHandler {
    async fn discover(
            &self,
            request: tonic::Request<akri_discovery_utils::discovery::v0::DiscoverRequest>,
        ) -> Result<tonic::Response<akri_discovery_utils::discovery::v0::DiscoverStream>, tonic::Status>;
}
```
* **RequestDeviceInfo** - This gRPC service is used by discovery handlers to query external information about the device with the initial information of an discovered device, such as its host ip address. 

### Private
* **EnvVarQuery** - This provides a mockable way to query for `get_discovery_handler` to query environment variables.
```Rust
trait EnvVarQuery {
    fn get_env_var(&self, name: &'static str) -> Result<String, VarError>;
}
```

* **QueryDevice** - Upon receive request from remote RequestDeviceInfo call, Agent will contact an external device inventory REST service supplier to fetch device related infomation. This exernal device inventory supplier can be specified in Akri configuration file.