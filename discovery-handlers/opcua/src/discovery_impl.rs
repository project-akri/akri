use super::wrappers::{
    opcua_client_wrapper::{create_opcua_discovery_client, OpcuaClient},
    tcp_stream_wrapper::{TcpStream, TcpStreamImpl},
};
use ::url::Url;
use akri_discovery_utils::filtering::{should_include, FilterList};
use anyhow::Context;
use log::{error, info, trace};
use opcua::client::prelude::*;
use opcua::core::constants::DEFAULT_OPC_UA_SERVER_PORT;
use std::{
    net::{SocketAddr, ToSocketAddrs},
    time::Duration,
};

/// Timeout for testing TCP connection to OPC UA Server or LDS DiscoveryEndpoint
/// Used when testing TCP connection before calling FindServers on the endpoint
const TCP_CONNECTION_TEST_TIMEOUT_SECS: u64 = 3;

/// `standard` is the only `OpcuaDiscoveryMethod` currently implemented, which takes in a set of DiscoveryURLs and discovers all the servers at those DiscoveryURLs.
///
/// Every OPC UA server/application has a DiscoveryEndpoint that Clients can access without establishing a session.
/// The address for this endpoint is defined by a DiscoveryURL.
/// However, if this DiscoveryURL is not known, the client can query a DiscoveryServer to get a set of servers' DiscoveryURLs.
/// A DiscoveryServer is "an Application that maintains a list of OPC UA Servers that are available on the network and
/// provides mechanisms for Clients to obtain this list" (OPC UA Specification 12). A LocalDiscoveryServer is an implementation
/// of an OPC UA DiscoveryServer.
/// `do_standard_discovery` creates an OPC UA Discovery Client and calls get_discovery_urls, passing in the DiscoveryURLs provided
/// in the OPC UA Configuration.
pub fn do_standard_discovery(
    discovery_urls: Vec<String>,
    filter_list: Option<FilterList>,
) -> Vec<String> {
    info!(
        "do_standard_discovery - for DiscoveryUrls {:?}",
        discovery_urls
    );
    let mut discovery_handler_client = create_opcua_discovery_client();
    let tcp_stream = TcpStreamImpl {};
    get_discovery_urls(
        &mut discovery_handler_client,
        discovery_urls,
        filter_list,
        tcp_stream,
    )
}

/// This calls FindServers on each DiscoveryURL provided in order to
/// (1) verify the DiscoveryURL
/// (2) discover other servers registered with a Local Discovery Server in the case that the DiscoveryURL is for an LDS
/// (3) determine whether the application at that URL should be included according to `ApplicationType` and the `application_names` filter
fn get_discovery_urls(
    discovery_handler_client: &mut impl OpcuaClient,
    lds_urls: Vec<String>,
    filter_list: Option<FilterList>,
    tcp_stream: impl TcpStream,
) -> Vec<String> {
    let mut discovery_urls: Vec<String> = Vec::new();
    lds_urls.iter().for_each(|url| {
        if let Err(e) = test_tcp_connection(url, &tcp_stream) {
            error!(
                "get_discovery_urls - failed to make tcp connection with url {} with error {:?}",
                url, e
            );
        } else {
            match discovery_handler_client.find_servers(url) {
                Ok(applications) => {
                    trace!(
                        "get_discovery_urls - Server at {} responded with {} Applications",
                        url,
                        applications.len()
                    );
                    let mut servers_discovery_urls: Vec<String> = applications
                        .iter()
                        .filter_map(|application| {
                            get_discovery_url_from_application_description(
                                application,
                                filter_list.as_ref(),
                                url,
                            )
                        })
                        .collect::<Vec<String>>();
                    discovery_urls.append(&mut servers_discovery_urls);
                }
                Err(err) => {
                    trace!(
                        "get_discovery_urls - cannot find servers on discovery server. Error {:?}",
                        err
                    );
                }
            };
        }
    });
    // Remove duplicates in the case that a server was registered with more than one LDS
    discovery_urls.dedup();
    discovery_urls
}

/// The Rust OPC UA implementation of FindServers does not use a timeout when connecting with a Server over TCP
/// So, an unsuccessful attempt can take over 2 minutes.
/// Therefore, this tests the connection using a timeout before calling FindServers on the DiscoveryURL.
fn test_tcp_connection(url: &str, tcp_stream: &impl TcpStream) -> Result<(), anyhow::Error> {
    let socket_addr = get_socket_addr(url)?;
    match tcp_stream.connect_timeout(
        &socket_addr,
        Duration::from_secs(TCP_CONNECTION_TEST_TIMEOUT_SECS),
    ) {
        Ok(_stream) => Ok(()),
        Err(e) => Err(anyhow::format_err!("{:?}", e)),
    }
}

/// This selects a DiscoveryURL from an application's `ApplicationDescription` so long as the Application passes the following criteria
/// (1) it is `ApplicationType::Server` (not a DiscoveryServer, Client, ClientServer)
/// (2) it passes the FilterList criteria for `application_name`
/// Note: OPC UA Applications can have more than one DiscoveryURL, often to support different transport protocols.
/// This function preferences tcp discovery URLs, as tcp endpoints support both application and communication layer security.
fn get_discovery_url_from_application_description(
    server: &ApplicationDescription,
    filter_list: Option<&FilterList>,
    ip_url: &str,
) -> Option<String> {
    trace!(
        "get_discovery_url_from_application - found server : {}",
        server.application_name
    );
    // Only discover ApplicationType::Server
    if server.application_type != ApplicationType::Server {
        trace!(
            "get_discovery_url_from_application - Application is a {:?} not a Server. Ignoring it.",
            server.application_type
        );
        None
    } else if !should_include(filter_list, server.application_name.text.as_ref()) {
        trace!(
            "get_discovery_url_from_application - Application {} has been filtered out by application name",
            server.application_name.text.to_string()
        );
        None
    } else if let Some(ref server_discovery_urls) = server.discovery_urls {
        // TODO: could two different DiscoveryUrls be registered as localhost:<port> on different lds's?
        trace!(
            "get_discovery_url_from_application - server has {:?} DiscoveryUrls",
            server_discovery_urls
        );
        // Pass the tcp DiscoveryURL by default, since it supports application authentication and
        // is more frequently utilized in OPC UA else pass first one
        let discovery_url = match server_discovery_urls
            .iter()
            .find(|discovery_url| discovery_url.as_ref().starts_with(OPC_TCP_SCHEME))
        {
            Some(tcp_discovery_url) => tcp_discovery_url.to_string(),
            None => server_discovery_urls[0].to_string(),
        };
        // If discovery_url is DNS, check if it is resolvable, if not convert it to ip address
        match get_discovery_url_ip(ip_url, discovery_url) {
            Ok(discovery_url) => Some(discovery_url),
            Err(e) => {
                trace!(
                    "get_discovery_url_from_application - failed to resolve discovery url with error {:?}",
                    e
                );
                None
            }
        }
    } else {
        trace!(
            "get_discovery_urls - Server {} doesn't have any DiscoveryUrls",
            server.application_name
        );
        None
    }
}

/// This returns a socket address for the OPC UA DiscoveryURL else an error if not properly formatted
fn get_socket_addr(url: &str) -> Result<SocketAddr, anyhow::Error> {
    let url = Url::parse(url).map_err(|_| anyhow::format_err!("could not parse url"))?;
    if url.scheme() != OPC_TCP_SCHEME {
        return Err(anyhow::format_err!(
            "format of OPC UA url {} is not valid",
            url
        ));
    }
    let host = url.host_str().unwrap();
    let port = url
        .port()
        .ok_or_else(|| anyhow::format_err!("provided discoveryURL is missing port"))?;

    // Convert host and port to socket address
    let addr_str = format!("{}:{}", host, port);
    let addrs = addr_str.to_socket_addrs();
    let addr = addrs.unwrap().next().unwrap();
    Ok(addr)
}

// This checks if the discovery_url can be resolved, if not use ip address instead
fn get_discovery_url_ip(
    ip_url_str: &str,
    discovery_url_str: String,
) -> Result<String, anyhow::Error> {
    let ip_url = Url::parse(ip_url_str).with_context(|| "could not parse url {ip_url_str}")?;
    let discovery_url = Url::parse(&discovery_url_str)
        .with_context(|| "could not parse url {discovery_url_str}")?;
    if discovery_url.scheme() != OPC_TCP_SCHEME {
        return Err(anyhow::format_err!(
            "format of OPC UA url {} is not valid",
            discovery_url
        ));
    }
    let mut path = discovery_url.path().to_string();
    let host = discovery_url.host_str().unwrap();
    let port = discovery_url.port().unwrap_or(DEFAULT_OPC_UA_SERVER_PORT);

    let addr_str = format!("{}:{}", host, port);

    // check if the hostname can be resolved to socket address
    match addr_str.to_socket_addrs() {
        Ok(_url) => Ok(discovery_url_str),
        Err(_) => {
            if ip_url_str.ends_with('/') && path.starts_with('/') {
                path.remove(0);
            }
            let url = if ip_url.path() == "" || ip_url.path() == "/" {
                format!("{}{}", ip_url, path)
            } else {
                ip_url_str.to_string()
            };
            trace!(
                "get_discovery_url_ip - cannot resolve the application url from server, using ip address instead of hostname: {}",
                url
            );
            Ok(url)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::wrappers::{
        opcua_client_wrapper::MockOpcuaClient, tcp_stream_wrapper::MockTcpStream,
    };
    use super::*;
    use mockall::Sequence;

    pub fn create_application_description(
        application_uri: &str,
        application_name: &str,
        application_type: ApplicationType,
        discovery_url: &str,
    ) -> ApplicationDescription {
        ApplicationDescription {
            application_uri: UAString::from(application_uri),
            product_uri: UAString::from(""),
            application_name: LocalizedText::new("", application_name),
            application_type,
            gateway_server_uri: UAString::from(""),
            discovery_profile_uri: UAString::from(""),
            discovery_urls: Some(vec![UAString::from(discovery_url)]),
        }
    }

    fn set_up_mock_tcp_stream(
        discovery_url: &'static str,
        discovery_url2: &'static str,
    ) -> MockTcpStream {
        let mut mock_tcp_stream = MockTcpStream::new();
        let mut tcp_stream_seq = Sequence::new();
        let tcp_timeout_duration = Duration::from_secs(TCP_CONNECTION_TEST_TIMEOUT_SECS);
        let discovery_url_socket_addr = get_socket_addr(discovery_url).unwrap();
        mock_tcp_stream
            .expect_connect_timeout()
            .times(1)
            .withf(move |addr: &SocketAddr, timeout: &Duration| {
                addr == &discovery_url_socket_addr && timeout == &tcp_timeout_duration
            })
            .return_once(move |_, _| Ok(()))
            .in_sequence(&mut tcp_stream_seq);

        let discovery_url_socket_addr2 = get_socket_addr(discovery_url2).unwrap();
        mock_tcp_stream
            .expect_connect_timeout()
            .times(1)
            .withf(move |addr: &SocketAddr, timeout: &Duration| {
                addr == &discovery_url_socket_addr2 && timeout == &tcp_timeout_duration
            })
            .return_once(move |_, _| Ok(()))
            .in_sequence(&mut tcp_stream_seq);

        mock_tcp_stream
    }

    #[test]
    fn test_get_discovery_urls() {
        let lds_url = "opc.tcp://127.0.0.1:4840/";
        let lds_url2 = "opc.tcp://10.0.0.1:4840/";
        let discovery_url = "opc.tcp://127.0.0.1:4855/";
        let discovery_url2 = "opc.tcp://127.0.0.1:4866/";
        let mut mock_client = MockOpcuaClient::new();
        let mut find_servers_seq = Sequence::new();

        let mock_tcp_stream = set_up_mock_tcp_stream(lds_url, lds_url2);

        let server_application_description = create_application_description(
            "urn:Mock OPC UA Server",
            "Mock OPC UA Server",
            ApplicationType::Server,
            discovery_url,
        );
        let server_application_description2 = create_application_description(
            "urn:Mock OPC UA Server2",
            "Mock OPC UA Server2",
            ApplicationType::Server,
            discovery_url2,
        );

        mock_client
            .expect_find_servers()
            .times(1)
            .withf(move |url: &str| url == lds_url)
            .return_once(move |_| Ok(vec![server_application_description]))
            .in_sequence(&mut find_servers_seq);

        mock_client
            .expect_find_servers()
            .times(1)
            .withf(move |url: &str| url == lds_url2)
            .return_once(move |_| Ok(vec![server_application_description2]))
            .in_sequence(&mut find_servers_seq);

        let discovery_urls = get_discovery_urls(
            &mut mock_client,
            vec![lds_url.to_string(), lds_url2.to_string()],
            None,
            mock_tcp_stream,
        );
        assert_eq!(discovery_urls.len(), 2);
        assert_eq!(&discovery_urls[0], discovery_url);
    }

    #[test]
    fn test_get_discovery_urls_server_not_found() {
        let discovery_url = "opc.tcp://127.0.0.1:4855/";
        let discovery_url2 = "opc.tcp://127.0.0.1:4866/";
        let mut mock_client = MockOpcuaClient::new();
        let mut find_servers_seq = Sequence::new();
        let mock_tcp_stream = set_up_mock_tcp_stream(discovery_url, discovery_url2);

        let server_application_description2 = create_application_description(
            "urn:Mock OPC UA Server2",
            "Mock OPC UA Server2",
            ApplicationType::Server,
            discovery_url2,
        );

        mock_client
            .expect_find_servers()
            .times(1)
            .withf(move |url: &str| url == discovery_url)
            .return_once(move |_| Err(StatusCode::BadResourceUnavailable))
            .in_sequence(&mut find_servers_seq);

        mock_client
            .expect_find_servers()
            .times(1)
            .withf(move |url: &str| url == discovery_url2)
            .return_once(move |_| Ok(vec![server_application_description2]))
            .in_sequence(&mut find_servers_seq);

        let discovery_urls = get_discovery_urls(
            &mut mock_client,
            vec![discovery_url.to_string(), discovery_url2.to_string()],
            None,
            mock_tcp_stream,
        );
        assert_eq!(discovery_urls.len(), 1);
        assert_eq!(&discovery_urls[0], discovery_url2);
    }

    #[test]
    fn test_get_discovery_urls_removes_duplicates() {
        let lds_url = "opc.tcp://127.0.0.1:4840/";
        let lds_url2 = "opc.tcp://10.0.0.1:4840/";
        let discovery_url = "opc.tcp://10.123.45.6:4855/";
        let mut mock_client = MockOpcuaClient::new();
        let mut find_servers_seq = Sequence::new();
        let mock_tcp_stream = set_up_mock_tcp_stream(lds_url, lds_url2);

        let server_application_description = create_application_description(
            "urn:Mock OPC UA Server",
            "Mock OPC UA Server",
            ApplicationType::Server,
            discovery_url,
        );
        let server_application_description2 = create_application_description(
            "urn:Mock OPC UA Server",
            "Mock OPC UA Server",
            ApplicationType::Server,
            discovery_url,
        );

        mock_client
            .expect_find_servers()
            .times(1)
            .withf(move |url: &str| url == lds_url)
            .return_once(move |_| Ok(vec![server_application_description]))
            .in_sequence(&mut find_servers_seq);

        mock_client
            .expect_find_servers()
            .times(1)
            .withf(move |url: &str| url == lds_url2)
            .return_once(move |_| Ok(vec![server_application_description2]))
            .in_sequence(&mut find_servers_seq);

        let discovery_urls = get_discovery_urls(
            &mut mock_client,
            vec![lds_url.to_string(), lds_url2.to_string()],
            None,
            mock_tcp_stream,
        );
        assert_eq!(discovery_urls.len(), 1);
    }

    #[test]
    // Test that find servers isn't called on invalid DiscoveryURL (missing opc)
    fn test_get_server_endpoints_invalid_url() {
        let mut mock_client = MockOpcuaClient::new();
        let mock_tcp_stream = MockTcpStream::new();
        assert!(get_discovery_urls(
            &mut mock_client,
            vec!["tcp://127.0.0.1:4855/".to_string()],
            None,
            mock_tcp_stream
        )
        .is_empty())
    }

    #[test]
    // Test that it filters out DiscoveryServers
    fn test_get_server_endpoints_filter_out_lds() {
        let discovery_url = "opc.tcp://127.0.0.1:4840/";
        let mut mock_client = MockOpcuaClient::new();
        let mut mock_tcp_stream = MockTcpStream::new();
        let tcp_timeout_duration = Duration::from_secs(TCP_CONNECTION_TEST_TIMEOUT_SECS);

        let discovery_server_application_description = create_application_description(
            "urn:Mock OPC UA Server",
            "Mock OPC UA Server",
            ApplicationType::DiscoveryServer,
            discovery_url,
        );
        mock_client
            .expect_find_servers()
            .times(1)
            .withf(move |url: &str| url == discovery_url)
            .return_once(move |_| Ok(vec![discovery_server_application_description]));
        let discovery_url_socket_addr = get_socket_addr(discovery_url).unwrap();
        mock_tcp_stream
            .expect_connect_timeout()
            .times(1)
            .withf(move |addr: &SocketAddr, timeout: &Duration| {
                addr == &discovery_url_socket_addr && timeout == &tcp_timeout_duration
            })
            .return_once(move |_, _| Ok(()));

        let discovery_urls = get_discovery_urls(
            &mut mock_client,
            vec![discovery_url.to_string()],
            None,
            mock_tcp_stream,
        );
        assert!(discovery_urls.is_empty());
    }

    #[test]
    // Test that it converts the discovery url to an ip address if the discovery url is a hostname that is not resolvable
    fn test_get_discovery_url_ip() {
        let ip_url = "opc.tcp://192.168.0.1:50000";
        let ip_url2 = "opc.tcp://192.168.0.1:50000/OPCUA/Simluation";

        //  OPCTest.invalid is not a valid hostname, it should be overwritten by the ip_url
        let discovery_url = "opc.tcp://OPCTest.invalid:50000/OPCUA/Simluation";
        assert_eq!(
            get_discovery_url_ip(ip_url, discovery_url.to_string()).unwrap(),
            "opc.tcp://192.168.0.1:50000/OPCUA/Simluation"
        );
        assert_eq!(
            get_discovery_url_ip(ip_url2, discovery_url.to_string()).unwrap(),
            "opc.tcp://192.168.0.1:50000/OPCUA/Simluation"
        );

        // 192.168.0.2 is a valid ip address, it should not be overwritten
        let discovery_url = "opc.tcp://192.168.0.2:50000/OPCUA/Simluation";
        assert_eq!(
            get_discovery_url_ip(ip_url, discovery_url.to_string()).unwrap(),
            "opc.tcp://192.168.0.2:50000/OPCUA/Simluation"
        );
    }
}
