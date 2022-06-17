/// Wrapper to enable mocking of OPC UA Client
pub mod opcua_client_wrapper {
    #[cfg(test)]
    use mockall::{automock, predicate::*};
    use opcua::client::prelude::*;

    #[cfg_attr(test, automock)]
    pub trait OpcuaClient {
        fn find_servers(
            &mut self,
            discovery_endpoint_url: &str,
        ) -> Result<Vec<ApplicationDescription>, StatusCode>;
    }

    pub struct OpcuaClientImpl {
        inner_opcua_client: Client,
    }

    impl OpcuaClientImpl {
        fn new(
            application_name: &str,
            application_uri: &str,
            create_sample_keypair: bool,
            session_retry_limit: i32,
        ) -> Self {
            OpcuaClientImpl {
                inner_opcua_client: ClientBuilder::new()
                    .application_name(application_name)
                    .application_uri(application_uri)
                    .create_sample_keypair(create_sample_keypair)
                    .session_retry_limit(session_retry_limit)
                    .client()
                    .unwrap(),
            }
        }
    }

    impl OpcuaClient for OpcuaClientImpl {
        fn find_servers(
            &mut self,
            discovery_endpoint_url: &str,
        ) -> Result<Vec<ApplicationDescription>, StatusCode> {
            self.inner_opcua_client.find_servers(discovery_endpoint_url)
        }
    }
    /// Returns an OPC UA Client that will only be used to connect to OPC UA Server and Local Discovery Servers' DiscoveryEndpoints
    pub fn create_opcua_discovery_client() -> impl OpcuaClient {
        // Automatically create a self-signed private key and public cert.
        // This will not be used to authenticate as only public endpoints will be accessed by the discovery client.
        let create_sample_keypair = true;
        // Do not try to create a session again
        let session_retry_limit = 0;
        OpcuaClientImpl::new(
            "DiscoveryHandlerClient",
            "urn:DiscoveryHandlerClient",
            create_sample_keypair,
            session_retry_limit,
        )
    }
}
pub mod tcp_stream_wrapper {
    #[cfg(test)]
    use mockall::{automock, predicate::*};
    use std::{
        io,
        net::{SocketAddr, TcpStream as StdTcpStream},
        time::Duration,
    };

    #[cfg_attr(test, automock)]
    pub trait TcpStream {
        fn connect_timeout(&self, addr: &SocketAddr, timeout: Duration) -> io::Result<()>;
    }

    pub struct TcpStreamImpl {}

    impl TcpStream for TcpStreamImpl {
        fn connect_timeout(&self, addr: &SocketAddr, timeout: Duration) -> io::Result<()> {
            // Do not need to return the stream since it is not used, so map success to Ok(())
            StdTcpStream::connect_timeout(addr, timeout).map(|_| ())
        }
    }
}
