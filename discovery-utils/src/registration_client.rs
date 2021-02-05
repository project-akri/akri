use super::discovery::v0::{RegisterRequest, registration_client::RegistrationClient};
use log::{error, info, trace};
use tonic::Request;

/// Agent registration endpoint for external discovery handlers
pub const AGENT_REGISTRATION_ENDPOINT: &str = "http://[::1]:10000";

pub async fn register(register_request: RegisterRequest)  -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    info!("register - entered");
    loop {
        if let Ok(mut client) = RegistrationClient::connect(AGENT_REGISTRATION_ENDPOINT).await {
            let request = Request::new(register_request.clone());
            client.register(request).await?;
            break;
        }
        info!("register - sleeping for 10 seconds and trying again");
        tokio::time::delay_for(std::time::Duration::from_secs(10)).await;
    }
    Ok(())
} 
