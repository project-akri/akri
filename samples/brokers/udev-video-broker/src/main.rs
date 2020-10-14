mod util;

use akri_shared::{
    akri::API_NAMESPACE,
    os::{
        env_var::{ActualEnvVarQuery, EnvVarQuery},
        signal,
    },
};
use futures::Future;
use log::{info, trace};
use util::{camera_capturer, camera_service};

/// devnode environment variable id
pub const UDEV_DEVNODE_LABEL_ID: &str = "UDEV_DEVNODE";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    println!("{} udev_broker ... env_logger::init", API_NAMESPACE);
    env_logger::try_init().unwrap();
    println!(
        "{} udev_broker ... env_logger::init finished",
        API_NAMESPACE
    );
    info!("{} Udev Broker logging started", API_NAMESPACE);

    // Set up shutdown channel
    let (exit_tx, exit_rx) = std::sync::mpsc::channel::<()>();
    let _shutdown_signal = signal::shutdown().then(|_| {
        trace!("{} Udev Broker shutdown signal received", API_NAMESPACE);
        exit_tx.send(())
    });

    let env_var_query = ActualEnvVarQuery {};
    let devnode = get_video_devnode(&env_var_query);

    // let frames_per_second = get_frames_per_second(&env_var_query);
    // let resolution = get_resolution(&env_var_query);
    let camera_capturer = camera_capturer::build_and_start_camera_capturer(&devnode);
    camera_service::serve(&devnode, camera_capturer)
        .await
        .unwrap();

    trace!("Waiting for shutdown signal");
    // wait for exit signal
    exit_rx.recv().unwrap();

    trace!("Udev broker ending");
    Ok(())
}

/// This gets video devnode from environment variable else panics.
fn get_video_devnode(env_var_query: &impl EnvVarQuery) -> String {
    trace!("get_video_devnode - getting devnode");

    let device_devnode = env_var_query
        .get_env_var(UDEV_DEVNODE_LABEL_ID)
        .expect("devnode not set in envrionment variable");

    trace!("get_video_devnode - found devnode {}", device_devnode);
    device_devnode
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

        mock_query
            .expect_get_env_var()
            .times(1)
            .withf(move |name: &str| name == UDEV_DEVNODE_LABEL_ID)
            .returning(move |_| Ok(MOCK_DEVICE_PATH.to_string()));

        assert_eq!(MOCK_DEVICE_PATH.to_string(), get_video_devnode(&mock_query));
    }
}
