mod util;
#[macro_use]
extern crate lazy_static;
use akri_shared::{
    akri::{metrics::run_metrics_server, API_NAMESPACE},
    os::env_var::{ActualEnvVarQuery, EnvVarQuery},
};
use log::{info, trace};
use prometheus::IntCounter;
use regex::Regex;
use tokio::signal;
use util::{camera_capturer, camera_service};

lazy_static! {
    pub static ref FRAME_COUNT_METRIC: IntCounter =
        prometheus::register_int_counter!("akri_frame_count", "Akri Frame Count")
            .expect("akri_frame_count cannot be created");
}

/// regular expression pattern of devnode environment variable id
pub const UDEV_DEVNODE_LABEL_ID_PATTERN: &str = "UDEV_DEVNODE_[A-F0-9]{6,6}$";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    println!("{} udev_broker ... env_logger::init", API_NAMESPACE);
    env_logger::try_init().unwrap();
    println!(
        "{} udev_broker ... env_logger::init finished",
        API_NAMESPACE
    );
    info!("{} Udev Broker logging started", API_NAMESPACE);

    tokio::spawn(async move {
        run_metrics_server().await.unwrap();
    });

    let env_var_query = ActualEnvVarQuery {};
    let devnode = get_video_devnode(&env_var_query);

    let camera_capturer = camera_capturer::build_and_start_camera_capturer(&devnode);
    camera_service::serve(&devnode, camera_capturer)
        .await
        .unwrap();

    trace!("Waiting for ctrl C shutdown signal");
    // Wait for exit signal
    signal::ctrl_c().await?;

    trace!("Udev broker ending");
    Ok(())
}

/// This gets video devnode from environment variable else panics.
fn get_video_devnode(env_var_query: &impl EnvVarQuery) -> String {
    trace!("get_video_devnode - getting devnode");

    // query UDEV_DEVNODE_LABEL_ID prefix and use the first one found as device_devnode
    lazy_static! {
        static ref RE: Regex = Regex::new(UDEV_DEVNODE_LABEL_ID_PATTERN).unwrap();
    }
    let device_devnodes = env_var_query
        .get_env_vars()
        .iter()
        .filter_map(|(n, v)| {
            if RE.is_match(n) {
                Some(v.clone())
            } else {
                None
            }
        })
        .collect::<Vec<String>>();
    let device_devnode = device_devnodes
        .first()
        .expect("devnode not set in envrionment variable");

    trace!("get_video_devnode - found devnode {}", device_devnode);
    device_devnode.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use akri_shared::os::env_var::MockEnvVarQuery;

    #[test]
    fn test_get_devnode() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mut mock_query = MockEnvVarQuery::new();
        const MOCK_DEVICE_PATH: &str = "/dev/video0";
        const MOCK_DEVICE_ENV_VAR_NAME: &str = "UDEV_DEVNODE_123456";
        mock_query
            .expect_get_env_vars()
            .times(1)
            .returning(move || {
                vec![(
                    MOCK_DEVICE_ENV_VAR_NAME.to_string(),
                    MOCK_DEVICE_PATH.to_string(),
                )]
            });

        assert_eq!(MOCK_DEVICE_PATH.to_string(), get_video_devnode(&mock_query));
    }
}
