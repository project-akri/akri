use super::{
    nessie::{
        nessie_server::{Nessie, NessieServer},
        NotifyRequest, NotifyResponse,
    },
    FrameBuffer,
};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tonic::{transport::Server, Request, Response};

pub const NESSIE_SERVER_ADDRESS: &str = "0.0.0.0";
pub const NESSIE_SERVER_PORT: &str = "8083";

pub struct NessieService {
    frame_rx: Arc<Mutex<FrameBuffer>>,
}

#[tonic::async_trait]
impl Nessie for NessieService {
    async fn get_nessie_now(
        &self,
        _request: Request<NotifyRequest>,
    ) -> Result<Response<NotifyResponse>, tonic::Status> {
        Ok(Response::new(NotifyResponse {
            frame: match self.frame_rx.lock().unwrap().pop_front() {
                Some(data) => data,
                _ => vec![],
            },
        }))
    }
}

pub async fn serve(frame_rx: Arc<Mutex<FrameBuffer>>) -> Result<(), String> {
    let nessie = NessieService { frame_rx };
    let service = NessieServer::new(nessie);

    let addr_str = format!("{}:{}", NESSIE_SERVER_ADDRESS, NESSIE_SERVER_PORT);
    let addr: SocketAddr = match addr_str.parse() {
        Ok(sock) => sock,
        Err(e) => {
            return Err(format!("Unable to parse socket: {:?}", e));
        }
    };

    tokio::spawn(async move {
        Server::builder()
            .add_service(service)
            .serve(addr)
            .await
            .expect("couldn't build server");
    });
    Ok(())
}
