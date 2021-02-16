use akri_discovery_utils::{
    discovery::{server::run_discovery_server, DISCOVERY_HANDLER_PATH},
    registration_client::register,
};
use akri_udev::{
    discovery_handler::{DiscoveryHandler, DISCOVERY_PORT},
    get_register_request,
};
use log::{info, trace};
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    env_logger::try_init()?;
    info!("main - udev discovery handler started");
    let mut use_uds = true;
    let mut endpoint: String = match std::env::var("POD_IP") {
        Ok(pod_ip) => {
            trace!("main - registering with Agent with IP endpoint");
            use_uds = false;
            format!("{}:{}", pod_ip, DISCOVERY_PORT)
        }
        Err(_) => {
            trace!("main - registering with Agent with uds endpoint");
            format!("{}/udev.sock", DISCOVERY_HANDLER_PATH)
        }
    };
    let (shutdown_sender, shutdown_receiver) = tokio::sync::mpsc::channel(2);
    let endpoint_clone = endpoint.clone();
    let handle = tokio::spawn(async move {
        run_discovery_server(
            DiscoveryHandler::new(Some(shutdown_sender)),
            &endpoint_clone,
            shutdown_receiver,
        )
        .await
        .unwrap();
    });
    if !use_uds {
        endpoint.insert_str(0, "http://");
    }
    register(get_register_request(&endpoint)).await?;
    handle.await?;
    info!("main - udev discovery handler ended");
    Ok(())
}
