use super::discovery::v0::{
    registration_client::RegistrationClient, RegisterDiscoveryHandlerRequest,
};
use log::{info, trace};
use std::convert::TryFrom;
use tonic::{
    transport::{Endpoint, Uri},
    Request,
};

pub async fn register_discovery_handler(
    register_request: &RegisterDiscoveryHandlerRequest,
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    info!("register_discovery_handler - entered");
    loop {
        // We will ignore this dummy uri because UDS does not use it.
        if let Ok(channel) = Endpoint::try_from("http://[::]:50051")?
            .connect_with_connector(tower::service_fn(move |_: Uri| {
                tokio::net::UnixStream::connect(super::get_registration_socket())
            }))
            .await
        {
            let mut client = RegistrationClient::new(channel);
            let request = Request::new(register_request.clone());
            client.register_discovery_handler(request).await?;
            break;
        }
        trace!("register_discovery_handler - sleeping for 10 seconds and trying again");
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    }
    Ok(())
}

/// Continually waits for message to re-register with an Agent
pub async fn register_discovery_handler_again(
    mut register_receiver: tokio::sync::mpsc::Receiver<()>,
    register_request: &RegisterDiscoveryHandlerRequest,
) {
    loop {
        match register_receiver.recv().await {
            Some(_) => {
                info!("register_again - received signal ... registering with Agent again");
                register_discovery_handler(register_request).await.unwrap();
            }
            None => {
                info!("register_again - connection to register_again_sender closed ... error")
            }
        }
    }
}
