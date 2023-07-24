use akri_shared::akri::{
    instance::device_usage::DeviceUsageKind, AKRI_SLOT_ANNOTATION_NAME_PREFIX,
};
use std::collections::HashMap;
use std::str::FromStr;

/// Output from crictl query
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct CriCtlOutput {
    containers: Vec<CriCtlContainer>,
}

/// Container from crictl query
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct CriCtlContainer {
    annotations: HashMap<String, String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ParseSlotUsageError;
#[derive(PartialEq, Clone, Debug, Default)]
pub struct SlotUsage {
    kind: DeviceUsageKind,
    slot_id: String,
}

impl std::fmt::Display for SlotUsage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.kind {
            DeviceUsageKind::Free => write!(f, ""),
            DeviceUsageKind::Configuration(vdev_id) => {
                write!(f, "C:{}:{}", vdev_id, self.slot_id)
            }
            DeviceUsageKind::Instance => write!(f, "{}", self.slot_id),
        }
    }
}

impl std::str::FromStr for SlotUsage {
    type Err = ParseSlotUsageError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Ok(SlotUsage {
                kind: DeviceUsageKind::Free,
                slot_id: s.to_string(),
            });
        }

        // Format "C:<vdev_id>:<slot_id>"
        if let Some((vdev_id, slot_id)) = s.strip_prefix("C:").and_then(|s| s.split_once(':')) {
            if slot_id.is_empty() {
                return Err(ParseSlotUsageError);
            }
            return Ok(SlotUsage {
                kind: DeviceUsageKind::Configuration(vdev_id.to_string()),
                slot_id: slot_id.to_string(),
            });
        }

        // Format "<usage_name>"
        Ok(SlotUsage {
            kind: DeviceUsageKind::Instance,
            slot_id: s.to_string(),
        })
    }
}

impl SlotUsage {
    pub fn create(kind: &DeviceUsageKind, slot_id: &str) -> Result<Self, anyhow::Error> {
        match kind {
            DeviceUsageKind::Free => {
                if !slot_id.is_empty() {
                    return Err(anyhow::anyhow!(
                        "Invalid input parameter, slot name: {} provided for free slot usage",
                        slot_id
                    ));
                };
            }
            _ => {
                if slot_id.is_empty() {
                    return Err(anyhow::anyhow!(
                        "Invalid input parameter, no slot provided for slot usage"
                    ));
                };
            }
        };

        Ok(Self {
            kind: kind.clone(),
            slot_id: slot_id.to_string(),
        })
    }

    pub fn get_kind(&self) -> DeviceUsageKind {
        self.kind.clone()
    }

    pub fn get_slot_id(&self) -> String {
        self.slot_id.clone()
    }

    pub fn is_same_slot(&self, slot: &str) -> bool {
        self.slot_id == slot
    }
}

/// This gets the usage slots for an instance by getting the annotations that were stored at id `AKRI_SLOT_ANNOTATION_NAME_PREFIX` during allocate.
pub fn get_container_slot_usage(crictl_output: &str) -> HashMap<String, DeviceUsageKind> {
    match serde_json::from_str::<CriCtlOutput>(crictl_output) {
        Ok(crictl_output_parsed) => crictl_output_parsed
            .containers
            .iter()
            .flat_map(|container| &container.annotations)
            .filter_map(|(key, value)| {
                if key.starts_with(AKRI_SLOT_ANNOTATION_NAME_PREFIX) {
                    let slot_id = key
                        .strip_prefix(AKRI_SLOT_ANNOTATION_NAME_PREFIX)
                        .unwrap_or_default();
                    match SlotUsage::from_str(value) {
                        Ok(slot_usage) => {
                            if slot_usage.is_same_slot(slot_id) {
                                Some((slot_usage.get_slot_id(), slot_usage.get_kind()))
                            } else {
                                None
                            }
                        }
                        Err(_) => None,
                    }
                } else {
                    None
                }
            })
            .collect(),
        Err(e) => {
            trace!(
                "handle_crictl_output - failed to parse crictl output: {:?} => [{:?}]",
                e,
                &crictl_output
            );
            HashMap::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get_container_str(annotation: &str) -> String {
        format!("{{ \
          \"id\": \"46afc04a13ac21d73ff93843efd39590d66927d9b5d743d239542cf2f6de703e\", \
          \"podSandboxId\": \"9094d7341170ecbc6fb0a6a72ba449c8ea98d3267c60e06d815d03102ca7a3e6\", \
          \"metadata\": {{ \
            \"name\": \"akri-agent\", \
            \"attempt\": 0 \
          }}, \
          \"image\": {{ \
            \"image\": \"akri.sh/agent@sha256:86bb6234353129bcae170cfc7db5ad5f282cfc3495555a39aa88042948491850\" \
          }}, \
          \"imageRef\": \"sha256:1305fb97b2db8e9aa715af6a6cd0711986da7935bcbb98f6363aaa5b86163072\", \
          \"state\": \"CONTAINER_RUNNING\", \
          \"createdAt\": \"1587749289000000000\", \
          \"labels\": {{ \
            \"io.kubernetes.container.name\": \"akri-agent\", \
            \"io.kubernetes.pod.name\": \"akri-agent-daemonset-lt2gc\", \
            \"io.kubernetes.pod.namespace\": \"default\", \
            \"io.kubernetes.pod.uid\": \"1ed0098d-8d6f-4001-8192-f690f9b8ae98\" \
          }}, \
          \"annotations\": {{ \
            {} \
            \"io.kubernetes.container.hash\": \"34d65174\", \
            \"io.kubernetes.container.restartCount\": \"0\", \
            \"io.kubernetes.container.terminationMessagePath\": \"/dev/termination-log\", \
            \"io.kubernetes.container.terminationMessagePolicy\": \"File\", \
            \"io.kubernetes.pod.terminationGracePeriod\": \"30\" \
          }} \
        }}",
        annotation)
    }

    #[test]
    fn test_get_container_slot_usage() {
        let _ = env_logger::builder().is_test(true).try_init();

        // Empty output
        assert_eq!(
            HashMap::<String, DeviceUsageKind>::new(),
            get_container_slot_usage(r#""#)
        );
        // Empty json output
        assert_eq!(
            HashMap::<String, DeviceUsageKind>::new(),
            get_container_slot_usage(r#"{}"#)
        );
        // Expected output with no containers
        assert_eq!(
            HashMap::<String, DeviceUsageKind>::new(),
            get_container_slot_usage(r#"{\"containers\": []}"#)
        );
        // Output with syntax error
        assert_eq!(
            HashMap::<String, DeviceUsageKind>::new(),
            get_container_slot_usage(r#"{ddd}"#)
        ); // syntax error
           // Expected output with no slot
        assert_eq!(
            HashMap::<String, DeviceUsageKind>::new(),
            get_container_slot_usage(&format!(
                "{{ \"containers\": [ {} ] }}",
                &get_container_str("")
            ))
        );
        // Expected output with slot (including unexpected property)
        let mut expected = HashMap::new();
        expected.insert("foo".to_string(), DeviceUsageKind::Instance);
        assert_eq!(
            expected,
            get_container_slot_usage(&format!(
                "{{ \"ddd\": \"\", \"containers\": [ {} ] }}",
                &get_container_str("\"akri.agent.slot-foo\": \"foo\",")
            ))
        );
        // Expected output with slot
        assert_eq!(
            expected,
            get_container_slot_usage(&format!(
                "{{ \"containers\": [ {} ] }}",
                &get_container_str("\"akri.agent.slot-foo\": \"foo\",")
            ))
        );
        // Expected output with multiple containers
        let mut expected_2 = HashMap::new();
        expected_2.insert("foo1".to_string(), DeviceUsageKind::Instance);
        expected_2.insert("foo2".to_string(), DeviceUsageKind::Instance);
        assert_eq!(
            expected_2,
            get_container_slot_usage(&format!(
                "{{ \"containers\": [ {}, {} ] }}",
                &get_container_str("\"akri.agent.slot-foo1\": \"foo1\","),
                &get_container_str("\"akri.agent.slot-foo2\": \"foo2\","),
            ))
        );
    }
}
