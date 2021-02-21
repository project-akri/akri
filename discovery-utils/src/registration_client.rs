use super::discovery::{
    v0::{registration_client::RegistrationClient, RegisterRequest},
    AGENT_REGISTRATION_SOCKET,
};
use log::{info, trace};
use std::convert::TryFrom;
use tonic::{
    transport::{Endpoint, Uri},
    Request,
};

pub async fn register(
    register_request: RegisterRequest,
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    info!("register - entered");
    loop {
        // lttp://... is a fake uri that is unused (in service_fn) but necessary for uds connection
        if let Ok(channel) = Endpoint::try_from("lttp://[::]:50051")?
            .connect_with_connector(tower::service_fn(|_: Uri| {
                tokio::net::UnixStream::connect(AGENT_REGISTRATION_SOCKET)
            }))
            .await
        {
            let mut client = RegistrationClient::new(channel);
            let request = Request::new(register_request.clone());
            client.register(request).await?;
            break;
        }
        trace!("register - sleeping for 10 seconds and trying again");
        tokio::time::delay_for(std::time::Duration::from_secs(10)).await;
    }
    Ok(())
}