#[macro_use]
extern crate log;
#[macro_use]
extern crate yaserde_derive;
#[macro_use]
extern crate serde_derive;

extern crate pest;
#[macro_use]
extern crate pest_derive;

extern crate hyper;
extern crate tokio_core;

mod protocols;
mod util;

use akri_shared::akri::API_NAMESPACE;
use env_logger;
use log::{info, trace};
use std::time::Duration;
use util::{
    config_action, constants::SLOT_RECONCILIATION_SLOT_GRACE_PERIOD_SECS,
    slot_reconciliation::periodic_slot_reconciliation,
};

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
