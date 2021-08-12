/// Akri API Version
pub const API_VERSION: &str = "v0";
/// Akri API Version Number
pub const API_VERSION_NUMBER: &str = "0";
/// Version of Pods that the Controller watches for
pub const POD_VERSION_NUMBER: &str = "1";
/// Akri CRD Namespace
pub const API_NAMESPACE: &str = "akri.sh";
/// Akri Configuration CRD name
pub const API_CONFIGURATIONS: &str = "configurations";
/// Akri Instance CRD name
pub const API_INSTANCES: &str = "instances";
/// Akri prefix
pub const AKRI_PREFIX: &str = "akri.sh";
/// Container Annotation name used to store slot name
pub const AKRI_SLOT_ANNOTATION_NAME: &str = "akri.agent.slot";

pub mod configuration;
pub mod instance;
pub mod metrics;

pub mod retry {
    use rand::random;
    use std::time::Duration;
    use tokio::time;

    /// Maximum amount of tries to update or create an instance
    pub const MAX_INSTANCE_UPDATE_TRIES: i8 = 5;

    /// This method will delay a random percentage of up to 200ms
    ///
    /// Wait for random amount of time to stagger update/create requests to etcd from
    /// plugins created at the same time from daemonset
    pub async fn random_delay() {
        let random_decimal: f32 = random::<f32>();
        let random_delay_0_to_200: u64 = (200_f32 * random_decimal) as u64;
        time::sleep(Duration::from_millis(random_delay_0_to_200)).await;
    }
}
