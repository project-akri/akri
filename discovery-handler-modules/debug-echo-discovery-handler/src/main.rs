use akri_debug_echo::{discovery_handler::{DISCOVERY_ENDPOINT, run_debug_echo_server}, get_register_request};
use akri_discovery_utils::registration_client::register;
use log::{info, trace};
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    env_logger::try_init()?;
    info!("main - debugEcho discovery handler started");
    let handle = tokio::spawn( async move {
        run_debug_echo_server().await.unwrap();
    });
    let endpoint = &format!("http://{}", DISCOVERY_ENDPOINT);
    register(get_register_request(endpoint)).await?;
    handle.await?;
    info!("main - debugEcho discovery handler ended");
    Ok(())
}
