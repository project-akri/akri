#[macro_use]
extern crate lazy_static;
mod util;

use akri_shared::{
    akri::{metrics::run_metrics_server, API_NAMESPACE},
    k8s::AKRI_CONFIGURATION_LABEL_NAME,
};
use futures::StreamExt;
use kube::runtime::{watcher::Config, Controller};
use prometheus::IntGaugeVec;
use std::sync::Arc;
use util::{
    context::{InstanceControllerContext, NodeWatcherContext, PodWatcherContext},
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

    // Start server for prometheus metrics
    tokio::spawn(run_metrics_server());
    let client = Arc::new(kube::Client::try_default().await?);
    let node_watcher_ctx = Arc::new(NodeWatcherContext::new(client.clone()));
    let pod_watcher_ctx = Arc::new(PodWatcherContext::new(client.clone()));

    node_watcher::check(client.clone()).await?;
    let node_controller = Controller::new(
        node_watcher_ctx.client.all().as_inner(),
        Config::default().any_semantic(),
    )
    .shutdown_on_signal()
    .run(
        node_watcher::reconcile,
        node_watcher::error_policy,
        node_watcher_ctx,
    )
    .filter_map(|x| async move { std::result::Result::ok(x) })
    .for_each(|_| futures::future::ready(()));

    pod_watcher::check(client.clone()).await?;
    let pod_controller = Controller::new(
        pod_watcher_ctx.client.all().as_inner(),
        Config::default().labels(AKRI_CONFIGURATION_LABEL_NAME),
    )
    .shutdown_on_signal()
    .run(
        pod_watcher::reconcile,
        pod_watcher::error_policy,
        pod_watcher_ctx,
    )
    .filter_map(|x| async move { std::result::Result::ok(x) })
    .for_each(|_| futures::future::ready(()));

    tokio::select! {
        _ = futures::future::join(node_controller, pod_controller) => {},
        _ = instance_action::run(Arc::new(InstanceControllerContext::new(client))) => {}
    }

    log::info!("{} Controller end", API_NAMESPACE);
    Ok(())
}
