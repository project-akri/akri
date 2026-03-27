use akri_discovery_utils::discovery::v0::ByteData;
use akri_shared::akri::configuration::{
    DiscoveryProperty, DiscoveryPropertyKeySelector, DiscoveryPropertySource,
};
use async_trait::async_trait;
use k8s_openapi::api::core::v1::{ConfigMap, Secret};
use std::sync::Arc;

use super::{DiscoveryError, DiscoveryManagerKubeInterface};

#[async_trait]
pub(super) trait PropertySolver {
    async fn solve(
        &self,
        client: Arc<dyn DiscoveryManagerKubeInterface>,
    ) -> Result<Option<(String, ByteData)>, DiscoveryError>;
}

#[async_trait]
impl PropertySolver for DiscoveryProperty {
    async fn solve(
        &self,
        client: Arc<dyn DiscoveryManagerKubeInterface>,
    ) -> Result<Option<(String, ByteData)>, DiscoveryError> {
        let value = if let Some(value) = self.value.as_ref() {
            Some(ByteData {
                vec: Some(value.as_bytes().to_vec()),
            })
        } else if let Some(value_from) = self.value_from.as_ref() {
            match value_from {
                DiscoveryPropertySource::ConfigMapKeyRef(val) => {
                    solve_value_from_config_map(val, client.as_ref()).await?
                }
                DiscoveryPropertySource::SecretKeyRef(val) => {
                    solve_value_from_secret(val, client.as_ref()).await?
                }
            }
        } else {
            Some(ByteData { vec: None })
        };
        Ok(value.map(|v| (self.name.clone(), v)))
    }
}

async fn solve_value_from_config_map(
    config_map_key_selector: &DiscoveryPropertyKeySelector,
    client: &dyn DiscoveryManagerKubeInterface,
) -> Result<Option<ByteData>, DiscoveryError> {
    let optional = config_map_key_selector.optional.unwrap_or_default();
    let config_map_name = &config_map_key_selector.name;
    let config_map_namespace = &config_map_key_selector.namespace;
    let config_map_key = &config_map_key_selector.key;

    let config_map = client
        .namespaced(config_map_namespace)
        .get(config_map_name)
        .await?;

    if config_map.is_none() {
        if optional {
            return Ok(None);
        } else {
            return Err(DiscoveryError::UnsolvableProperty("ConfigMap"));
        }
    }
    let config_map: ConfigMap = config_map.unwrap();
    if let Some(data) = config_map.data {
        if let Some(v) = data.get(config_map_key) {
            return Ok(Some(ByteData {
                vec: Some(v.as_bytes().to_vec()),
            }));
        }
    }
    if let Some(binary_data) = config_map.binary_data {
        if let Some(v) = binary_data.get(config_map_key) {
            return Ok(Some(ByteData {
                vec: Some(v.0.clone()),
            }));
        }
    }

    // config_map key/value not found
    if optional {
        Ok(None)
    } else {
        Err(DiscoveryError::UnsolvableProperty("ConfigMap"))
    }
}

async fn solve_value_from_secret(
    secret_key_selector: &DiscoveryPropertyKeySelector,
    client: &dyn DiscoveryManagerKubeInterface,
) -> Result<Option<ByteData>, DiscoveryError> {
    let optional = secret_key_selector.optional.unwrap_or_default();
    let secret_name = &secret_key_selector.name;
    let secret_namespace = &secret_key_selector.namespace;
    let secret_key = &secret_key_selector.key;

    let secret = client.namespaced(secret_namespace).get(secret_name).await?;
    if secret.is_none() {
        if optional {
            return Ok(None);
        } else {
            return Err(DiscoveryError::UnsolvableProperty("Secret"));
        }
    }
    let secret: Secret = secret.unwrap();
    // All key-value pairs in the stringData field are internally merged into the data field
    // we don't need to check string_data.
    if let Some(data) = secret.data {
        if let Some(v) = data.get(secret_key) {
            return Ok(Some(ByteData {
                vec: Some(v.0.clone()),
            }));
        }
    }

    // secret key/value not found
    if optional {
        Ok(None)
    } else {
        Err(DiscoveryError::UnsolvableProperty("Secret"))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use akri_shared::k8s::api::MockApi;
    use k8s_openapi::ByteString;

    use crate::discovery_handler_manager::mock::MockDiscoveryManagerKubeInterface;

    use super::*;

    #[tokio::test]
    async fn test_get_discovery_properties_value_from_secret_no_secret_found() {
        let _ = env_logger::builder().is_test(true).try_init();
        let namespace_name = "namespace_name";
        let secret_name = "secret_1";
        let key_in_secret = "key_in_secret";

        let selector = DiscoveryPropertyKeySelector {
            key: key_in_secret.to_string(),
            name: secret_name.to_string(),
            namespace: namespace_name.to_string(),
            optional: Some(false),
        };

        let mut mock_secret_api = MockApi::new();
        mock_secret_api
            .expect_get()
            .times(1)
            .withf(move |name| name == secret_name)
            .returning(move |_| Ok(None));
        let mut mock_kube_client = MockDiscoveryManagerKubeInterface::new();
        mock_kube_client
            .secret
            .expect_namespaced()
            .return_once(|_| Box::new(mock_secret_api));

        // solve_value_from_secret should return error if secret not found
        let result = solve_value_from_secret(&selector, &mock_kube_client).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_discovery_properties_value_from_secret_no_secret_found_optional() {
        let _ = env_logger::builder().is_test(true).try_init();
        let namespace_name = "namespace_name";
        let secret_name = "secret_1";
        let key_in_secret = "key_in_secret";

        let selector = DiscoveryPropertyKeySelector {
            key: key_in_secret.to_string(),
            name: secret_name.to_string(),
            namespace: namespace_name.to_string(),
            optional: Some(true),
        };

        let mut mock_secret_api = MockApi::new();
        mock_secret_api
            .expect_get()
            .times(1)
            .withf(move |name| name == secret_name)
            .returning(move |_| Ok(None));
        let mut mock_kube_client = MockDiscoveryManagerKubeInterface::new();
        mock_kube_client
            .secret
            .expect_namespaced()
            .return_once(|_| Box::new(mock_secret_api));

        // solve_value_from_secret for an optional key should return None if secret not found
        let result = solve_value_from_secret(&selector, &mock_kube_client).await;
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_get_discovery_properties_value_from_secret_no_key() {
        let _ = env_logger::builder().is_test(true).try_init();
        let namespace_name = "namespace_name";
        let secret_name = "secret_1";
        let key_in_secret = "key_in_secret";

        let selector = DiscoveryPropertyKeySelector {
            key: key_in_secret.to_string(),
            name: secret_name.to_string(),
            namespace: namespace_name.to_string(),
            optional: Some(false),
        };

        let mut mock_secret_api = MockApi::new();
        mock_secret_api
            .expect_get()
            .times(1)
            .withf(move |name| name == secret_name)
            .returning(move |_| Ok(Default::default()));
        let mut mock_kube_client = MockDiscoveryManagerKubeInterface::new();
        mock_kube_client
            .secret
            .expect_namespaced()
            .return_once(|_| Box::new(mock_secret_api));

        // solve_value_from_secret should return error if key in secret not found
        assert!(
            solve_value_from_secret(&selector, &mock_kube_client)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn test_get_discovery_properties_value_from_secret_no_key_optional() {
        let _ = env_logger::builder().is_test(true).try_init();
        let namespace_name = "namespace_name";
        let secret_name = "secret_1";
        let key_in_secret = "key_in_config_map";

        let selector = DiscoveryPropertyKeySelector {
            key: key_in_secret.to_string(),
            name: secret_name.to_string(),
            namespace: namespace_name.to_string(),
            optional: Some(true),
        };

        let mut mock_secret_api = MockApi::new();
        mock_secret_api
            .expect_get()
            .times(1)
            .withf(move |name| name == secret_name)
            .returning(move |_| Ok(Default::default()));
        let mut mock_kube_client = MockDiscoveryManagerKubeInterface::new();
        mock_kube_client
            .secret
            .expect_namespaced()
            .return_once(|_| Box::new(mock_secret_api));

        // solve_value_from_secret for an optional key should return None if key in secret not found
        let result = solve_value_from_secret(&selector, &mock_kube_client).await;
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_get_discovery_properties_value_from_secret_no_value() {
        let _ = env_logger::builder().is_test(true).try_init();
        let namespace_name = "namespace_name";
        let secret_name = "secret_1";
        let key_in_secret = "key_in_secret";

        let selector = DiscoveryPropertyKeySelector {
            key: key_in_secret.to_string(),
            name: secret_name.to_string(),
            namespace: namespace_name.to_string(),
            optional: Some(false),
        };

        let mut mock_secret_api = MockApi::new();
        mock_secret_api
            .expect_get()
            .times(1)
            .withf(move |name| name == secret_name)
            .returning(move |_| {
                let secret = Secret {
                    data: Some(BTreeMap::new()),
                    ..Default::default()
                };
                Ok(Some(secret))
            });
        let mut mock_kube_client = MockDiscoveryManagerKubeInterface::new();
        mock_kube_client
            .secret
            .expect_namespaced()
            .return_once(|_| Box::new(mock_secret_api));

        // solve_value_from_secret should return error if no value in secret
        let result = solve_value_from_secret(&selector, &mock_kube_client).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_discovery_properties_value_from_secret_no_value_optional() {
        let _ = env_logger::builder().is_test(true).try_init();
        let namespace_name = "namespace_name";
        let secret_name = "secret_1";
        let key_in_secret = "key_in_config_map";

        let selector = DiscoveryPropertyKeySelector {
            key: key_in_secret.to_string(),
            name: secret_name.to_string(),
            namespace: namespace_name.to_string(),
            optional: Some(true),
        };

        let mut mock_secret_api = MockApi::new();
        mock_secret_api
            .expect_get()
            .times(1)
            .withf(move |name| name == secret_name)
            .returning(move |_| {
                let secret = Secret {
                    data: Some(BTreeMap::new()),
                    ..Default::default()
                };
                Ok(Some(secret))
            });
        let mut mock_kube_client = MockDiscoveryManagerKubeInterface::new();
        mock_kube_client
            .secret
            .expect_namespaced()
            .return_once(|_| Box::new(mock_secret_api));

        // solve_value_from_secret for an optional key should return None if key in secret not found
        let result = solve_value_from_secret(&selector, &mock_kube_client).await;
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_get_discovery_properties_value_from_secret_data_value() {
        let _ = env_logger::builder().is_test(true).try_init();
        let namespace_name = "namespace_name";
        let secret_name = "secret_1";
        let key_in_secret = "key_in_secret";
        let value_in_secret = "value_in_secret";

        let selector = DiscoveryPropertyKeySelector {
            key: key_in_secret.to_string(),
            name: secret_name.to_string(),
            namespace: namespace_name.to_string(),
            optional: Some(false),
        };

        let mut mock_secret_api = MockApi::new();
        mock_secret_api
            .expect_get()
            .times(1)
            .withf(move |name| name == secret_name)
            .returning(move |_| {
                let data = BTreeMap::from([(
                    key_in_secret.to_string(),
                    ByteString(value_in_secret.into()),
                )]);
                let secret = Secret {
                    data: Some(data),
                    ..Default::default()
                };
                Ok(Some(secret))
            });
        let mut mock_kube_client = MockDiscoveryManagerKubeInterface::new();
        mock_kube_client
            .secret
            .expect_namespaced()
            .return_once(|_| Box::new(mock_secret_api));

        let expected_result = ByteData {
            vec: Some(value_in_secret.into()),
        };

        // solve_value_from_secret should return correct value if data value in secret
        let result = solve_value_from_secret(&selector, &mock_kube_client).await;
        assert_eq!(result.unwrap().unwrap(), expected_result);
    }

    #[tokio::test]
    async fn test_get_discovery_properties_value_from_config_map_no_config_map_found() {
        let _ = env_logger::builder().is_test(true).try_init();
        let namespace_name = "namespace_name";
        let config_map_name = "config_map_1";
        let key_in_config_map = "key_in_config_map";

        let selector = DiscoveryPropertyKeySelector {
            key: key_in_config_map.to_string(),
            name: config_map_name.to_string(),
            namespace: namespace_name.to_string(),
            optional: Some(false),
        };

        let mut mock_cm_api = MockApi::new();
        mock_cm_api
            .expect_get()
            .times(1)
            .withf(move |name| name == config_map_name)
            .returning(move |_| Ok(None));
        let mut mock_kube_client = MockDiscoveryManagerKubeInterface::new();
        mock_kube_client
            .config
            .expect_namespaced()
            .return_once(|_| Box::new(mock_cm_api));

        // solve_value_from_config_map should return error if configMap not found
        let result = solve_value_from_config_map(&selector, &mock_kube_client).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_discovery_properties_value_from_config_map_no_config_map_found_optional() {
        let _ = env_logger::builder().is_test(true).try_init();
        let namespace_name = "namespace_name";
        let config_map_name = "config_map_1";
        let key_in_config_map = "key_in_config_map";

        let selector = DiscoveryPropertyKeySelector {
            key: key_in_config_map.to_string(),
            name: config_map_name.to_string(),
            namespace: namespace_name.to_string(),
            optional: Some(true),
        };

        let mut mock_cm_api = MockApi::new();
        mock_cm_api
            .expect_get()
            .times(1)
            .withf(move |name| name == config_map_name)
            .returning(move |_| Ok(None));
        let mut mock_kube_client = MockDiscoveryManagerKubeInterface::new();
        mock_kube_client
            .config
            .expect_namespaced()
            .return_once(|_| Box::new(mock_cm_api));

        // solve_value_from_config_map for an optional key should return None if configMap not found
        let result = solve_value_from_config_map(&selector, &mock_kube_client).await;
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_get_discovery_properties_value_from_config_map_no_key() {
        let _ = env_logger::builder().is_test(true).try_init();
        let namespace_name = "namespace_name";
        let config_map_name = "config_map_1";
        let key_in_config_map = "key_in_config_map";

        let selector = DiscoveryPropertyKeySelector {
            key: key_in_config_map.to_string(),
            name: config_map_name.to_string(),
            namespace: namespace_name.to_string(),
            optional: Some(false),
        };

        let mut mock_cm_api = MockApi::new();
        mock_cm_api
            .expect_get()
            .times(1)
            .withf(move |name| name == config_map_name)
            .returning(move |_| Ok(Default::default()));
        let mut mock_kube_client = MockDiscoveryManagerKubeInterface::new();
        mock_kube_client
            .config
            .expect_namespaced()
            .return_once(|_| Box::new(mock_cm_api));

        // solve_value_from_config_map should return error if key in configMap not found
        let result = solve_value_from_config_map(&selector, &mock_kube_client).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_discovery_properties_value_from_config_map_no_key_optional() {
        let _ = env_logger::builder().is_test(true).try_init();
        let namespace_name = "namespace_name";
        let config_map_name = "config_map_1";
        let key_in_config_map = "key_in_config_map";

        let selector = DiscoveryPropertyKeySelector {
            key: key_in_config_map.to_string(),
            name: config_map_name.to_string(),
            namespace: namespace_name.to_string(),
            optional: Some(true),
        };

        let mut mock_cm_api = MockApi::new();
        mock_cm_api
            .expect_get()
            .times(1)
            .withf(move |name| name == config_map_name)
            .returning(move |_| Ok(Default::default()));
        let mut mock_kube_client = MockDiscoveryManagerKubeInterface::new();
        mock_kube_client
            .config
            .expect_namespaced()
            .return_once(|_| Box::new(mock_cm_api));

        // solve_value_from_config_map for an optional key should return None if key in configMap not found
        let result = solve_value_from_config_map(&selector, &mock_kube_client).await;
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_get_discovery_properties_value_from_config_map_no_value() {
        let _ = env_logger::builder().is_test(true).try_init();
        let namespace_name = "namespace_name";
        let config_map_name = "config_map_1";
        let key_in_config_map = "key_in_config_map";

        let selector = DiscoveryPropertyKeySelector {
            key: key_in_config_map.to_string(),
            name: config_map_name.to_string(),
            namespace: namespace_name.to_string(),
            optional: Some(false),
        };

        let mut mock_cm_api = MockApi::new();
        mock_cm_api
            .expect_get()
            .times(1)
            .withf(move |name| name == config_map_name)
            .returning(move |_| {
                let config_map = ConfigMap {
                    data: Some(BTreeMap::new()),
                    binary_data: Some(BTreeMap::new()),
                    ..Default::default()
                };
                Ok(Some(config_map))
            });
        let mut mock_kube_client = MockDiscoveryManagerKubeInterface::new();
        mock_kube_client
            .config
            .expect_namespaced()
            .return_once(|_| Box::new(mock_cm_api));

        // solve_value_from_config_map should return error if no value in configMap
        let result = solve_value_from_config_map(&selector, &mock_kube_client).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_discovery_properties_value_from_config_map_no_value_optional() {
        let _ = env_logger::builder().is_test(true).try_init();
        let namespace_name = "namespace_name";
        let config_map_name = "config_map_1";
        let key_in_config_map = "key_in_config_map";

        let selector = DiscoveryPropertyKeySelector {
            key: key_in_config_map.to_string(),
            name: config_map_name.to_string(),
            namespace: namespace_name.to_string(),
            optional: Some(true),
        };

        let mut mock_cm_api = MockApi::new();
        mock_cm_api
            .expect_get()
            .times(1)
            .withf(move |name| name == config_map_name)
            .returning(move |_| {
                let config_map = ConfigMap {
                    data: Some(BTreeMap::new()),
                    binary_data: Some(BTreeMap::new()),
                    ..Default::default()
                };
                Ok(Some(config_map))
            });
        let mut mock_kube_client = MockDiscoveryManagerKubeInterface::new();
        mock_kube_client
            .config
            .expect_namespaced()
            .return_once(|_| Box::new(mock_cm_api));

        // solve_value_from_config_map for an optional key should return None if key in configMap not found
        let result = solve_value_from_config_map(&selector, &mock_kube_client).await;
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_get_discovery_properties_value_from_config_map_data_value() {
        let _ = env_logger::builder().is_test(true).try_init();
        let namespace_name = "namespace_name";
        let config_map_name = "config_map_1";
        let key_in_config_map = "key_in_config_map";
        let value_in_config_map = "value_in_config_map";

        let selector = DiscoveryPropertyKeySelector {
            key: key_in_config_map.to_string(),
            name: config_map_name.to_string(),
            namespace: namespace_name.to_string(),
            optional: Some(false),
        };

        let mut mock_cm_api = MockApi::new();
        mock_cm_api
            .expect_get()
            .times(1)
            .withf(move |name| name == config_map_name)
            .returning(move |_| {
                let data = BTreeMap::from([(
                    key_in_config_map.to_string(),
                    value_in_config_map.to_string(),
                )]);
                let config_map = ConfigMap {
                    data: Some(data),
                    binary_data: Some(BTreeMap::new()),
                    ..Default::default()
                };
                Ok(Some(config_map))
            });
        let mut mock_kube_client = MockDiscoveryManagerKubeInterface::new();
        mock_kube_client
            .config
            .expect_namespaced()
            .return_once(|_| Box::new(mock_cm_api));

        let expected_result = ByteData {
            vec: Some(value_in_config_map.into()),
        };

        // solve_value_from_config_map should return correct value if data value in configMap
        let result = solve_value_from_config_map(&selector, &mock_kube_client).await;
        assert_eq!(result.unwrap().unwrap(), expected_result);
    }

    #[tokio::test]
    async fn test_get_discovery_properties_value_from_config_map_binary_data_value() {
        let _ = env_logger::builder().is_test(true).try_init();
        let namespace_name = "namespace_name";
        let config_map_name = "config_map_1";
        let key_in_config_map = "key_in_config_map";
        let value_in_config_map = "value_in_config_map";

        let selector = DiscoveryPropertyKeySelector {
            key: key_in_config_map.to_string(),
            name: config_map_name.to_string(),
            namespace: namespace_name.to_string(),
            optional: Some(false),
        };

        let mut mock_cm_api = MockApi::new();
        mock_cm_api
            .expect_get()
            .times(1)
            .withf(move |name| name == config_map_name)
            .returning(move |_| {
                let binary_data = BTreeMap::from([(
                    key_in_config_map.to_string(),
                    ByteString(value_in_config_map.into()),
                )]);
                let config_map = ConfigMap {
                    data: Some(BTreeMap::new()),
                    binary_data: Some(binary_data),
                    ..Default::default()
                };
                Ok(Some(config_map))
            });
        let mut mock_kube_client = MockDiscoveryManagerKubeInterface::new();
        mock_kube_client
            .config
            .expect_namespaced()
            .return_once(|_| Box::new(mock_cm_api));

        let expected_result = ByteData {
            vec: Some(value_in_config_map.into()),
        };

        // solve_value_from_config_map should return correct value if binary data value in configMap
        let result = solve_value_from_config_map(&selector, &mock_kube_client).await;
        assert_eq!(result.unwrap().unwrap(), expected_result);
    }

    #[tokio::test]
    async fn test_get_discovery_properties_value_from_config_map_data_and_binary_data_value() {
        let _ = env_logger::builder().is_test(true).try_init();
        let namespace_name = "namespace_name";
        let config_map_name = "config_map_1";
        let key_in_config_map = "key_in_config_map";
        let value_in_config_map = "value_in_config_map";
        let binary_value_in_config_map = "binary_value_in_config_map";

        let selector = DiscoveryPropertyKeySelector {
            key: key_in_config_map.to_string(),
            name: config_map_name.to_string(),
            namespace: namespace_name.to_string(),
            optional: Some(false),
        };

        let mut mock_cm_api = MockApi::new();
        mock_cm_api
            .expect_get()
            .times(1)
            .withf(move |name| name == config_map_name)
            .returning(move |_| {
                let data = BTreeMap::from([(
                    key_in_config_map.to_string(),
                    value_in_config_map.to_string(),
                )]);
                let binary_data = BTreeMap::from([(
                    key_in_config_map.to_string(),
                    ByteString(binary_value_in_config_map.into()),
                )]);
                let config_map = ConfigMap {
                    data: Some(data),
                    binary_data: Some(binary_data),
                    ..Default::default()
                };
                Ok(Some(config_map))
            });
        let mut mock_kube_client = MockDiscoveryManagerKubeInterface::new();
        mock_kube_client
            .config
            .expect_namespaced()
            .return_once(|_| Box::new(mock_cm_api));

        let expected_result = ByteData {
            vec: Some(value_in_config_map.into()),
        };

        // solve_value_from_config_map should return value from data if both data and binary data value exist
        let result = solve_value_from_config_map(&selector, &mock_kube_client).await;
        assert_eq!(result.unwrap().unwrap(), expected_result);
    }
}
