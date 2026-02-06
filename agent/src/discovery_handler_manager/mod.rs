pub mod discovery_handler_registry;
mod discovery_property_solver;
#[cfg(any(test, feature = "agent-full"))]
mod embedded_handler;
mod registration_socket;

use std::{collections::HashMap, sync::Arc};

use akri_shared::{akri::configuration::Configuration, k8s::api::IntoApi};
use k8s_openapi::api::core::v1::{ConfigMap, Secret};

use kube::runtime::reflector::ObjectRef;
use thiserror::Error;
use tokio::sync::{mpsc, watch};

use self::discovery_handler_registry::DHRegistryImpl;

pub use registration_socket::run_registration_server;

#[derive(Error, Debug)]
pub enum DiscoveryError {
    #[error("Invalid discovery details provided to discovery handler")]
    InvalidDiscoveryDetails,

    #[error("Discovery Handler {0} is unavailable")]
    UnavailableDiscoveryHandler(String),

    #[error("discoveryProperties' referenced {0} not found")]
    UnsolvableProperty(&'static str),

    #[error(transparent)]
    KubeError(#[from] kube::Error),

    #[error("No registered handler for {0}")]
    NoHandler(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub fn new_registry(
    kube_client: Arc<dyn DiscoveryManagerKubeInterface>,
) -> (
    watch::Receiver<HashMap<String, crate::device_manager::cdi::Kind>>,
    impl discovery_handler_registry::DiscoveryHandlerRegistry,
    mpsc::Receiver<ObjectRef<Configuration>>,
) {
    let (sender, receiver) = watch::channel(Default::default());
    let (configuration_notifier, notifier) = mpsc::channel(10);
    let registry = DHRegistryImpl::new(kube_client, sender, configuration_notifier);
    (receiver, registry, notifier)
}

pub trait DiscoveryManagerKubeInterface: IntoApi<Secret> + IntoApi<ConfigMap> {}

impl<T: IntoApi<Secret> + IntoApi<ConfigMap>> DiscoveryManagerKubeInterface for T {}

#[cfg(test)]
mod mock {

    use akri_shared::k8s::api::{Api, IntoApi, MockIntoApi};
    use k8s_openapi::api::core::v1::{ConfigMap, Secret};
    #[derive(Default)]
    pub struct MockDiscoveryManagerKubeInterface {
        pub secret: MockIntoApi<Secret>,
        pub config: MockIntoApi<ConfigMap>,
    }

    impl MockDiscoveryManagerKubeInterface {
        pub fn new() -> Self {
            Self {
                secret: MockIntoApi::new(),
                config: MockIntoApi::new(),
            }
        }
    }

    impl IntoApi<Secret> for MockDiscoveryManagerKubeInterface {
        fn all(&self) -> Box<dyn Api<Secret>> {
            self.secret.all()
        }

        fn namespaced(&self, namespace: &str) -> Box<dyn Api<Secret>> {
            self.secret.namespaced(namespace)
        }

        fn default_namespaced(&self) -> Box<dyn Api<Secret>> {
            self.secret.default_namespaced()
        }
    }

    impl IntoApi<ConfigMap> for MockDiscoveryManagerKubeInterface {
        fn all(&self) -> Box<dyn Api<ConfigMap>> {
            self.config.all()
        }

        fn namespaced(&self, namespace: &str) -> Box<dyn Api<ConfigMap>> {
            self.config.namespaced(namespace)
        }

        fn default_namespaced(&self) -> Box<dyn Api<ConfigMap>> {
            self.config.default_namespaced()
        }
    }
}
