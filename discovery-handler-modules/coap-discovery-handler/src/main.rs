mod discovery_handler;
mod discovery_utils;

#[macro_use]
extern crate serde_derive;

use akri_discovery_utils::discovery::discovery_handler::run_discovery_handler;
use discovery_handler::DiscoveryHandlerImpl;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    env_logger::try_init()?;

    let name = "coap";
    let shared = true;
    let (register_sender, register_receiver) = tokio::sync::mpsc::channel(2);
    let discovery_handler = DiscoveryHandlerImpl::new(register_sender);

    run_discovery_handler(discovery_handler, register_receiver, name, shared).await?;

    Ok(())
}
