pub mod to_serialize {
    use super::common::*;
    #[derive(Default, PartialEq, Debug, YaSerialize)]
    #[yaserde(prefix = "s", namespace = "s: http://www.w3.org/2003/05/soap-envelope")]
    pub struct Envelope {
        #[yaserde(prefix = "s", rename = "Header")]
        pub header: Header,

        #[yaserde(prefix = "s", rename = "Body")]
        pub body: Body,
    }

    #[derive(Default, PartialEq, Debug, YaSerialize)]
    #[yaserde(
        prefix = "s",
        namespace = "s: http://www.w3.org/2003/05/soap-envelope",
        namespace = "d: http://schemas.xmlsoap.org/ws/2005/04/discovery"
    )]
    pub struct Body {
        #[yaserde(prefix = "d", rename = "Probe")]
        pub probe: Probe,
    }

    #[derive(Default, PartialEq, Debug, YaSerialize)]
    #[yaserde(
        prefix = "s",
        namespace = "s: http://www.w3.org/2003/05/soap-envelope",
        namespace = "w: http://schemas.xmlsoap.org/ws/2004/08/addressing"
    )]
    pub struct Header {
        #[yaserde(prefix = "w", rename = "MessageID")]
        pub message_id: String,

        #[yaserde(prefix = "w", rename = "To")]
        pub reply_to: String,

        #[yaserde(prefix = "w", rename = "Action")]
        pub action: String,
    }
}

pub mod to_deserialize {
    use super::common::*;

    #[derive(Default, PartialEq, Debug, YaDeserialize)]
    #[yaserde(prefix = "s", namespace = "s: http://www.w3.org/2003/05/soap-envelope")]
    pub struct Envelope {
        #[yaserde(prefix = "s", rename = "Header")]
        pub header: Header,

        #[yaserde(prefix = "s", rename = "Body")]
        pub body: Body,
    }

    #[derive(Default, PartialEq, Debug, YaDeserialize)]
    #[yaserde(
        prefix = "s",
        namespace = "s: http://www.w3.org/2003/05/soap-envelope",
        namespace = "d: http://schemas.xmlsoap.org/ws/2005/04/discovery"
    )]
    pub struct Body {
        #[yaserde(prefix = "d", rename = "ProbeMatches")]
        pub probe_matches: ProbeMatches,
    }

    #[derive(Default, PartialEq, Debug, YaDeserialize)]
    #[yaserde(
        prefix = "s",
        namespace = "s: http://www.w3.org/2003/05/soap-envelope",
        namespace = "w: http://schemas.xmlsoap.org/ws/2004/08/addressing"
    )]
    pub struct Header {
        #[yaserde(prefix = "w", rename = "RelatesTo")]
        pub relates_to: String,
    }
}

#[allow(dead_code)]
pub mod probe_types {
    pub const DEVICE_NAMESPACE_PREFIX: &str = "devwsdl";
    pub const NETWORK_VIDEO_TRANSMITTER_NAMESPACE_PREFIX: &str = "netwsdl";
    pub const DEVICE_NAMESPACE_DESCRIPTOR: &str = "devwsdl: http://www.onvif.org/ver10/device/wsdl";
    pub const NETWORK_VIDEO_TRANSMITTER_NAMESPACE_DESCRIPTOR: &str =
        "netwsdl: http://www.onvif.org/ver10/network/wsdl";
    pub const DEVICE: &str = "devwsdl:Device";
    pub const NETWORK_VIDEO_TRANSMITTER: &str = "netwsdl:NetworkVideoTransmitter";
}

pub mod common {
    #[derive(Default, PartialEq, Debug, YaDeserialize, YaSerialize)]
    #[yaserde(
        prefix = "d",
        namespace = "d: http://schemas.xmlsoap.org/ws/2005/04/discovery",
        namespace = probe_typews::NETWORK_VIDEO_TRANSMITTER_NAMESPACE_DESCRIPTOR,
        namespace = probe_typews::DEVICE_NAMESPACE_DESCRIPTOR
    )]
    pub struct Probe {
        #[yaserde(prefix = "d", rename = "Types")]
        pub probe_types: Vec<String>,
    }

    #[derive(Default, PartialEq, Debug, YaDeserialize, YaSerialize)]
    #[yaserde(
        prefix = "d",
        namespace = "d: http://schemas.xmlsoap.org/ws/2005/04/discovery",
        namespace = "wsa: http://schemas.xmlsoap.org/ws/2004/08/addressing"
    )]
    pub struct ProbeMatch {
        #[yaserde(prefix = "d", rename = "XAddrs")]
        pub xaddrs: String,
        #[yaserde(prefix = "wsa", rename = "EndpointReference")]
        pub endpoint_reference: String,
        #[yaserde(prefix = "d", rename = "Types")]
        pub probe_types: Vec<String>,
        #[yaserde(prefix = "d", rename = "Scopes")]
        pub scopes: Vec<String>,
        #[yaserde(prefix = "d", rename = "MetadataVersion")]
        pub metadata_version: String,
    }

    #[derive(Default, PartialEq, Debug, YaDeserialize, YaSerialize)]
    #[yaserde(
        prefix = "d",
        namespace = "d: http://schemas.xmlsoap.org/ws/2005/04/discovery"
    )]
    pub struct ProbeMatches {
        #[yaserde(prefix = "d", rename = "ProbeMatch")]
        pub probe_match: Vec<ProbeMatch>,
    }
}

pub mod util {
    use super::super::discovery_utils::{OnvifQuery, OnvifQueryImpl};
    use super::{common, probe_types, to_deserialize, to_serialize};
    use akri_discovery_utils::filtering::{FilterList, FilterType};
    use log::{error, info, trace};
    use std::collections::HashSet;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use tokio::{
        io::ErrorKind,
        net::UdpSocket,
        time::{self, Duration, Instant},
    };

    fn create_onvif_discovery_message(uuid_string: &str) -> String {
        let probe_types: Vec<String> = vec![probe_types::NETWORK_VIDEO_TRANSMITTER.into()];
        let envelope = to_serialize::Envelope {
            header: to_serialize::Header {
                message_id: uuid_string.into(),
                action: "http://schemas.xmlsoap.org/ws/2005/04/discovery/Probe".into(),
                reply_to: "urn:schemas-xmlsoap-org:ws:2005:04:discovery".into(),
            },
            body: to_serialize::Body {
                probe: common::Probe { probe_types },
            },
        };
        let envelope_as_string = yaserde::ser::to_string(&envelope).unwrap();
        trace!(
            "create_onvif_discovery_message - discovery message: {:?}",
            &envelope_as_string
        );
        envelope_as_string
    }

    #[cfg(test)]
    mod serialize_tests {
        use super::*;

        #[test]
        fn test_create_onvif_discovery_message() {
            let _ = env_logger::builder().is_test(true).try_init();

            let uuid_str = format!("uuid:{}", uuid::Uuid::new_v4());
            let expected_msg = format!(
                "<?xml version=\"1.0\" encoding=\"utf-8\"?><s:Envelope xmlns:s=\"http://www.w3.org/2003/05/soap-envelope\"><s:Header xmlns:w=\"http://schemas.xmlsoap.org/ws/2004/08/addressing\"><w:MessageID>{}</w:MessageID><w:To>urn:schemas-xmlsoap-org:ws:2005:04:discovery</w:To><w:Action>http://schemas.xmlsoap.org/ws/2005/04/discovery/Probe</w:Action></s:Header><s:Body xmlns:d=\"http://schemas.xmlsoap.org/ws/2005/04/discovery\"><d:Probe><d:Types>netwsdl:NetworkVideoTransmitter</d:Types></d:Probe></s:Body></s:Envelope>",
                &uuid_str
            );
            assert_eq!(expected_msg, create_onvif_discovery_message(&uuid_str));
        }
    }

    fn get_scope_filtered_uris_from_discovery_response(
        discovery_response: &str,
        scopes: Option<&FilterList>,
    ) -> Vec<String> {
        let response_envelope =
            yaserde::de::from_str::<to_deserialize::Envelope>(discovery_response);
        // The response envelope follows this format:
        //   <Envelope><Body><ProbeMatches><ProbeMatch><XAddrs>
        //       https://10.0.0.1:5357/svc
        //       https://10.0.0.2:5357/svc
        //       https://10.0.0.3:5357/svc
        //   </ProbeMatch></ProbeMatches></XAddrs></Body></Envelope>
        response_envelope
            .unwrap()
            .body
            .probe_matches
            .probe_match
            .iter()
            .filter(|probe_match| !execute_filter(scopes, &probe_match.scopes))
            .flat_map(|probe_match| probe_match.xaddrs.split_whitespace())
            .map(|addr| addr.to_string())
            .collect::<Vec<String>>()
    }

    async fn get_responsive_uris(
        uris: HashSet<String>,
        onvif_query: &impl OnvifQuery,
    ) -> Vec<String> {
        let futures: Vec<_> = uris
            .iter()
            .map(|uri| onvif_query.is_device_responding(uri))
            .collect();
        let results = futures_util::future::join_all(futures).await;
        results
            .into_iter()
            .filter_map(|r| match r {
                Ok(uri) => Some(uri),
                Err(e) => {
                    trace!(
                        "device not responding to date/time request with error {}",
                        e
                    );
                    None
                }
            })
            .collect()
    }

    pub(crate) fn execute_filter(
        filter_list: Option<&FilterList>,
        filter_against: &[String],
    ) -> bool {
        if filter_list.is_none() {
            return false;
        }
        let filter_action = filter_list.as_ref().unwrap().action.clone();
        let filter_count = filter_list
            .unwrap()
            .items
            .iter()
            .filter(|pattern| {
                filter_against
                    .iter()
                    .filter(|filter_against_item| filter_against_item.contains(*pattern))
                    .count()
                    > 0
            })
            .count();

        if FilterType::Include == filter_action {
            filter_count == 0
        } else {
            filter_count != 0
        }
    }

    #[cfg(test)]
    mod deserialize_tests {
        use super::*;

        #[test]
        fn test_get_scope_filtered_uris_from_discovery_response() {
            let _ = env_logger::builder().is_test(true).try_init();

            let uris = vec!["uri_one".to_string(), "uri_two".to_string()];
            let response = format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<SOAP-ENV:Envelope xmlns:SOAP-ENV=\"http://www.w3.org/2003/05/soap-envelope\" xmlns:SOAP-ENC=\"http://www.w3.org/2003/05/soap-encoding\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xmlns:xsd=\"http://www.w3.org/2001/XMLSchema\" xmlns:xs=\"http://www.w3.org/2000/10/XMLSchema\" xmlns:wsse=\"http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-secext-1.0.xsd\" xmlns:wsa5=\"http://www.w3.org/2005/08/addressing\" xmlns:xop=\"http://www.w3.org/2004/08/xop/include\" xmlns:wsa=\"http://schemas.xmlsoap.org/ws/2004/08/addressing\" xmlns:tt=\"http://www.onvif.org/ver10/schema\" xmlns:ns1=\"http://www.w3.org/2005/05/xmlmime\" xmlns:wstop=\"http://docs.oasis-open.org/wsn/t-1\" xmlns:ns7=\"http://docs.oasis-open.org/wsrf/r-2\" xmlns:ns2=\"http://docs.oasis-open.org/wsrf/bf-2\" xmlns:dndl=\"http://www.onvif.org/ver10/network/wsdl/DiscoveryLookupBinding\" xmlns:dnrd=\"http://www.onvif.org/ver10/network/wsdl/RemoteDiscoveryBinding\" xmlns:d=\"http://schemas.xmlsoap.org/ws/2005/04/discovery\" xmlns:dn=\"http://www.onvif.org/ver10/network/wsdl\" xmlns:ns10=\"http://www.onvif.org/ver10/replay/wsdl\" xmlns:ns11=\"http://www.onvif.org/ver10/search/wsdl\" xmlns:ns13=\"http://www.onvif.org/ver20/analytics/wsdl/RuleEngineBinding\" xmlns:ns14=\"http://www.onvif.org/ver20/analytics/wsdl/AnalyticsEngineBinding\" xmlns:tan=\"http://www.onvif.org/ver20/analytics/wsdl\" xmlns:ns15=\"http://www.onvif.org/ver10/events/wsdl/PullPointSubscriptionBinding\" xmlns:ns16=\"http://www.onvif.org/ver10/events/wsdl/EventBinding\" xmlns:tev=\"http://www.onvif.org/ver10/events/wsdl\" xmlns:ns17=\"http://www.onvif.org/ver10/events/wsdl/SubscriptionManagerBinding\" xmlns:ns18=\"http://www.onvif.org/ver10/events/wsdl/NotificationProducerBinding\" xmlns:ns19=\"http://www.onvif.org/ver10/events/wsdl/NotificationConsumerBinding\" xmlns:ns20=\"http://www.onvif.org/ver10/events/wsdl/PullPointBinding\" xmlns:ns21=\"http://www.onvif.org/ver10/events/wsdl/CreatePullPointBinding\" xmlns:ns22=\"http://www.onvif.org/ver10/events/wsdl/PausableSubscriptionManagerBinding\" xmlns:wsnt=\"http://docs.oasis-open.org/wsn/b-2\" xmlns:ns3=\"http://www.onvif.org/ver10/analyticsdevice/wsdl\" xmlns:ns4=\"http://www.onvif.org/ver10/deviceIO/wsdl\" xmlns:ns5=\"http://www.onvif.org/ver10/display/wsdl\" xmlns:ns8=\"http://www.onvif.org/ver10/receiver/wsdl\" xmlns:ns9=\"http://www.onvif.org/ver10/recording/wsdl\" xmlns:tds=\"http://www.onvif.org/ver10/device/wsdl\" xmlns:timg=\"http://www.onvif.org/ver20/imaging/wsdl\" xmlns:tptz=\"http://www.onvif.org/ver20/ptz/wsdl\" xmlns:trt=\"http://www.onvif.org/ver10/media/wsdl\" xmlns:trt2=\"http://www.onvif.org/ver20/media/wsdl\" xmlns:ter=\"http://www.onvif.org/ver10/error\" xmlns:tns1=\"http://www.onvif.org/ver10/topics\" xmlns:tnsn=\"http://www.eventextension.com/2011/event/topics\"><SOAP-ENV:Header><wsa:MessageID>urn:uuid:2bc6f06c-5566-7788-99ac-0012414fb745</wsa:MessageID><wsa:RelatesTo>uuid:7b1d26aa-b02e-4ad2-8aab-4c928298ee0c</wsa:RelatesTo><wsa:To SOAP-ENV:mustUnderstand=\"true\">http://schemas.xmlsoap.org/ws/2004/08/addressing/role/anonymous</wsa:To><wsa:Action SOAP-ENV:mustUnderstand=\"true\">http://schemas.xmlsoap.org/ws/2005/04/discovery/ProbeMatches</wsa:Action></SOAP-ENV:Header><SOAP-ENV:Body><d:ProbeMatches><d:ProbeMatch><wsa:EndpointReference><wsa:Address>urn:uuid:10919da4-5566-7788-99aa-0012414fb745</wsa:Address></wsa:EndpointReference><d:Types>dn:NetworkVideoTransmitter</d:Types><d:Scopes>onvif://www.onvif.org/type/video_encoder onvif://www.onvif.org/type/audio_encoder onvif://www.onvif.org/hardware/IPC-model onvif://www.onvif.org/location/country/china onvif://www.onvif.org/name/NVT onvif://www.onvif.org/Profile/Streaming </d:Scopes><d:XAddrs>{}</d:XAddrs><d:MetadataVersion>10</d:MetadataVersion></d:ProbeMatch></d:ProbeMatches></SOAP-ENV:Body></SOAP-ENV:Envelope>",
                &uris.join(" ")
            );
            assert_eq!(
                uris,
                get_scope_filtered_uris_from_discovery_response(&response, None)
            );
        }
    }

    pub async fn get_discovery_response_socket() -> Result<UdpSocket, anyhow::Error> {
        let uuid_str = format!("uuid:{}", uuid::Uuid::new_v4());
        trace!("get_discovery_response_socket - for {}", &uuid_str);
        const LOCAL_IPV4_ADDR: Ipv4Addr = Ipv4Addr::UNSPECIFIED;
        const LOCAL_PORT: u16 = 0;
        let local_socket_addr = SocketAddr::new(IpAddr::V4(LOCAL_IPV4_ADDR), LOCAL_PORT);

        // WS-Discovery multicast ip and port selected from available standard
        // options.  See https://en.wikipedia.org/wiki/WS-Discovery
        const MULTI_IPV4_ADDR: Ipv4Addr = Ipv4Addr::new(239, 255, 255, 250);
        const MULTI_PORT: u16 = 3702;
        let multi_socket_addr = SocketAddr::new(IpAddr::V4(MULTI_IPV4_ADDR), MULTI_PORT);

        trace!(
            "get_discovery_response_socket - binding to: {:?}",
            local_socket_addr
        );
        let socket = UdpSocket::bind(local_socket_addr).await?;
        trace!(
            "get_discovery_response_socket - joining multicast: {:?} {:?}",
            &MULTI_IPV4_ADDR,
            &LOCAL_IPV4_ADDR
        );
        socket
            .join_multicast_v4(MULTI_IPV4_ADDR, LOCAL_IPV4_ADDR)
            .unwrap();

        let envelope_as_string = create_onvif_discovery_message(&uuid_str);
        socket
            .send_to(envelope_as_string.as_bytes(), multi_socket_addr)
            .await?;
        Ok(socket)
    }

    pub async fn simple_onvif_discover(
        socket: &mut UdpSocket,
        scopes_filters: Option<&FilterList>,
        timeout: Duration,
    ) -> Result<Vec<String>, anyhow::Error> {
        let mut broadcast_responses = Vec::new();

        let start = Instant::now();
        loop {
            let elapsed = start.elapsed();
            if elapsed >= timeout {
                break;
            }

            let time_left = timeout - elapsed;

            match try_recv_string(socket, time_left).await {
                Ok(s) => {
                    broadcast_responses.push(s);
                }
                Err(e) => match e.kind() {
                    ErrorKind::WouldBlock | ErrorKind::TimedOut => {
                        trace!("simple_onvif_discover - recv_from error ... continue collecting responses {:?}", e);
                    }
                    _ => {
                        error!("simple_onvif_discover - recv_from error: {:?}", e);
                        return Err(anyhow::anyhow!(e));
                    }
                },
            }
        }

        trace!(
            "simple_onvif_discover - uris discovered by udp broadcast {:?}",
            broadcast_responses
        );
        let mut filtered_uris = std::collections::HashSet::new();
        broadcast_responses.into_iter().for_each(|r| {
            filtered_uris.extend(get_scope_filtered_uris_from_discovery_response(
                &r,
                scopes_filters,
            ))
        });
        trace!(
            "simple_onvif_discover - uris after filtering by scopes {:?}",
            filtered_uris
        );
        let devices = get_responsive_uris(filtered_uris, &OnvifQueryImpl {}).await;
        info!("simple_onvif_discover - devices: {:?}", devices);
        Ok(devices)
    }

    async fn try_recv_string(s: &mut UdpSocket, timeout: Duration) -> std::io::Result<String> {
        let mut buf = vec![0; 16 * 1024];
        let len = time::timeout(timeout, s.recv(&mut buf)).await??;
        Ok(String::from_utf8_lossy(&buf[..len]).to_string())
    }

    #[cfg(test)]
    mod discovery_tests {
        use super::*;
        use std::{
            sync::{Arc, Mutex},
            time::{Duration, SystemTime},
        };

        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn test_timeout_for_simple_onvif_discover() {
            let _ = env_logger::builder().is_test(true).try_init();

            let timeout = Duration::from_secs(2);
            let duration = Arc::new(Mutex::new(Duration::from_secs(5)));

            let thread_duration = duration.clone();
            tokio::spawn(async move {
                let start = SystemTime::now();
                let _ignore = simple_onvif_discover(
                    &mut get_discovery_response_socket().await.unwrap(),
                    None,
                    timeout,
                )
                .await
                .unwrap();
                let end = SystemTime::now();
                let mut inner_duration = thread_duration.lock().unwrap();
                *inner_duration = end.duration_since(start).unwrap();
                trace!(
                    "call to simple_onvif_discover took {} milliseconds",
                    inner_duration.as_millis()
                );
            });

            let wait_for_call_millis = timeout.as_secs() * 1000 + 200;
            trace!("wait for {} milliseconds", wait_for_call_millis);
            std::thread::sleep(Duration::from_millis(wait_for_call_millis));
            // validate that this ends in 2 seconds or less
            trace!("duration to test: {}", duration.lock().unwrap().as_millis());
            // we could test for exactly 2 seconds here, but a little wiggle room seems reasonable
            assert!(duration.lock().unwrap().as_millis() <= wait_for_call_millis.into());
        }
    }
}
