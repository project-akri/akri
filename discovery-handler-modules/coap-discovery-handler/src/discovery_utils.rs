use coap::CoAPClient as Client;
use coap_lite::{CoapRequest, CoapResponse};
use log::{debug, info};
#[cfg(test)]
use mockall::{automock, predicate::*};
use std::net::{SocketAddr, ToSocketAddrs};
use std::time::Duration;

type Result<T> = std::io::Result<T>;

#[cfg_attr(test, automock)]
pub trait CoAPClient: Sized {
    fn get_with_timeout(&self, url: &str, timeout: Duration) -> Result<CoapResponse>;
    fn send_all_coap(&self, request: &CoapRequest<SocketAddr>, segment: u8) -> Result<()>;
    fn set_receive_timeout(&self, dur: Option<Duration>) -> Result<()>;
    fn receive_from(&self) -> Result<(CoapResponse, SocketAddr)>;
}

pub struct CoAPClientImpl {
    client: Client,
}

impl CoAPClientImpl {
    pub fn new<A: ToSocketAddrs>(addr: A) -> Self {
        CoAPClientImpl {
            client: Client::new(addr).unwrap(),
        }
    }
}

impl CoAPClient for CoAPClientImpl {
    fn get_with_timeout(&self, url: &str, timeout: Duration) -> Result<CoapResponse> {
        Client::get_with_timeout(url, timeout)
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

pub struct CoREResource {
    uri: String,
    interface: Option<String>,
    rtype: Option<String>,
}

impl CoREResource {
    fn new(uri: String) -> CoREResource {
        CoREResource {
            uri,
            interface: None,
            rtype: None,
        }
    }
}

pub fn parse_link_value(link_value: &str) -> Vec<(String, String)> {
    use coap_lite::link_format::LinkFormatParser;

    let mut parser = LinkFormatParser::new(link_value);
    let mut resources: Vec<(String, String)> = vec![];

    while let Some(Ok((uri, mut attr_it))) = parser.next() {
        debug!("Found CoAP resource {}", uri);
        let mut resource = CoREResource::new(uri.to_string());

        while let Some((attr, value)) = attr_it
            .next()
            .map(|(name, unquote)| (name, unquote.to_string()))
        {
            debug!("attr {} value {}", attr, value);

            match attr {
                "rt" => resource.rtype = Some(value),
                "if" if value.as_str() == "sensor" => {
                    // Only "sensor" interface is supported for now
                    resource.interface = Some(value)
                }
                _ => {
                    // Other attributes are not supported yet
                }
            }
        }

        if resource.interface.is_none() {
            // Unsupported interface
            continue;
        }

        if let Some(rtype) = resource.rtype {
            resources.push((resource.uri, rtype));
        }
    }

    info!("Parsed resources {:?}", resources);

    resources
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_resources() {
        let link = r#"</sensors/temp>;rt="oic.r.temperature";if="sensor""#;
        let resources = parse_link_value(link);

        let (uri, rtype) = resources.get(0).expect("No resources parsed");

        assert_eq!(rtype, "oic.r.temperature");
        assert_eq!(uri, "/sensors/temp");
    }
}
