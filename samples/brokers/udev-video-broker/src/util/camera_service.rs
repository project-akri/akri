use super::super::FRAME_COUNT_METRIC;
use super::camera::{
    camera_client::CameraClient,
    camera_server::{Camera, CameraServer},
    NotifyRequest, NotifyResponse,
};
use log::{info, trace};
use rscam::Camera as RsCamera;
use std::{
    net::SocketAddr,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub const CAMERA_SERVICE_SERVER_ADDRESS: &str = "0.0.0.0";
pub const CAMERA_SERVICE_TEST_LOCALHOST: &str = "127.0.0.1";
pub const CAMERA_SERVICE_PORT: &str = "8083";

/// gRPC service that serves frames from camera at `devnode` on request.
pub struct CameraService {
    /// v4l2 wrapper for grabbing frames from udev camera
    camera_capturer: RsCamera,
    /// device node of camera (ie /dev/video0)
    devnode: String,
}

#[tonic::async_trait]
impl Camera for CameraService {
    /// This gets a frame from the RsCamera and returns it.
    async fn get_frame(
        &self,
        _request: tonic::Request<NotifyRequest>,
    ) -> Result<tonic::Response<NotifyResponse>, tonic::Status> {
        trace!("CameraService.get_frame grpc request");
        FRAME_COUNT_METRIC.inc();
        Ok(tonic::Response::new(NotifyResponse {
            frame: {
                let frame = self.camera_capturer.capture().unwrap();
                (frame[..]).to_vec()
            },
            camera: self.devnode.clone(),
        }))
    }
}

/// This creates camera server
pub async fn serve(devnode: &str, camera_capturer: RsCamera) -> Result<(), String> {
    info!("Entered serve for camera service");
    let camera_service = CameraService {
        camera_capturer,
        devnode: devnode.to_string(),
    };
    let service = CameraServer::new(camera_service);

    let addr_str = format!("{}:{}", CAMERA_SERVICE_SERVER_ADDRESS, CAMERA_SERVICE_PORT);
    let addr: SocketAddr = match addr_str.parse() {
        Ok(sock) => sock,
        Err(e) => {
            return Err(format!("Unable to parse socket: {:?}", e));
        }
    };

    tokio::task::spawn(async move {
        trace!("Entered Server::builder task (addr: {})", &addr);
        tonic::transport::Server::builder()
            .add_service(service)
            .serve(addr)
            .await
            .expect("couldn't build server");
        trace!("Exit Server::builder task");
    });

    trace!("Wait for server to start up by polling its existence");
    // Test that server is running, trying for at most 10 seconds
    // Similar to grpc.timeout, which is yet to be implemented for tonic
    // See issue: https://github.com/hyperium/tonic/issues/75
    let mut connected = false;
    let start = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs();
    let start_plus_10 = start + 10;

    while (SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs()
        < start_plus_10)
        && !connected
    {
        let client_addr_str = format!(
            "http://{}:{}",
            CAMERA_SERVICE_TEST_LOCALHOST, CAMERA_SERVICE_PORT
        );
        connected = match CameraClient::connect(client_addr_str).await {
            Ok(_) => {
                trace!("Connected to server, stop polling");
                true
            }
            Err(e) => {
                trace!("Unable to connect to server, continue polling: {:?}", e);
                tokio::time::sleep(Duration::from_secs(1)).await;
                false
            }
        };
    }

    if !connected {
        Err(format!("Could not connect to Camera server {}", &addr_str))
    } else {
        Ok(())
    }
}
