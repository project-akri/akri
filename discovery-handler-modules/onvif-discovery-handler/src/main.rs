use akri_discovery_utils::discovery::discovery_handler::{
    run_discovery_handler, REGISTER_AGAIN_CHANNEL_CAPACITY,
};
use akri_onvif::{discovery_handler::DiscoveryHandlerImpl, DISCOVERY_HANDLER_NAME, SHARED};
use log::info;
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    env_logger::try_init()?;
    info!("main - onvif discovery handler started");
    let (register_sender, register_receiver) =
        tokio::sync::mpsc::channel(REGISTER_AGAIN_CHANNEL_CAPACITY);
    let discovery_handler = DiscoveryHandlerImpl::new(Some(register_sender));
    run_discovery_handler(
        discovery_handler,
        register_receiver,
        DISCOVERY_HANDLER_NAME,
        SHARED,
    )
    .await?;
    info!("main - onvif discovery handler ended");
    Ok(())
}
