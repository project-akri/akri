extern crate hyper;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
#[cfg(feature = "udev-feat")]
extern crate pest;
#[cfg(feature = "udev-feat")]
#[macro_use]
extern crate pest_derive;
#[macro_use]
extern crate serde_derive;
extern crate tokio_core;
#[cfg(feature = "onvif-feat")]
#[macro_use]
extern crate yaserde_derive;

mod protocols;
mod util;

use akri_shared::akri::{metrics::run_metrics_server, API_NAMESPACE};
use log::{info, trace};
use prometheus::{HistogramVec, IntGaugeVec};
use std::time::Duration;
use util::{
    config_action, constants::SLOT_RECONCILIATION_SLOT_GRACE_PERIOD_SECS,
    slot_reconciliation::periodic_slot_reconciliation,
};

lazy_static! {
    // Reports the number of Instances visible to this node, grouped by Configuration and whether it is shared
    pub static ref INSTANCE_COUNT_METRIC: IntGaugeVec = prometheus::register_int_gauge_vec!("akri_instance_count", "Akri Instance Count", &["configuration", "is_shared"]).unwrap();
    // Reports the time to get discovery results, grouped by Configuration
    pub static ref DISCOVERY_RESPONSE_TIME_METRIC: HistogramVec = prometheus::register_histogram_vec!("akri_discovery_response_time", "Akri Discovery Response Time", &["configuration"]).unwrap();
}
/// This is the entry point for the Akri Agent.
/// It must be built on unix systems, since the underlying libraries for the `DevicePluginService` unix socket connection are unix only.
#[cfg(unix)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    println!("{} Agent start", API_NAMESPACE);

    println!(
        "{} KUBERNETES_PORT found ... env_logger::init",
        API_NAMESPACE
    );
    env_logger::try_init()?;
    trace!(
        "{} KUBERNETES_PORT found ... env_logger::init finished",
        API_NAMESPACE
    );

    let mut tasks = Vec::new();

    // Start server for prometheus metrics
    tasks.push(tokio::spawn(async move {
        run_metrics_server().await.unwrap();
    }));

    tasks.push(tokio::spawn(async move {
        let slot_grace_period = Duration::from_secs(SLOT_RECONCILIATION_SLOT_GRACE_PERIOD_SECS);
        periodic_slot_reconciliation(slot_grace_period)
            .await
            .unwrap();
    }));

    tasks.push(tokio::spawn(async move {
        config_action::do_config_watch().await.unwrap()
    }));

    futures::future::try_join_all(tasks).await?;
    info!("{} Agent end", API_NAMESPACE);
    Ok(())
}
