mod util;
#[macro_use]
extern crate lazy_static;
use akri_shared::{
    akri::API_NAMESPACE,
    os::{
        env_var::{ActualEnvVarQuery, EnvVarQuery},
        signal,
    },
};
use futures::Future;
use log::{error, info, trace};
use prometheus::{IntCounter, Registry};
use util::{camera_capturer, camera_service};
use warp::{Filter, Rejection, Reply};

lazy_static! {
    pub static ref REGISTRY: Registry = Registry::new();
    pub static ref FRAME_COUNT: IntCounter = IntCounter::new("frame_count", "Frame Count")
        .expect("frame_count metric cannot be created");
}

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

    register_custom_metrics();

    // Set up metrics server
    let metrics_route = warp::path!("metrics").and_then(metrics_handler);

    trace!("Starting metrics server on port 8080 at /metrics");
    tokio::task::spawn(async move {
        warp::serve(metrics_route).run(([0, 0, 0, 0], 8080)).await;
    });

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
    // tokio::task::spawn(fake_get_frame());

    trace!("Waiting for shutdown signal");
    // wait for exit signal
    exit_rx.recv().unwrap();

    trace!("Udev broker ending");
    Ok(())
}

pub fn register_custom_metrics() {
    REGISTRY
        .register(Box::new(FRAME_COUNT.clone()))
        .expect("frame count cannot be registered");
}

async fn metrics_handler() -> Result<impl Reply, Rejection> {
    use prometheus::Encoder;
    let encoder = prometheus::TextEncoder::new();

    let mut buffer = Vec::new();
    if let Err(e) = encoder.encode(&REGISTRY.gather(), &mut buffer) {
        error!("could not encode custom metrics: {}", e);
    };
    let mut res = match String::from_utf8(buffer.clone()) {
        Ok(v) => v,
        Err(e) => {
            error!("custom metrics could not be from_utf8'd: {}", e);
            String::default()
        }
    };
    buffer.clear();

    let mut buffer = Vec::new();
    if let Err(e) = encoder.encode(&prometheus::gather(), &mut buffer) {
        error!("could not encode prometheus metrics: {}", e);
    };
    let res_custom = match String::from_utf8(buffer.clone()) {
        Ok(v) => v,
        Err(e) => {
            error!("prometheus metrics could not be from_utf8'd: {}", e);
            String::default()
        }
    };
    buffer.clear();

    res.push_str(&res_custom);
    Ok(res)
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
