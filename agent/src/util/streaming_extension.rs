use akri_discovery_utils::discovery::v0::DiscoverResponse;
use async_trait::async_trait;
use tokio::sync::mpsc;
use tonic::{Code, Status};

/// An extension trait that is used to get the latest message from both embedded and
/// external Discovery Handlers' streams.
#[async_trait]
pub trait StreamingExt: Send {
    async fn get_message(&mut self) -> Result<Option<DiscoverResponse>, Status>;
}

#[async_trait]
impl StreamingExt for mpsc::Receiver<Result<DiscoverResponse, Status>> {
    async fn get_message(&mut self) -> Result<Option<DiscoverResponse>, Status> {
        match self.recv().await {
            Some(result) => match result {
                Ok(res) => Ok(Some(res)),
                Err(e) => Err(e),
            },
            None => Err(Status::new(Code::Unavailable, "broken pipe")),
        }
    }
}

#[async_trait]
impl StreamingExt for tonic::codec::Streaming<DiscoverResponse> {
    async fn get_message(&mut self) -> Result<Option<DiscoverResponse>, Status> {
        self.message().await
    }
}
