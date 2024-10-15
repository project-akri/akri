use super::akri::{API_NAMESPACE, API_VERSION};

use mockall::predicate::*;

pub mod api;
pub mod job;
pub mod pod;

pub const NODE_SELECTOR_OP_IN: &str = "In";
pub const OBJECT_NAME_FIELD: &str = "metadata.name";
pub const RESOURCE_REQUIREMENTS_KEY: &str = "{{PLACEHOLDER}}";
pub const ERROR_NOT_FOUND: u16 = 404;
pub const ERROR_CONFLICT: u16 = 409;
pub const APP_LABEL_ID: &str = "app";
pub const CONTROLLER_LABEL_ID: &str = "controller";
pub const AKRI_CONFIGURATION_LABEL_NAME: &str = "akri.sh/configuration";
pub const AKRI_INSTANCE_LABEL_NAME: &str = "akri.sh/instance";
pub const AKRI_TARGET_NODE_LABEL_NAME: &str = "akri.sh/target-node";

/// OwnershipType defines what type of Kubernetes object
/// an object is dependent on
#[derive(Clone, Debug)]
pub enum OwnershipType {
    Configuration,
    Instance,
    Pod,
    Service,
}

/// OwnershipInfo provides enough information to identify
/// the Kubernetes object an object depends on
#[derive(Clone, Debug)]
pub struct OwnershipInfo {
    object_type: OwnershipType,
    object_uid: String,
    object_name: String,
}

impl OwnershipInfo {
    pub fn new(object_type: OwnershipType, object_name: String, object_uid: String) -> Self {
        OwnershipInfo {
            object_type,
            object_uid,
            object_name,
        }
    }

    pub fn get_api_version(&self) -> String {
        match self.object_type {
            OwnershipType::Instance | OwnershipType::Configuration => {
                format!("{}/{}", API_NAMESPACE, API_VERSION)
            }
            OwnershipType::Pod | OwnershipType::Service => "core/v1".to_string(),
        }
    }

    pub fn get_kind(&self) -> String {
        match self.object_type {
            OwnershipType::Instance => "Instance",
            OwnershipType::Configuration => "Configuration",
            OwnershipType::Pod => "Pod",
            OwnershipType::Service => "Service",
        }
        .to_string()
    }

    pub fn get_controller(&self) -> Option<bool> {
        Some(true)
    }

    pub fn get_block_owner_deletion(&self) -> Option<bool> {
        Some(true)
    }

    pub fn get_name(&self) -> String {
        self.object_name.clone()
    }

    pub fn get_uid(&self) -> String {
        self.object_uid.clone()
    }
}

#[cfg(test)]
pub mod test_ownership {
    use super::*;

    #[tokio::test]
    async fn test_ownership_from_config() {
        let name = "asdf";
        let uid = "zxcv";
        let ownership = OwnershipInfo::new(
            OwnershipType::Configuration,
            name.to_string(),
            uid.to_string(),
        );
        assert_eq!(
            format!("{}/{}", API_NAMESPACE, API_VERSION),
            ownership.get_api_version()
        );
        assert_eq!("Configuration", &ownership.get_kind());
        assert!(ownership.get_controller().unwrap());
        assert!(ownership.get_block_owner_deletion().unwrap());
        assert_eq!(name, &ownership.get_name());
        assert_eq!(uid, &ownership.get_uid());
    }
    #[tokio::test]
    async fn test_ownership_from_instance() {
        let name = "asdf";
        let uid = "zxcv";
        let ownership =
            OwnershipInfo::new(OwnershipType::Instance, name.to_string(), uid.to_string());
        assert_eq!(
            format!("{}/{}", API_NAMESPACE, API_VERSION),
            ownership.get_api_version()
        );
        assert_eq!("Instance", &ownership.get_kind());
        assert!(ownership.get_controller().unwrap());
        assert!(ownership.get_block_owner_deletion().unwrap());
        assert_eq!(name, &ownership.get_name());
        assert_eq!(uid, &ownership.get_uid());
    }
    #[tokio::test]
    async fn test_ownership_from_pod() {
        let name = "asdf";
        let uid = "zxcv";
        let ownership = OwnershipInfo::new(OwnershipType::Pod, name.to_string(), uid.to_string());
        assert_eq!("core/v1", ownership.get_api_version());
        assert_eq!("Pod", &ownership.get_kind());
        assert!(ownership.get_controller().unwrap());
        assert!(ownership.get_block_owner_deletion().unwrap());
        assert_eq!(name, &ownership.get_name());
        assert_eq!(uid, &ownership.get_uid());
    }
    #[tokio::test]
    async fn test_ownership_from_service() {
        let name = "asdf";
        let uid = "zxcv";
        let ownership =
            OwnershipInfo::new(OwnershipType::Service, name.to_string(), uid.to_string());
        assert_eq!("core/v1", ownership.get_api_version());
        assert_eq!("Service", &ownership.get_kind());
        assert!(ownership.get_controller().unwrap());
        assert!(ownership.get_block_owner_deletion().unwrap());
        assert_eq!(name, &ownership.get_name());
        assert_eq!(uid, &ownership.get_uid());
    }
}
