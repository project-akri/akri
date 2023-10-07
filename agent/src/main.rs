extern crate hyper;
#[macro_use]
extern crate log;
#[macro_use]
extern crate serde_derive;
mod util;

use akri_shared::akri::{metrics::run_metrics_server, API_NAMESPACE};
use log::{info, trace};
use std::{
    collections::HashMap,
    env,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::broadcast;
#[cfg(feature = "agent-full")]
use util::registration::register_embedded_discovery_handlers;
use util::{
    config_action,
    constants::{
        NEW_DISCOVERY_HANDLER_CHANNEL_CAPACITY, SLOT_RECONCILIATION_SLOT_GRACE_PERIOD_SECS,
    },
    registration::{run_registration_server, DiscoveryHandlerName},
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
    let node_name = env::var("AGENT_NODE_NAME")?;

    // Start server for Prometheus metrics
    tasks.push(tokio::spawn(async move {
        run_metrics_server().await.unwrap();
    }));

    let discovery_handler_map = Arc::new(Mutex::new(HashMap::new()));
    let discovery_handler_map_clone = discovery_handler_map.clone();
    let (new_discovery_handler_sender, _): (
        broadcast::Sender<DiscoveryHandlerName>,
        broadcast::Receiver<DiscoveryHandlerName>,
    ) = broadcast::channel(NEW_DISCOVERY_HANDLER_CHANNEL_CAPACITY);
    let new_discovery_handler_sender_clone = new_discovery_handler_sender.clone();
    #[cfg(feature = "agent-full")]
    register_embedded_discovery_handlers(discovery_handler_map_clone.clone())?;

    // Start registration service for registering `DiscoveryHandlers`
    tasks.push(tokio::spawn(async move {
        run_registration_server(discovery_handler_map_clone, new_discovery_handler_sender)
            .await
            .unwrap();
    }));

    tasks.push(tokio::spawn(async move {
        let slot_grace_period = Duration::from_secs(SLOT_RECONCILIATION_SLOT_GRACE_PERIOD_SECS);
        periodic_slot_reconciliation(slot_grace_period)
            .await
            .unwrap();
    }));

    tasks.push(tokio::spawn(async move {
        config_action::do_config_watch(
            discovery_handler_map,
            new_discovery_handler_sender_clone,
            node_name,
        )
        .await
        .unwrap()
    }));

    futures::future::try_join_all(tasks).await?;
    info!("{} Agent end", API_NAMESPACE);
    Ok(())
}
