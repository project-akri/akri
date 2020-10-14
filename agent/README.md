# Introduction 
This is the Akri Agent project.  It is an implementation of a [Kubernetes device plugin](https://kubernetes.io/docs/concepts/extend-kubernetes/compute-storage-net/device-plugins/). 

# Design

## Traits

### Public
* **DiscoveryHandler** - This provides an abstraction to allow protocol specific code to handle discovery and provide details for Instance creation.  Planned implementations of this trait include `OnvifDiscoveryHandler`, `UdevDiscoveryHandler`, `OpcuaDiscoveryHandler`, and `DebugEchoDiscoveryHandler`.
```Rust
#[async_trait]
pub trait DiscoveryHandler {
    async fn discover(&self) -> Result<Vec<DiscoveryResult>, Error>;
    fn are_shared(&self) -> Result<bool, Error>;
}
```

### Private
* **EnvVarQuery** - This provides a mockable way to query for `get_discovery_handler` to query environment variables.
```Rust
trait EnvVarQuery {
    fn get_env_var(&self, name: &'static str) -> Result<String, VarError>;
}
```

