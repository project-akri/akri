extern crate hyper;
#[macro_use]
extern crate log;
#[macro_use]
extern crate serde_derive;
mod device_manager;
mod discovery_handler_manager;
mod plugin_manager;
mod util;

use akri_shared::akri::{API_NAMESPACE, metrics::run_metrics_server};
use log::{info, trace};
use std::{
    collections::HashMap,
    env,
    sync::{Arc, Mutex},
};

/// This is the entry point for the Akri Agent.
/// It must be built on unix systems, since the underlying libraries for the `DevicePluginService` unix socket connection are unix only.
#[cfg(unix)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    println!("{API_NAMESPACE} Agent start");

    println!("{API_NAMESPACE} KUBERNETES_PORT found ... env_logger::init");
    env_logger::try_init()?;
    trace!("{API_NAMESPACE} KUBERNETES_PORT found ... env_logger::init finished");

    let mut tasks = Vec::new();
    let node_name = env::var("AGENT_NODE_NAME")?;

    {
        let kube_client = Arc::new(kube::Client::try_default().await?);

        // Start server for Prometheus metrics
        tasks.push(tokio::spawn(async move {
            run_metrics_server().await.unwrap();
        }));

        let (device_notifier, discovery_handler_registry, config_notifier) =
            discovery_handler_manager::new_registry(kube_client.clone());

        let dh_registry = Arc::new(discovery_handler_registry);
        let local_dh_reg = dh_registry.clone();
        let local_node_name = node_name.clone();

        tasks.push(tokio::spawn(async {
            discovery_handler_manager::run_registration_server(
                local_dh_reg,
                &akri_discovery_utils::get_registration_socket(),
                local_node_name,
            )
            .await
            .unwrap()
        }));

        let im_device_manager = Arc::new(device_manager::InMemoryManager::new(device_notifier));

        let device_plugin_manager = Arc::new(
            plugin_manager::device_plugin_instance_controller::DevicePluginManager::new(
                node_name.clone(),
                kube_client.clone(),
                im_device_manager.clone(),
            ),
        );

        let (instances_cache, device_plugin_controller_task) =
            plugin_manager::device_plugin_instance_controller::start_dpm(
                device_plugin_manager.clone(),
            );
        tasks.push(device_plugin_controller_task);

        tasks.push(tokio::spawn(
            plugin_manager::device_plugin_slot_reclaimer::start_reclaimer(device_plugin_manager),
        ));

        let config_controller_context = Arc::new(
            util::discovery_configuration_controller::ControllerContext {
                instances_cache,
                dh_registry,
                client: kube_client.clone(),
                agent_identifier: node_name.clone(),
                error_backoffs: Mutex::new(HashMap::new()),
            },
        );

        tasks.push(tokio::spawn(async {
            util::discovery_configuration_controller::start_controller(
                config_controller_context,
                config_notifier,
            )
            .await;
        }));
    }

    futures::future::try_join_all(tasks).await?;
    info!("{API_NAMESPACE} Agent end");
    Ok(())
}
