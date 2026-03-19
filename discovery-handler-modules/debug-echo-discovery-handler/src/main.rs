use akri_debug_echo::{
    DEBUG_ECHO_INSTANCES_SHARED_LABEL, DISCOVERY_HANDLER_NAME,
    discovery_handler::DiscoveryHandlerImpl,
};
use akri_discovery_utils::discovery::discovery_handler::{
    REGISTER_AGAIN_CHANNEL_CAPACITY, run_discovery_handler,
};
use log::info;
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    env_logger::try_init()?;
    info!("main - debugEcho discovery handler started");
    let (register_sender, register_receiver) =
        tokio::sync::mpsc::channel(REGISTER_AGAIN_CHANNEL_CAPACITY);
    let discovery_handler = DiscoveryHandlerImpl::new(Some(register_sender));
    let shared: bool = std::env::var(DEBUG_ECHO_INSTANCES_SHARED_LABEL)
        .unwrap()
        .parse()
        .unwrap();
    run_discovery_handler(
        discovery_handler,
        register_receiver,
        DISCOVERY_HANDLER_NAME,
        shared,
    )
    .await?;
    info!("main - debugEcho discovery handler ended");
    Ok(())
}
