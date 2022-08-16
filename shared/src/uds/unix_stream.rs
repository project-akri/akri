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

pub async fn try_connect(socket_path: &str) -> Result<(), anyhow::Error> {
    use std::convert::TryFrom;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
        let path = socket_path.to_string();
        // We will ignore this dummy uri because UDS does not use it.
        if let Ok(_v) = tonic::transport::Endpoint::try_from("http://[::]:50051")
            .map_err(|e| anyhow::format_err!("{}", e))?
            .connect_with_connector(tower::service_fn(move |_: tonic::transport::Uri| {
                tokio::net::UnixStream::connect(path.clone())
            }))
            .await
        {
            connected = true
        } else {
            tokio::time::sleep(Duration::from_secs(1)).await
        }
    }
    if connected {
        Ok(())
    } else {
        Err(anyhow::format_err!(
            "Could not connect to server on socket {}",
            socket_path
        ))
    }
}
