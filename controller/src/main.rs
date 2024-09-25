#[macro_use]
extern crate lazy_static;
mod util;

use akri_shared::akri::{metrics::run_metrics_server, API_NAMESPACE};
use prometheus::IntGaugeVec;
use std::sync::Arc;
use util::{
    controller_ctx::{ControllerContext, CONTROLLER_FIELD_MANAGER_ID},
    instance_action, node_watcher, pod_watcher,
};

/// Length of time to sleep between controller system validation checks
pub const SYSTEM_CHECK_DELAY_SECS: u64 = 30;

lazy_static! {
    // Reports the number of Broker pods running, grouped by Configuration and Node
    pub static ref BROKER_POD_COUNT_METRIC: IntGaugeVec = prometheus::register_int_gauge_vec!("akri_broker_pod_count", "Akri Broker Pod Count", &["configuration", "node"]).unwrap();
}

/// This is the entry point for the controller.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    println!("{} Controller start", API_NAMESPACE);

    println!(
        "{} KUBERNETES_PORT found ... env_logger::init",
        API_NAMESPACE
    );
    env_logger::try_init()?;
    println!(
        "{} KUBERNETES_PORT found ... env_logger::init finished",
        API_NAMESPACE
    );

    log::info!("{} Controller logging started", API_NAMESPACE);
    let mut tasks = Vec::new();

    // Start server for prometheus metrics
    tokio::spawn(run_metrics_server());

    let controller_ctx = Arc::new(ControllerContext::new(
        Arc::new(kube::Client::try_default().await?),
        CONTROLLER_FIELD_MANAGER_ID,
    ));
    let instance_water_ctx = controller_ctx.clone();
    let node_watcher_ctx = controller_ctx.clone();
    let pod_watcher_ctx = controller_ctx.clone();

    // Handle instance changes
    tasks.push(tokio::spawn(async {
        instance_action::run(instance_water_ctx).await;
    }));
    // Watch for node disappearance
    tasks.push(tokio::spawn(async {
        node_watcher::run(node_watcher_ctx).await;
    }));
    // Watch for broker Pod state changes
    tasks.push(tokio::spawn(async {
        pod_watcher::run(pod_watcher_ctx).await;
    }));

    futures::future::try_join_all(tasks).await?;

    log::info!("{} Controller end", API_NAMESPACE);
    Ok(())
}
