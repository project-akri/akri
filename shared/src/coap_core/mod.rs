use coap::CoAPClient as ImplClient;
use coap_lite::{CoapRequest, CoapResponse};
use mockall::{automock, predicate::*};
use std::net::{SocketAddr, ToSocketAddrs};
use std::time::Duration;

type Result<T> = std::io::Result<T>;

#[automock]
pub trait CoAPClient: Sized {
    fn get_with_timeout(&self, url: &str, timeout: Duration) -> Result<CoapResponse>;
    fn send_all_coap(&self, request: &CoapRequest<SocketAddr>, segment: u8) -> Result<()>;
    fn set_receive_timeout(&self, dur: Option<Duration>) -> Result<()>;
    fn receive_from(&self) -> Result<(CoapResponse, SocketAddr)>;
}

pub struct CoAPClientImpl {
    client: ImplClient,
}

impl CoAPClientImpl {
    pub fn new<A: ToSocketAddrs>(addr: A) -> Self {
        CoAPClientImpl {
            client: ImplClient::new(addr).unwrap(),
        }
    }
}

impl CoAPClient for CoAPClientImpl {
    fn get_with_timeout(&self, url: &str, timeout: Duration) -> Result<CoapResponse> {
        ImplClient::get_with_timeout(url, timeout)
    }

    fn send_all_coap(&self, request: &CoapRequest<SocketAddr>, segment: u8) -> Result<()> {
        self.client.send_all_coap(request, segment)
    }

    fn set_receive_timeout(&self, dur: Option<Duration>) -> Result<()> {
        self.client.set_receive_timeout(dur)
    }

    fn receive_from(&self) -> Result<(CoapResponse, SocketAddr)> {
        self.client.receive_from()
    }
}
