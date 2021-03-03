use akri_debug_echo::{
    discovery_handler::DiscoveryHandler, INSTANCES_ARE_LOCAL_LABEL, PROTOCOL_NAME,
};
use akri_discovery_utils::discovery::discovery_handler::run_discovery_handler;
use log::info;
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    env_logger::try_init()?;
    info!("main - debugEcho discovery handler started");
    let (register_sender, register_receiver) = tokio::sync::mpsc::channel(2);
    let discovery_handler = DiscoveryHandler::new(Some(register_sender));
    let is_local: bool = std::env::var(INSTANCES_ARE_LOCAL_LABEL)
        .unwrap()
        .parse()
        .unwrap();
    run_discovery_handler(
        discovery_handler,
        register_receiver,
        PROTOCOL_NAME,
        is_local,
    )
    .await?;
    info!("main - debugEcho discovery handler ended");
    Ok(())
}
