/// Module to enable UDS with tonic grpc.
/// This is unix only since the underlying UnixStream and UnixListener libraries are unix only.
use std::{
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tonic::transport::server::Connected;

#[derive(Debug)]
pub struct UnixStream(pub tokio::net::UnixStream);

impl Connected for UnixStream {
    type ConnectInfo = UdsConnectInfo;

    fn connect_info(&self) -> Self::ConnectInfo {
        UdsConnectInfo {
            peer_addr: self.0.peer_addr().ok().map(Arc::new),
            peer_cred: self.0.peer_cred().ok(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct UdsConnectInfo {
    pub peer_addr: Option<Arc<tokio::net::unix::SocketAddr>>,
    pub peer_cred: Option<tokio::net::unix::UCred>,
}

impl AsyncRead for UnixStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.0).poll_read(cx, buf)
    }
}

impl AsyncWrite for UnixStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.0).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.0).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.0).poll_shutdown(cx)
    }
}

pub async fn try_connect(socket_path: &str) -> anyhow::Result<()> {
    use anyhow::Context;
    use std::time::{Duration, SystemTime};

    // We will ignore this dummy uri because UDS does not use it.
    // Some servers will check the uri content so the uri needs to
    // be in valid format even it's not used, the scheme part is used
    // to specific what scheme to use, such as http or https
    let endpoint = tonic::transport::Endpoint::from_static("http://[::1]:50051");

    // Test that server is running, trying for at most 10 seconds
    // Similar to grpc.timeout, which is yet to be implemented for tonic
    // See issue: https://github.com/hyperium/tonic/issues/75
    let start = SystemTime::now();

    loop {
        let path_connector = tower::service_fn({
            let socket_path = socket_path.to_string();
            move |_: tonic::transport::Uri| tokio::net::UnixStream::connect(socket_path.clone())
        });

        if let Err(e) = endpoint.connect_with_connector(path_connector).await {
            let elapsed = start.elapsed().expect("System time should be monotonic");
            if elapsed.as_secs() < 10 {
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
            return Err(e).context("After trying for at least 10 seconds");
        }

        return Ok(());
    }
}
