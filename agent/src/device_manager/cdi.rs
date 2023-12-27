///This module represents the schema used by CDI in version 0.6.0:
/// https://github.com/cncf-tags/container-device-interface/blob/main/SPEC.md
///
/// It provides helpers to convert from v0 discovery handler protocol
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub name: String,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub annotations: HashMap<String, String>,
    #[serde(default)]
    pub container_edits: ContainerEdit,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContainerEdit {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub device_nodes: Vec<DeviceNode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mounts: Vec<Mount>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hooks: Vec<Hook>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeviceNode {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_path: Option<String>,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub device_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub major: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minor: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_mode: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permissions: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gid: Option<u32>,
}

impl From<akri_discovery_utils::discovery::v0::DeviceSpec> for DeviceNode {
    fn from(value: akri_discovery_utils::discovery::v0::DeviceSpec) -> Self {
        Self {
            path: value.container_path,
            host_path: Some(value.host_path),
            permissions: Some(value.permissions),
            ..Default::default()
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Mount {
    pub host_path: String,
    pub container_path: String,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub mount_type: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
}

impl From<akri_discovery_utils::discovery::v0::Mount> for Mount {
    fn from(value: akri_discovery_utils::discovery::v0::Mount) -> Self {
        let options = match value.read_only {
            false => vec![],
            true => vec!["ro".to_string()],
        };
        Self {
            host_path: value.host_path,
            container_path: value.container_path,
            mount_type: None,
            options,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Hook {
    pub hook_name: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u32>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "cdiVersion", rename = "0.6.0")]
pub struct Kind {
    pub kind: String,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub annotations: HashMap<String, String>,
    pub devices: Vec<Device>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub container_edits: Vec<ContainerEdit>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cdi_mount_from_discovery() {
        let discovery_mount = akri_discovery_utils::discovery::v0::Mount {
            container_path: "/path/in/container".to_string(),
            host_path: "/path/in/host".to_string(),
            read_only: true,
        };
        let expected_mount = Mount {
            host_path: "/path/in/host".to_string(),
            container_path: "/path/in/container".to_string(),
            mount_type: None,
            options: vec!["ro".to_string()],
        };
        assert_eq!(Mount::from(discovery_mount), expected_mount);

        let discovery_mount = akri_discovery_utils::discovery::v0::Mount {
            container_path: "/path/in/container".to_string(),
            host_path: "/path/in/host".to_string(),
            read_only: false,
        };
        let expected_mount = Mount {
            host_path: "/path/in/host".to_string(),
            container_path: "/path/in/container".to_string(),
            mount_type: None,
            options: vec![],
        };
        assert_eq!(Mount::from(discovery_mount), expected_mount);
    }

    #[test]
    fn test_device_node_from_device_spec() {
        let device_spec = akri_discovery_utils::discovery::v0::DeviceSpec {
            container_path: "/path/in/container".to_string(),
            host_path: "/path/in/host".to_string(),
            permissions: "rw".to_string(),
        };
        let expected_device_node = DeviceNode {
            path: "/path/in/container".to_string(),
            host_path: Some("/path/in/host".to_string()),
            device_type: None,
            major: None,
            minor: None,
            file_mode: None,
            permissions: Some("rw".to_string()),
            uid: None,
            gid: None,
        };
        assert_eq!(DeviceNode::from(device_spec), expected_device_node)
    }
}
