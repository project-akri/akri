mod discovery_handler;
mod discovery_utils;

#[macro_use]
extern crate serde_derive;

use akri_discovery_utils::discovery::discovery_handler::run_discovery_handler;
use discovery_handler::DiscoveryHandlerImpl;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    env_logger::try_init()?;

    // Specify the name of this DiscoveryHandler. A discovery handler is usually, but not necessarily, identified by
    // the protocol it uses.
    let name = "coap";
    // Specify whether the devices discovered by this discovery handler are locally attached (or embedded) to nodes or are
    // network based and usable/sharable by multiple nodes.
    let shared = false;
    // A DiscoveryHandler must handle the Agent dropping a connection due to a Configuration that utilizes this
    // DiscoveryHandler being deleted or the Agent erroring. It is impossible to determine the cause of the
    // disconnection, so in case the Agent did error out, the Discovery Handler should try to re-register.
    let (register_sender, register_receiver) = tokio::sync::mpsc::channel(2);
    // Create a DiscoveryHandler
    let discovery_handler = DiscoveryHandlerImpl::new(register_sender);
    // This function will register the DiscoveryHandler with the Agent's registration socket
    // and serve its discover service over UDS at the socket path
    // `format!("{}/{}.sock"), env::var("DISCOVERY_HANDLERS_DIRECTORY"), name)`.
    run_discovery_handler(discovery_handler, register_receiver, name, shared).await?;
    Ok(())
}
