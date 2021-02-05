use akri_discovery_utils::discovery::v0::{RegisterRequest, registration_client::RegistrationClient};
use super::discovery_handler::DISCOVERY_ENDPOINT;
use log::{error, info, trace};
use tonic::Request;

pub const REGISTRATION_ENDPOINT: &str = "http://[::1]:10000";

pub async fn register()  -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    info!("register - entered");
    let mut client = RegistrationClient::connect(REGISTRATION_ENDPOINT).await?;
    let request = Request::new(RegisterRequest {
        protocol: "debugEcho".to_string(),
        endpoint: format!("http://{}", DISCOVERY_ENDPOINT),
        is_local: true,
    });
    client.register(request).await?;
    Ok(())
}