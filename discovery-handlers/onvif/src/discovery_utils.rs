use super::credential_store::CredentialStore;
use super::username_token::UsernameToken;
use async_trait::async_trait;
use futures_util::stream::TryStreamExt;
use hyper::Request;
use log::trace;
#[cfg(test)]
use mockall::{automock, predicate::*};
use std::io::{Error, ErrorKind};
use sxd_document::{parser, Package};
use sxd_xpath::Value;

pub const ONVIF_DEVICE_SERVICE_URL_LABEL_ID: &str = "ONVIF_DEVICE_SERVICE_URL";
pub const ONVIF_DEVICE_IP_ADDRESS_LABEL_ID: &str = "ONVIF_DEVICE_IP_ADDRESS";
pub const ONVIF_DEVICE_MAC_ADDRESS_LABEL_ID: &str = "ONVIF_DEVICE_MAC_ADDRESS";
pub const ONVIF_DEVICE_UUID_LABEL_ID: &str = "ONVIF_DEVICE_UUID";
pub const MEDIA_WSDL: &str = "http://www.onvif.org/ver10/media/wsdl";
pub const DEVICE_WSDL: &str = "http://www.onvif.org/ver10/device/wsdl";

/// OnvifQuery can access ONVIF properties given an ONVIF camera's device service url.
///
/// An implementation of an onvif query can retrieve the camera's ip/mac address, profiles and streaming uri.
#[cfg_attr(test, automock)]
#[async_trait]
pub trait OnvifQuery {
    async fn get_device_ip_and_mac_address(
        &self,
        service_url: &str,
        device_uuid: &str,
    ) -> Result<(String, String), anyhow::Error>;
    async fn get_device_service_uri(
        &self,
        url: &str,
        service: &str,
    ) -> Result<String, anyhow::Error>;
    async fn get_device_profiles(&self, url: &str) -> Result<Vec<String>, anyhow::Error>;
    async fn get_device_profile_streaming_uri(
        &self,
        url: &str,
        profile_token: &str,
    ) -> Result<String, anyhow::Error>;
    async fn is_device_responding(&self, url: &str) -> Result<String, anyhow::Error>;
}

#[derive(Default)]
pub struct OnvifQueryImpl {
    credential_store: CredentialStore,
}

impl OnvifQueryImpl {
    pub fn new(credential_store: CredentialStore) -> Self {
        Self { credential_store }
    }
}

#[async_trait]
impl OnvifQuery for OnvifQueryImpl {
    /// Gets the ip and mac address of a given ONVIF camera
    async fn get_device_ip_and_mac_address(
        &self,
        service_url: &str,
        device_uuid: &str,
    ) -> Result<(String, String), anyhow::Error> {
        let credential = self.credential_store.get(device_uuid);
        let http = HttpRequest {};
        inner_get_device_ip_and_mac_address(service_url, credential, &http).await
    }

    /// Gets specific service, like media, from a given ONVIF camera
    async fn get_device_service_uri(
        &self,
        url: &str,
        service: &str,
    ) -> Result<String, anyhow::Error> {
        let http = HttpRequest {};
        inner_get_device_service_uri(url, service, &http).await
    }

    /// Gets the list of streaming profiles for a given ONVIF camera
    async fn get_device_profiles(&self, url: &str) -> Result<Vec<String>, anyhow::Error> {
        let http = HttpRequest {};
        inner_get_device_profiles(url, &http).await
    }

    /// Gets the streaming uri for a given ONVIF camera's profile
    async fn get_device_profile_streaming_uri(
        &self,
        url: &str,
        profile_token: &str,
    ) -> Result<String, anyhow::Error> {
        let http = HttpRequest {};
        inner_get_device_profile_streaming_uri(url, profile_token, &http).await
    }

    /// Calls the publically accessible GetSystemDateAndTime endpoint to determine
    /// that the camera is responsive. If responsive, returns the responding url.
    async fn is_device_responding(&self, url: &str) -> Result<String, anyhow::Error> {
        let http = HttpRequest {};
        inner_is_device_responding(url, &http).await
    }
}

/// Http can send an HTTP::Post.
///
/// An implementation of http can send an HTTP::Post.
#[cfg_attr(test, automock)]
#[async_trait]
trait Http {
    async fn post(&self, url: &str, mime_action: &str, msg: &str)
        -> Result<Package, anyhow::Error>;
}

struct HttpRequest {}

impl HttpRequest {
    /// This converts an http response body into an sxd_document::Package
    fn handle_request_body(body: &str) -> Result<Package, anyhow::Error> {
        let xml_as_tree = match parser::parse(body) {
            Ok(xml_as_tree) => xml_as_tree,
            Err(e) => return Err(Error::new(ErrorKind::InvalidData, e).into()),
        };
        trace!(
            "handle_request_body - response as xmltree: {:?}",
            xml_as_tree
        );
        Ok(xml_as_tree)
    }
}

#[async_trait]
impl Http for HttpRequest {
    /// This sends an HTTP::Post and converts the response body into an sxd_document::Package
    async fn post(
        &self,
        url: &str,
        mime_action: &str,
        msg: &str,
    ) -> Result<Package, anyhow::Error> {
        trace!(
            "post - url:{}, mime_action:{}, msg:{}",
            &url,
            &mime_action,
            &msg
        );

        let full_mime = format!(
            "{}; {}; {};",
            "application/soap+xml", "charset=utf-8", mime_action
        );
        let request = Request::post(url)
            .header("CONTENT-TYPE", full_mime)
            .body(msg.to_string().into())
            .expect("infallible");
        // terminate a request if it takes over a second
        let response = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            hyper::Client::new().request(request),
        )
        .await??;
        if response.status() != 200 {
            return Err(anyhow::format_err!(
                "Received a response status of {}, expected 200",
                response.status()
            ));
        }
        let response_body = response
            .into_body()
            .try_fold(bytes::BytesMut::new(), |mut acc, chunk| async {
                acc.extend(chunk);
                Ok(acc)
            })
            .await?
            .freeze();
        let response_body_str = std::str::from_utf8(&response_body)?;
        match HttpRequest::handle_request_body(response_body_str) {
            Ok(dom) => Ok(dom),
            Err(e) => Err(Error::new(ErrorKind::InvalidData, e).into()),
        }
    }
}

/// Creates a SOAP mime action
fn get_action(wsdl: &str, function: &str) -> String {
    format!("action=\"{}/{}\"", wsdl, function)
}

/// Gets the ip and mac address for a given ONVIF camera
async fn inner_get_device_ip_and_mac_address(
    service_url: &str,
    credential: Option<(String, Option<String>)>,
    http: &impl Http,
) -> Result<(String, String), anyhow::Error> {
    let username_token = credential.map(|(uname, passwd)| {
        UsernameToken::new(uname.as_str(), passwd.unwrap_or_default().as_str())
    });
    let message = get_network_interfaces_message(&username_token);
    let network_interfaces_xml = match http
        .post(
            service_url,
            &get_action(DEVICE_WSDL, "GetNetworkInterfaces"),
            message.as_str(),
        )
        .await
    {
        Ok(xml) => xml,
        Err(e) => {
            return Err(anyhow::format_err!(
                "failed to get network interfaces from device: {:?}",
                e
            ))
        }
    };
    let network_interfaces_doc = network_interfaces_xml.as_document();
    let ip_address = match sxd_xpath::evaluate_xpath(
            &network_interfaces_doc,
            "//*[local-name()='GetNetworkInterfacesResponse']/*[local-name()='NetworkInterfaces']/*[local-name()='IPv4']/*[local-name()='Config']/*/*[local-name()='Address']/text()"
        ) {
            Ok(Value::String(ip)) => ip,
            Ok(Value::Nodeset(ns)) => match ns.into_iter().map(|x| x.string_value()).collect::<Vec<String>>().first() {
                Some(first) => first.to_string(),
                None => return Err(anyhow::format_err!("Failed to get ONVIF ip address: none specified in response"))
            },
            Ok(Value::Boolean(_)) |
            Ok(Value::Number(_)) => return Err(anyhow::format_err!("Failed to get ONVIF ip address: unexpected type")),
            Err(e) => return Err(anyhow::format_err!("Failed to get ONVIF ip address: {}", e))
        };
    trace!(
        "inner_get_device_ip_and_mac_address - network interfaces (ip address): {:?}",
        ip_address
    );
    let mac_address = match sxd_xpath::evaluate_xpath(
            &network_interfaces_doc,
            "//*[local-name()='GetNetworkInterfacesResponse']/*[local-name()='NetworkInterfaces']/*[local-name()='Info']/*[local-name()='HwAddress']/text()"
        ) {
            Ok(Value::String(mac)) => mac,
            Ok(Value::Nodeset(ns)) => match ns.iter().map(|x| x.string_value()).collect::<Vec<String>>().first() {
                Some(first) => first.to_string(),
                None => return Err(anyhow::format_err!("Failed to get ONVIF mac address: none specified in response"))
            },
            Ok(Value::Boolean(_)) |
            Ok(Value::Number(_)) => return Err(anyhow::format_err!("Failed to get ONVIF mac address: unexpected type")),
            Err(e) => return Err(anyhow::format_err!("Failed to get ONVIF mac address: {}", e))
        };
    trace!(
        "inner_get_device_ip_and_mac_address - network interfaces (mac address): {:?}",
        mac_address
    );
    Ok((ip_address, mac_address))
}

fn get_soap_security_header(username_token: &UsernameToken) -> String {
    format!(
        r#"
    <wsse:Security soap:mustUnderstand="1" 
        xmlns:wsse="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-secext-1.0.xsd">
        <wsse:UsernameToken xmlns:wsu="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-utility-1.0.xsd">
            <wsse:Username>{}</wsse:Username>
            <wsse:Password Type="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-username-token-profile-1.0#PasswordDigest">{}</wsse:Password>
            <wsse:Nonce EncodingType="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-soap-message-security-1.0#Base64Binary">{}</wsse:Nonce>
            <wsu:Created xmlns="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-utility-1.0.xsd">{}</wsu:Created>
        </wsse:UsernameToken>
    </wsse:Security>
    "#,
        username_token.username,
        username_token.digest,
        username_token.nonce,
        username_token.created
    )
}

/// SOAP request body for getting the network interfaces for an ONVIF camera
fn get_network_interfaces_message(username_token: &Option<UsernameToken>) -> String {
    let security_header = if let Some(username_token) = username_token {
        get_soap_security_header(username_token)
    } else {
        "".to_string()
    };

    format!(
        r#"
<soap:Envelope 
    xmlns:soap="http://www.w3.org/2003/05/soap-envelope" 
    xmlns:wsdl="http://www.onvif.org/ver10/device/wsdl">
    <soap:Header>
    {}
    </soap:Header>
    <soap:Body>
        <wsdl:GetNetworkInterfaces/>
    </soap:Body>
</soap:Envelope>"#,
        security_header
    )
}

/// Gets a specific service (like media) uri from an ONVIF camera
async fn inner_get_device_service_uri(
    url: &str,
    service: &str,
    http: &impl Http,
) -> Result<String, anyhow::Error> {
    let services_xml = match http
        .post(
            url,
            &get_action(DEVICE_WSDL, "GetServices"),
            GET_SERVICES_TEMPLATE,
        )
        .await
    {
        Ok(xml) => xml,
        Err(e) => {
            return Err(anyhow::format_err!(
                "failed to get services from device: {:?}",
                e
            ))
        }
    };
    let services_doc = services_xml.as_document();
    let service_xpath_query = format!(
        "//*[local-name()='GetServicesResponse']/*[local-name()='Service' and *[local-name()='Namespace']/text() ='{}']/*[local-name()='XAddr']/text()",
        service
    );
    let requested_device_service_uri =
        match sxd_xpath::evaluate_xpath(&services_doc, service_xpath_query.as_str()) {
            Ok(uri) => uri.string(),
            Err(e) => {
                return Err(anyhow::format_err!(
                    "failed to get servuce uri from resoinse: {:?}",
                    e
                ))
            }
        };
    trace!(
        "inner_get_device_service_uri - service ({}) uris: {:?}",
        service,
        requested_device_service_uri
    );
    Ok(requested_device_service_uri)
}

/// SOAP request body for getting the supported services' uris for an ONVIF camera
const GET_SERVICES_TEMPLATE: &str = r#"<soap:Envelope xmlns:soap="http://www.w3.org/2003/05/soap-envelope" xmlns:wsdl="http://www.onvif.org/ver10/device/wsdl">
    <soap:Header/>
    <soap:Body>
        <wsdl:GetServices />
    </soap:Body>
</soap:Envelope>"#;

/// Gets list of media profiles for a given ONVIF camera
async fn inner_get_device_profiles(
    url: &str,
    http: &impl Http,
) -> Result<Vec<String>, anyhow::Error> {
    let action = get_action(MEDIA_WSDL, "GetProfiles");
    let message = GET_PROFILES_TEMPLATE.to_string();
    let profiles_xml = match http.post(url, &action, &message).await {
        Ok(xml) => xml,
        Err(e) => {
            return Err(anyhow::format_err!(
                "failed to get profiles from device: {:?}",
                e
            ))
        }
    };
    let profiles_doc = profiles_xml.as_document();
    let profiles_query = sxd_xpath::evaluate_xpath(
        &profiles_doc,
        "//*[local-name()='GetProfilesResponse']/*[local-name()='Profiles']/@token",
    );
    let profiles = match profiles_query {
        Ok(Value::Nodeset(profiles_items)) => profiles_items
            .iter()
            .map(|profile_item| profile_item.string_value())
            .collect::<Vec<String>>(),
        Ok(Value::Boolean(_)) | Ok(Value::Number(_)) | Ok(Value::String(_)) => {
            return Err(anyhow::format_err!(
                "Failed to get ONVIF profiles: unexpected type"
            ))
        }
        Err(e) => return Err(anyhow::format_err!("Failed to get ONVIF profiles: {}", e)),
    };
    trace!("inner_get_device_profiles - profiles: {:?}", profiles);
    Ok(profiles)
}

/// Gets the streaming uri for a given profile for an ONVIF camera
async fn inner_get_device_profile_streaming_uri(
    url: &str,
    profile_token: &str,
    http: &impl Http,
) -> Result<String, anyhow::Error> {
    let stream_soap = get_stream_uri_message(profile_token);
    let stream_uri_xml = match http
        .post(url, &get_action(MEDIA_WSDL, "GetStreamUri"), &stream_soap)
        .await
    {
        Ok(xml) => xml,
        Err(e) => {
            return Err(anyhow::format_err!(
                "failed to get streaming uri from device: {:?}",
                e
            ))
        }
    };
    let stream_uri_doc = stream_uri_xml.as_document();
    let stream_uri = match sxd_xpath::evaluate_xpath(
        &stream_uri_doc,
        "//*[local-name()='GetStreamUriResponse']/*[local-name()='MediaUri']/*[local-name()='Uri']/text()"
        ) {
            Ok(stream) => stream.string(),
            Err(e) => {
                return Err(anyhow::format_err!(
                    "failed to get service uri from response: {:?}",
                    e
                ))
            }
        };
    Ok(stream_uri)
}

/// Gets the streaming uri for a given profile for an ONVIF camera
async fn inner_is_device_responding(url: &str, http: &impl Http) -> Result<String, anyhow::Error> {
    http.post(
        url,
        &get_action(DEVICE_WSDL, "GetSystemDateAndTime"),
        GET_SYSTEM_DATE_AND_TIME_TEMPLATE,
    )
    .await?;
    Ok(url.to_string())
}

/// Gets SOAP request body for getting the streaming uri for a specific profile for an ONVIF camera
fn get_stream_uri_message(profile: &str) -> String {
    format!(
        r#"<soap:Envelope xmlns:soap="http://www.w3.org/2003/05/soap-envelope" xmlns:wsdl="http://www.onvif.org/ver10/media/wsdl" xmlns:sch="http://www.onvif.org/ver10/schema">
            <soap:Header/>
    <soap:Body>
        <wsdl:GetStreamUri>
        <wsdl:StreamSetup>
            <sch:Stream>RTP-Unicast</sch:Stream>
            <sch:Transport>
                <sch:Protocol>RTSP</sch:Protocol>
            </sch:Transport>
        </wsdl:StreamSetup>
        <wsdl:ProfileToken>{}</wsdl:ProfileToken>
        </wsdl:GetStreamUri>
    </soap:Body>
</soap:Envelope>;"#,
        profile
    )
}

/// SOAP request body for getting the media profiles for an ONVIF camera
const GET_PROFILES_TEMPLATE: &str = r#"<soap:Envelope xmlns:soap="http://www.w3.org/2003/05/soap-envelope" xmlns:wsdl="http://www.onvif.org/ver10/media/wsdl">
    <soap:Header/>
    <soap:Body>
        <wsdl:GetProfiles/>
    </soap:Body>
</soap:Envelope>"#;

//  const GET_DEVICE_INFORMATION_TEMPLATE: &str = r#"<soap:Envelope xmlns:soap="http://www.w3.org/2003/05/soap-envelope" xmlns:wsdl="http://www.onvif.org/ver10/device/wsdl">
//     <soap:Header/>
//         <soap:Body>
//             <wsdl:GetDeviceInformation/>
//         </soap:Body>
//     </soap:Envelope>"#;

const GET_SYSTEM_DATE_AND_TIME_TEMPLATE: &str = r#"<soap:Envelope xmlns:soap="http://www.w3.org/2003/05/soap-envelope" xmlns:wsdl="http://www.onvif.org/ver10/device/wsdl">
    <soap:Header/>
    <soap:Body>
        <wsdl:GetSystemDateAndTime/>
    </soap:Body>
</soap:Envelope>"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn configure_post(mock: &mut MockHttp, url: &str, mime: &str, msg: &str, output_xml: &str) {
        let inner_url = url.to_string();
        let inner_mime = mime.to_string();
        let inner_msg = msg.to_string();
        let inner_output_xml = output_xml.to_string();
        trace!("mock.expect_post url:{}, mime:{}, msg:{}", url, mime, msg);
        mock.expect_post()
            .times(1)
            .withf(move |actual_url, actual_mime, actual_msg| {
                actual_url == inner_url && actual_mime == inner_mime && actual_msg == inner_msg
            })
            .returning(move |_, _, _| {
                let xml_as_tree = parser::parse(&inner_output_xml).unwrap();
                Ok(xml_as_tree)
            });
    }

    #[tokio::test]
    async fn test_inner_get_device_ip_and_mac_address_ip_in_manual() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mut mock = MockHttp::new();
        let response = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<SOAP-ENV:Envelope xmlns:SOAP-ENV=\"http://www.w3.org/2003/05/soap-envelope\" xmlns:SOAP-ENC=\"http://www.w3.org/2003/05/soap-encoding\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xmlns:xsd=\"http://www.w3.org/2001/XMLSchema\" xmlns:xs=\"http://www.w3.org/2000/10/XMLSchema\" xmlns:wsse=\"http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-secext-1.0.xsd\" xmlns:wsa5=\"http://www.w3.org/2005/08/addressing\" xmlns:xop=\"http://www.w3.org/2004/08/xop/include\" xmlns:wsa=\"http://schemas.xmlsoap.org/ws/2004/08/addressing\" xmlns:tt=\"http://www.onvif.org/ver10/schema\" xmlns:ns1=\"http://www.w3.org/2005/05/xmlmime\" xmlns:wstop=\"http://docs.oasis-open.org/wsn/t-1\" xmlns:ns7=\"http://docs.oasis-open.org/wsrf/r-2\" xmlns:ns2=\"http://docs.oasis-open.org/wsrf/bf-2\" xmlns:dndl=\"http://www.onvif.org/ver10/network/wsdl/DiscoveryLookupBinding\" xmlns:dnrd=\"http://www.onvif.org/ver10/network/wsdl/RemoteDiscoveryBinding\" xmlns:d=\"http://schemas.xmlsoap.org/ws/2005/04/discovery\" xmlns:dn=\"http://www.onvif.org/ver10/network/wsdl\" xmlns:ns10=\"http://www.onvif.org/ver10/replay/wsdl\" xmlns:ns11=\"http://www.onvif.org/ver10/search/wsdl\" xmlns:ns13=\"http://www.onvif.org/ver20/analytics/wsdl/RuleEngineBinding\" xmlns:ns14=\"http://www.onvif.org/ver20/analytics/wsdl/AnalyticsEngineBinding\" xmlns:tan=\"http://www.onvif.org/ver20/analytics/wsdl\" xmlns:ns15=\"http://www.onvif.org/ver10/events/wsdl/PullPointSubscriptionBinding\" xmlns:ns16=\"http://www.onvif.org/ver10/events/wsdl/EventBinding\" xmlns:tev=\"http://www.onvif.org/ver10/events/wsdl\" xmlns:ns17=\"http://www.onvif.org/ver10/events/wsdl/SubscriptionManagerBinding\" xmlns:ns18=\"http://www.onvif.org/ver10/events/wsdl/NotificationProducerBinding\" xmlns:ns19=\"http://www.onvif.org/ver10/events/wsdl/NotificationConsumerBinding\" xmlns:ns20=\"http://www.onvif.org/ver10/events/wsdl/PullPointBinding\" xmlns:ns21=\"http://www.onvif.org/ver10/events/wsdl/CreatePullPointBinding\" xmlns:ns22=\"http://www.onvif.org/ver10/events/wsdl/PausableSubscriptionManagerBinding\" xmlns:wsnt=\"http://docs.oasis-open.org/wsn/b-2\" xmlns:ns3=\"http://www.onvif.org/ver10/analyticsdevice/wsdl\" xmlns:ns4=\"http://www.onvif.org/ver10/deviceIO/wsdl\" xmlns:ns5=\"http://www.onvif.org/ver10/display/wsdl\" xmlns:ns8=\"http://www.onvif.org/ver10/receiver/wsdl\" xmlns:ns9=\"http://www.onvif.org/ver10/recording/wsdl\" xmlns:tds=\"http://www.onvif.org/ver10/device/wsdl\" xmlns:timg=\"http://www.onvif.org/ver20/imaging/wsdl\" xmlns:tptz=\"http://www.onvif.org/ver20/ptz/wsdl\" xmlns:trt=\"http://www.onvif.org/ver10/media/wsdl\" xmlns:trt2=\"http://www.onvif.org/ver20/media/wsdl\" xmlns:ter=\"http://www.onvif.org/ver10/error\" xmlns:tns1=\"http://www.onvif.org/ver10/topics\" xmlns:tnsn=\"http://www.eventextension.com/2011/event/topics\"><SOAP-ENV:Header></SOAP-ENV:Header><SOAP-ENV:Body><tds:GetNetworkInterfacesResponse><tds:NetworkInterfaces token=\"eth0\"><tt:Enabled>true</tt:Enabled><tt:Info><tt:Name>eth0</tt:Name><tt:HwAddress>00:12:41:5c:a1:a5</tt:HwAddress><tt:MTU>1500</tt:MTU></tt:Info><tt:Link><tt:AdminSettings><tt:AutoNegotiation>false</tt:AutoNegotiation><tt:Speed>10</tt:Speed><tt:Duplex>Full</tt:Duplex></tt:AdminSettings><tt:OperSettings><tt:AutoNegotiation>false</tt:AutoNegotiation><tt:Speed>10</tt:Speed><tt:Duplex>Full</tt:Duplex></tt:OperSettings><tt:InterfaceType>0</tt:InterfaceType></tt:Link><tt:IPv4><tt:Enabled>true</tt:Enabled><tt:Config><tt:Manual><tt:Address>192.168.1.36</tt:Address><tt:PrefixLength>24</tt:PrefixLength></tt:Manual><tt:DHCP>false</tt:DHCP></tt:Config></tt:IPv4></tds:NetworkInterfaces></tds:GetNetworkInterfacesResponse></SOAP-ENV:Body></SOAP-ENV:Envelope>";
        let username_token = None;
        let message = get_network_interfaces_message(&username_token);
        configure_post(
            &mut mock,
            "test_inner_get_device_ip_and_mac_address-url",
            &get_action(DEVICE_WSDL, "GetNetworkInterfaces"),
            &message,
            response,
        );
        assert_eq!(
            ("192.168.1.36".to_string(), "00:12:41:5c:a1:a5".to_string()),
            inner_get_device_ip_and_mac_address(
                "test_inner_get_device_ip_and_mac_address-url",
                None,
                &mock
            )
            .await
            .unwrap()
        );
    }

    #[tokio::test]
    async fn test_inner_get_device_ip_and_mac_address_ip_in_from_dhcp() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mut mock = MockHttp::new();
        let response = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<SOAP-ENV:Envelope xmlns:SOAP-ENV=\"http://www.w3.org/2003/05/soap-envelope\" xmlns:SOAP-ENC=\"http://www.w3.org/2003/05/soap-encoding\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xmlns:xsd=\"http://www.w3.org/2001/XMLSchema\" xmlns:wsa5=\"http://www.w3.org/2005/08/addressing\" xmlns:c14n=\"http://www.w3.org/2001/10/xml-exc-c14n#\" xmlns:wsu=\"http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-utility-1.0.xsd\" xmlns:xenc=\"http://www.w3.org/2001/04/xmlenc#\" xmlns:ds=\"http://www.w3.org/2000/09/xmldsig#\" xmlns:wsse=\"http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-secext-1.0.xsd\" xmlns:xmime=\"http://tempuri.org/xmime.xsd\" xmlns:xop=\"http://www.w3.org/2004/08/xop/include\" xmlns:wsa=\"http://schemas.xmlsoap.org/ws/2004/08/addressing\" xmlns:tt=\"http://www.onvif.org/ver10/schema\" xmlns:wsbf2=\"http://docs.oasis-open.org/wsrf/bf-2\" xmlns:wstop=\"http://docs.oasis-open.org/wsn/t-1\" xmlns:wsr2=\"http://docs.oasis-open.org/wsrf/r-2\" xmlns:daae=\"http://www.onvif.org/ver20/analytics/wsdl/AnalyticsEngineBinding\" xmlns:dare=\"http://www.onvif.org/ver20/analytics/wsdl/RuleEngineBinding\" xmlns:tan=\"http://www.onvif.org/ver20/analytics/wsdl\" xmlns:decpp=\"http://www.onvif.org/ver10/events/wsdl/CreatePullPointBinding\" xmlns:dee=\"http://www.onvif.org/ver10/events/wsdl/EventBinding\" xmlns:denc=\"http://www.onvif.org/ver10/events/wsdl/NotificationConsumerBinding\" xmlns:denf=\"http://www.onvif.org/ver10/events/wsdl/NotificationProducerBinding\" xmlns:depp=\"http://www.onvif.org/ver10/events/wsdl/PullPointBinding\" xmlns:depps=\"http://www.onvif.org/ver10/events/wsdl/PullPointSubscriptionBinding\" xmlns:tev=\"http://www.onvif.org/ver10/events/wsdl\" xmlns:depsm=\"http://www.onvif.org/ver10/events/wsdl/PausableSubscriptionManagerBinding\" xmlns:wsnt=\"http://docs.oasis-open.org/wsn/b-2\" xmlns:desm=\"http://www.onvif.org/ver10/events/wsdl/SubscriptionManagerBinding\" xmlns:dndl=\"http://www.onvif.org/ver10/network/wsdl/DiscoveryLookupBinding\" xmlns:dnrd=\"http://www.onvif.org/ver10/network/wsdl/RemoteDiscoveryBinding\" xmlns:d=\"http://schemas.xmlsoap.org/ws/2005/04/discovery\" xmlns:dn=\"http://www.onvif.org/ver10/network/wsdl\" xmlns:tad=\"http://www.onvif.org/ver10/analyticsdevice/wsdl\" xmlns:tds=\"http://www.onvif.org/ver10/device/wsdl\" xmlns:timg=\"http://www.onvif.org/ver20/imaging/wsdl\" xmlns:tls=\"http://www.onvif.org/ver10/display/wsdl\" xmlns:tmd=\"http://www.onvif.org/ver10/deviceIO/wsdl\" xmlns:tptz=\"http://www.onvif.org/ver20/ptz/wsdl\" xmlns:trc=\"http://www.onvif.org/ver10/recording/wsdl\" xmlns:trp=\"http://www.onvif.org/ver10/replay/wsdl\" xmlns:trt=\"http://www.onvif.org/ver10/media/wsdl\" xmlns:trv=\"http://www.onvif.org/ver10/receiver/wsdl\" xmlns:tse=\"http://www.onvif.org/ver10/search/wsdl\" xmlns:ter=\"http://www.onvif.org/ver10/error\" xmlns:tns1=\"http://www.onvif.org/ver10/topics\" xmlns:tnsn=\"http://www.eventextension.com/2011/event/topics\"><SOAP-ENV:Header></SOAP-ENV:Header><SOAP-ENV:Body><tds:GetNetworkInterfacesResponse><tds:NetworkInterfaces token=\"eth0\"><tt:Enabled>true</tt:Enabled><tt:Info><tt:Name>eth0</tt:Name><tt:HwAddress>00:FC:DA:B1:69:CC</tt:HwAddress><tt:MTU>1500</tt:MTU></tt:Info><tt:IPv4><tt:Enabled>true</tt:Enabled><tt:Config><tt:LinkLocal><tt:Address>10.137.185.208</tt:Address><tt:PrefixLength>0</tt:PrefixLength></tt:LinkLocal><tt:FromDHCP><tt:Address>10.137.185.208</tt:Address><tt:PrefixLength>23</tt:PrefixLength></tt:FromDHCP><tt:DHCP>true</tt:DHCP></tt:Config></tt:IPv4></tds:NetworkInterfaces></tds:GetNetworkInterfacesResponse></SOAP-ENV:Body></SOAP-ENV:Envelope>\r\n";
        let username_token = None;
        let message = get_network_interfaces_message(&username_token);
        configure_post(
            &mut mock,
            "test_inner_get_device_ip_and_mac_address-url",
            &get_action(DEVICE_WSDL, "GetNetworkInterfaces"),
            &message,
            response,
        );
        assert_eq!(
            (
                "10.137.185.208".to_string(),
                "00:FC:DA:B1:69:CC".to_string()
            ),
            inner_get_device_ip_and_mac_address(
                "test_inner_get_device_ip_and_mac_address-url",
                None,
                &mock
            )
            .await
            .unwrap()
        );
    }

    #[tokio::test]
    async fn test_inner_get_device_service_uri() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mut mock = MockHttp::new();
        let response = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<SOAP-ENV:Envelope xmlns:SOAP-ENV=\"http://www.w3.org/2003/05/soap-envelope\" xmlns:SOAP-ENC=\"http://www.w3.org/2003/05/soap-encoding\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xmlns:xsd=\"http://www.w3.org/2001/XMLSchema\" xmlns:xs=\"http://www.w3.org/2000/10/XMLSchema\" xmlns:wsse=\"http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-secext-1.0.xsd\" xmlns:wsa5=\"http://www.w3.org/2005/08/addressing\" xmlns:xop=\"http://www.w3.org/2004/08/xop/include\" xmlns:wsa=\"http://schemas.xmlsoap.org/ws/2004/08/addressing\" xmlns:tt=\"http://www.onvif.org/ver10/schema\" xmlns:ns1=\"http://www.w3.org/2005/05/xmlmime\" xmlns:wstop=\"http://docs.oasis-open.org/wsn/t-1\" xmlns:ns7=\"http://docs.oasis-open.org/wsrf/r-2\" xmlns:ns2=\"http://docs.oasis-open.org/wsrf/bf-2\" xmlns:dndl=\"http://www.onvif.org/ver10/network/wsdl/DiscoveryLookupBinding\" xmlns:dnrd=\"http://www.onvif.org/ver10/network/wsdl/RemoteDiscoveryBinding\" xmlns:d=\"http://schemas.xmlsoap.org/ws/2005/04/discovery\" xmlns:dn=\"http://www.onvif.org/ver10/network/wsdl\" xmlns:ns10=\"http://www.onvif.org/ver10/replay/wsdl\" xmlns:ns11=\"http://www.onvif.org/ver10/search/wsdl\" xmlns:ns13=\"http://www.onvif.org/ver20/analytics/wsdl/RuleEngineBinding\" xmlns:ns14=\"http://www.onvif.org/ver20/analytics/wsdl/AnalyticsEngineBinding\" xmlns:tan=\"http://www.onvif.org/ver20/analytics/wsdl\" xmlns:ns15=\"http://www.onvif.org/ver10/events/wsdl/PullPointSubscriptionBinding\" xmlns:ns16=\"http://www.onvif.org/ver10/events/wsdl/EventBinding\" xmlns:tev=\"http://www.onvif.org/ver10/events/wsdl\" xmlns:ns17=\"http://www.onvif.org/ver10/events/wsdl/SubscriptionManagerBinding\" xmlns:ns18=\"http://www.onvif.org/ver10/events/wsdl/NotificationProducerBinding\" xmlns:ns19=\"http://www.onvif.org/ver10/events/wsdl/NotificationConsumerBinding\" xmlns:ns20=\"http://www.onvif.org/ver10/events/wsdl/PullPointBinding\" xmlns:ns21=\"http://www.onvif.org/ver10/events/wsdl/CreatePullPointBinding\" xmlns:ns22=\"http://www.onvif.org/ver10/events/wsdl/PausableSubscriptionManagerBinding\" xmlns:wsnt=\"http://docs.oasis-open.org/wsn/b-2\" xmlns:ns3=\"http://www.onvif.org/ver10/analyticsdevice/wsdl\" xmlns:ns4=\"http://www.onvif.org/ver10/deviceIO/wsdl\" xmlns:ns5=\"http://www.onvif.org/ver10/display/wsdl\" xmlns:ns8=\"http://www.onvif.org/ver10/receiver/wsdl\" xmlns:ns9=\"http://www.onvif.org/ver10/recording/wsdl\" xmlns:tds=\"http://www.onvif.org/ver10/device/wsdl\" xmlns:timg=\"http://www.onvif.org/ver20/imaging/wsdl\" xmlns:tptz=\"http://www.onvif.org/ver20/ptz/wsdl\" xmlns:trt=\"http://www.onvif.org/ver10/media/wsdl\" xmlns:trt2=\"http://www.onvif.org/ver20/media/wsdl\" xmlns:ter=\"http://www.onvif.org/ver10/error\" xmlns:tns1=\"http://www.onvif.org/ver10/topics\" xmlns:tnsn=\"http://www.eventextension.com/2011/event/topics\"><SOAP-ENV:Header></SOAP-ENV:Header><SOAP-ENV:Body><tds:GetServicesResponse><tds:Service><tds:Namespace>http://www.onvif.org/ver10/device/wsdl</tds:Namespace><tds:XAddr>http://192.168.1.35:8899/onvif/device_service</tds:XAddr><tds:Version><tt:Major>2</tt:Major><tt:Minor>41</tt:Minor></tds:Version></tds:Service><tds:Service><tds:Namespace>http://www.onvif.org/ver10/media/wsdl</tds:Namespace><tds:XAddr>http://192.168.1.35:8899/onvif/Media</tds:XAddr><tds:Version><tt:Major>2</tt:Major><tt:Minor>41</tt:Minor></tds:Version></tds:Service><tds:Service><tds:Namespace>http://www.onvif.org/ver10/events/wsdl</tds:Namespace><tds:XAddr>http://192.168.1.35:8899/onvif/Events</tds:XAddr><tds:Version><tt:Major>2</tt:Major><tt:Minor>41</tt:Minor></tds:Version></tds:Service><tds:Service><tds:Namespace>http://www.onvif.org/ver20/imaging/wsdl</tds:Namespace><tds:XAddr>http://192.168.1.35:8899/onvif/Imaging</tds:XAddr><tds:Version><tt:Major>2</tt:Major><tt:Minor>41</tt:Minor></tds:Version></tds:Service><tds:Service><tds:Namespace>http://www.onvif.org/ver20/ptz/wsdl</tds:Namespace><tds:XAddr>http://192.168.1.35:8899/onvif/PTZ</tds:XAddr><tds:Version><tt:Major>2</tt:Major><tt:Minor>41</tt:Minor></tds:Version></tds:Service></tds:GetServicesResponse></SOAP-ENV:Body></SOAP-ENV:Envelope>";
        configure_post(
            &mut mock,
            "test_inner_get_device_service_uri-url",
            &get_action(DEVICE_WSDL, "GetServices"),
            GET_SERVICES_TEMPLATE,
            response,
        );
        assert_eq!(
            "http://192.168.1.35:8899/onvif/Media".to_string(),
            inner_get_device_service_uri(
                "test_inner_get_device_service_uri-url",
                MEDIA_WSDL,
                &mock
            )
            .await
            .unwrap()
        );
    }

    #[tokio::test]
    async fn test_inner_get_device_profiles() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mut mock = MockHttp::new();
        // GetProfiles is the first call
        {
            let response = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<SOAP-ENV:Envelope xmlns:SOAP-ENV=\"http://www.w3.org/2003/05/soap-envelope\" xmlns:SOAP-ENC=\"http://www.w3.org/2003/05/soap-encoding\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xmlns:xsd=\"http://www.w3.org/2001/XMLSchema\" xmlns:xs=\"http://www.w3.org/2000/10/XMLSchema\" xmlns:wsse=\"http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-secext-1.0.xsd\" xmlns:wsa5=\"http://www.w3.org/2005/08/addressing\" xmlns:xop=\"http://www.w3.org/2004/08/xop/include\" xmlns:wsa=\"http://schemas.xmlsoap.org/ws/2004/08/addressing\" xmlns:tt=\"http://www.onvif.org/ver10/schema\" xmlns:ns1=\"http://www.w3.org/2005/05/xmlmime\" xmlns:wstop=\"http://docs.oasis-open.org/wsn/t-1\" xmlns:ns7=\"http://docs.oasis-open.org/wsrf/r-2\" xmlns:ns2=\"http://docs.oasis-open.org/wsrf/bf-2\" xmlns:dndl=\"http://www.onvif.org/ver10/network/wsdl/DiscoveryLookupBinding\" xmlns:dnrd=\"http://www.onvif.org/ver10/network/wsdl/RemoteDiscoveryBinding\" xmlns:d=\"http://schemas.xmlsoap.org/ws/2005/04/discovery\" xmlns:dn=\"http://www.onvif.org/ver10/network/wsdl\" xmlns:ns10=\"http://www.onvif.org/ver10/replay/wsdl\" xmlns:ns11=\"http://www.onvif.org/ver10/search/wsdl\" xmlns:ns13=\"http://www.onvif.org/ver20/analytics/wsdl/RuleEngineBinding\" xmlns:ns14=\"http://www.onvif.org/ver20/analytics/wsdl/AnalyticsEngineBinding\" xmlns:tan=\"http://www.onvif.org/ver20/analytics/wsdl\" xmlns:ns15=\"http://www.onvif.org/ver10/events/wsdl/PullPointSubscriptionBinding\" xmlns:ns16=\"http://www.onvif.org/ver10/events/wsdl/EventBinding\" xmlns:tev=\"http://www.onvif.org/ver10/events/wsdl\" xmlns:ns17=\"http://www.onvif.org/ver10/events/wsdl/SubscriptionManagerBinding\" xmlns:ns18=\"http://www.onvif.org/ver10/events/wsdl/NotificationProducerBinding\" xmlns:ns19=\"http://www.onvif.org/ver10/events/wsdl/NotificationConsumerBinding\" xmlns:ns20=\"http://www.onvif.org/ver10/events/wsdl/PullPointBinding\" xmlns:ns21=\"http://www.onvif.org/ver10/events/wsdl/CreatePullPointBinding\" xmlns:ns22=\"http://www.onvif.org/ver10/events/wsdl/PausableSubscriptionManagerBinding\" xmlns:wsnt=\"http://docs.oasis-open.org/wsn/b-2\" xmlns:ns3=\"http://www.onvif.org/ver10/analyticsdevice/wsdl\" xmlns:ns4=\"http://www.onvif.org/ver10/deviceIO/wsdl\" xmlns:ns5=\"http://www.onvif.org/ver10/display/wsdl\" xmlns:ns8=\"http://www.onvif.org/ver10/receiver/wsdl\" xmlns:ns9=\"http://www.onvif.org/ver10/recording/wsdl\" xmlns:tds=\"http://www.onvif.org/ver10/device/wsdl\" xmlns:timg=\"http://www.onvif.org/ver20/imaging/wsdl\" xmlns:tptz=\"http://www.onvif.org/ver20/ptz/wsdl\" xmlns:trt=\"http://www.onvif.org/ver10/media/wsdl\" xmlns:trt2=\"http://www.onvif.org/ver20/media/wsdl\" xmlns:ter=\"http://www.onvif.org/ver10/error\" xmlns:tns1=\"http://www.onvif.org/ver10/topics\" xmlns:tnsn=\"http://www.eventextension.com/2011/event/topics\"><SOAP-ENV:Header></SOAP-ENV:Header><SOAP-ENV:Body><trt:GetProfilesResponse><trt:Profiles fixed=\"true\" token=\"000\"><tt:Name>Profile_000</tt:Name><tt:VideoSourceConfiguration token=\"000\"><tt:Name>VideoS_000</tt:Name><tt:UseCount>3</tt:UseCount><tt:SourceToken>000</tt:SourceToken><tt:Bounds height=\"1080\" width=\"1920\" y=\"0\" x=\"0\"></tt:Bounds></tt:VideoSourceConfiguration><tt:AudioSourceConfiguration token=\"000\"><tt:Name>Audio_000</tt:Name><tt:UseCount>2</tt:UseCount><tt:SourceToken>000</tt:SourceToken></tt:AudioSourceConfiguration><tt:VideoEncoderConfiguration token=\"000\"><tt:Name>VideoE_000</tt:Name><tt:UseCount>1</tt:UseCount><tt:Encoding>H264</tt:Encoding><tt:Resolution><tt:Width>1280</tt:Width><tt:Height>720</tt:Height></tt:Resolution><tt:Quality>5</tt:Quality><tt:RateControl><tt:FrameRateLimit>25</tt:FrameRateLimit><tt:EncodingInterval>1</tt:EncodingInterval><tt:BitrateLimit>2560</tt:BitrateLimit></tt:RateControl><tt:H264><tt:GovLength>2</tt:GovLength><tt:H264Profile>High</tt:H264Profile></tt:H264><tt:Multicast><tt:Address><tt:Type>IPv4</tt:Type><tt:IPv4Address>224.1.2.3</tt:IPv4Address></tt:Address><tt:Port>0</tt:Port><tt:TTL>0</tt:TTL><tt:AutoStart>false</tt:AutoStart></tt:Multicast><tt:SessionTimeout>PT10S</tt:SessionTimeout></tt:VideoEncoderConfiguration><tt:AudioEncoderConfiguration token=\"000\"><tt:Name>AudioE_000</tt:Name><tt:UseCount>2</tt:UseCount><tt:Encoding>G711</tt:Encoding><tt:Bitrate>64</tt:Bitrate><tt:SampleRate>8</tt:SampleRate><tt:Multicast><tt:Address><tt:Type>IPv4</tt:Type><tt:IPv4Address>224.1.2.3</tt:IPv4Address></tt:Address><tt:Port>0</tt:Port><tt:TTL>0</tt:TTL><tt:AutoStart>false</tt:AutoStart></tt:Multicast><tt:SessionTimeout>PT10S</tt:SessionTimeout></tt:AudioEncoderConfiguration><tt:VideoAnalyticsConfiguration token=\"000\"><tt:Name>Analytics_000</tt:Name><tt:UseCount>2</tt:UseCount><tt:AnalyticsEngineConfiguration><tt:AnalyticsModule Type=\"tt:CellMotionEngine\" Name=\"MyCellMotionEngine\"><tt:Parameters><tt:SimpleItem Value=\"4\" Name=\"Sensitivity\"></tt:SimpleItem><tt:ElementItem Name=\"Layout\"><tt:CellLayout Columns=\"22\" Rows=\"18\"><tt:Transformation><tt:Translate x=\"-1.0\" y=\"-1.0\" /><tt:Scale x=\"0.09090\" y=\"0.111111\" /></tt:Transformation></tt:CellLayout></tt:ElementItem></tt:Parameters></tt:AnalyticsModule><tt:AnalyticsModule Type=\"tt:TamperEngine\" Name=\"MyTamperEngine\"><tt:Parameters><tt:SimpleItem Value=\"4\" Name=\"Sensitivity\"></tt:SimpleItem><tt:ElementItem Name=\"Field\"><tt:PolygonConfiguration><tt:Polygon><tt:Point x=\"0\" y=\"0\"/><tt:Point x=\"0\" y=\"0\"/><tt:Point x=\"0\" y=\"0\"/><tt:Point x=\"0\" y=\"0\"/></tt:Polygon></tt:PolygonConfiguration></tt:ElementItem><tt:ElementItem Name=\"Transform\"><tt:Transformation><tt:Translate x=\"-1.0\" y=\"-1.0\"/><tt:Scale x=\"0.001250\" y=\"0.001667\"/></tt:Transformation></tt:ElementItem></tt:Parameters></tt:AnalyticsModule></tt:AnalyticsEngineConfiguration><tt:RuleEngineConfiguration><tt:Rule Type=\"tt:CellMotionDetector\" Name=\"MyMotionDetectorRule\"><tt:Parameters><tt:SimpleItem Value=\"zwA\" Name=\"ActiveCells\"></tt:SimpleItem><tt:SimpleItem Value=\"1000\" Name=\"AlarmOffDelay\"></tt:SimpleItem><tt:SimpleItem Value=\"1000\" Name=\"AlarmOnDelay\"></tt:SimpleItem><tt:SimpleItem Value=\"4\" Name=\"MinCount\"></tt:SimpleItem></tt:Parameters></tt:Rule><tt:Rule Type=\"tt:TamperDetector\" Name=\"MyTamperDetectorRule\"><tt:Parameters><tt:ElementItem Name=\"Field\"><tt:PolygonConfiguration><tt:Polygon><tt:Point x=\"0\" y=\"0\"/><tt:Point x=\"0\" y=\"0\"/><tt:Point x=\"0\" y=\"0\"/><tt:Point x=\"0\" y=\"0\"/></tt:Polygon></tt:PolygonConfiguration></tt:ElementItem></tt:Parameters></tt:Rule></tt:RuleEngineConfiguration></tt:VideoAnalyticsConfiguration><tt:PTZConfiguration token=\"000\"><tt:Name>PTZ_000</tt:Name><tt:UseCount>2</tt:UseCount><tt:NodeToken>000</tt:NodeToken><tt:DefaultRelativePanTiltTranslationSpace>http://www.onvif.org/ver10/tptz/PanTiltSpaces/TranslationGenericSpace</tt:DefaultRelativePanTiltTranslationSpace><tt:DefaultRelativeZoomTranslationSpace>http://www.onvif.org/ver10/tptz/ZoomSpaces/TranslationGenericSpace</tt:DefaultRelativeZoomTranslationSpace><tt:DefaultContinuousPanTiltVelocitySpace>http://www.onvif.org/ver10/tptz/PanTiltSpaces/VelocityGenericSpace</tt:DefaultContinuousPanTiltVelocitySpace><tt:DefaultContinuousZoomVelocitySpace>http://www.onvif.org/ver10/tptz/ZoomSpaces/VelocityGenericSpace</tt:DefaultContinuousZoomVelocitySpace><tt:DefaultPTZSpeed><tt:PanTilt space=\"http://www.onvif.org/ver10/tptz/PanTiltSpaces/GenericSpeedSpace\" y=\"1\" x=\"1\"></tt:PanTilt><tt:Zoom space=\"http://www.onvif.org/ver10/tptz/ZoomSpaces/ZoomGenericSpeedSpace\" x=\"1\"></tt:Zoom></tt:DefaultPTZSpeed><tt:DefaultPTZTimeout>PT1S</tt:DefaultPTZTimeout><tt:PanTiltLimits><tt:Range><tt:URI>http://www.onvif.org/ver10/tptz/PanTiltSpaces/PositionGenericSpace</tt:URI><tt:XRange><tt:Min>-1</tt:Min><tt:Max>1</tt:Max></tt:XRange><tt:YRange><tt:Min>-1</tt:Min><tt:Max>1</tt:Max></tt:YRange></tt:Range></tt:PanTiltLimits><tt:ZoomLimits><tt:Range><tt:URI>http://www.onvif.org/ver10/tptz/ZoomSpaces/PositionGenericSpace</tt:URI><tt:XRange><tt:Min>-1</tt:Min><tt:Max>1</tt:Max></tt:XRange></tt:Range></tt:ZoomLimits></tt:PTZConfiguration></trt:Profiles><trt:Profiles fixed=\"true\" token=\"001\"><tt:Name>Profile_001</tt:Name><tt:VideoSourceConfiguration token=\"000\"><tt:Name>VideoS_000</tt:Name><tt:UseCount>3</tt:UseCount><tt:SourceToken>000</tt:SourceToken><tt:Bounds height=\"1080\" width=\"1920\" y=\"0\" x=\"0\"></tt:Bounds></tt:VideoSourceConfiguration><tt:AudioSourceConfiguration token=\"000\"><tt:Name>Audio_000</tt:Name><tt:UseCount>2</tt:UseCount><tt:SourceToken>000</tt:SourceToken></tt:AudioSourceConfiguration><tt:VideoEncoderConfiguration token=\"001\"><tt:Name>VideoE_001</tt:Name><tt:UseCount>1</tt:UseCount><tt:Encoding>H264</tt:Encoding><tt:Resolution><tt:Width>704</tt:Width><tt:Height>576</tt:Height></tt:Resolution><tt:Quality>5</tt:Quality><tt:RateControl><tt:FrameRateLimit>25</tt:FrameRateLimit><tt:EncodingInterval>1</tt:EncodingInterval><tt:BitrateLimit>1024</tt:BitrateLimit></tt:RateControl><tt:H264><tt:GovLength>2</tt:GovLength><tt:H264Profile>High</tt:H264Profile></tt:H264><tt:Multicast><tt:Address><tt:Type>IPv4</tt:Type><tt:IPv4Address>224.1.2.3</tt:IPv4Address></tt:Address><tt:Port>0</tt:Port><tt:TTL>0</tt:TTL><tt:AutoStart>false</tt:AutoStart></tt:Multicast><tt:SessionTimeout>PT10S</tt:SessionTimeout></tt:VideoEncoderConfiguration><tt:AudioEncoderConfiguration token=\"000\"><tt:Name>AudioE_000</tt:Name><tt:UseCount>2</tt:UseCount><tt:Encoding>G711</tt:Encoding><tt:Bitrate>64</tt:Bitrate><tt:SampleRate>8</tt:SampleRate><tt:Multicast><tt:Address><tt:Type>IPv4</tt:Type><tt:IPv4Address>224.1.2.3</tt:IPv4Address></tt:Address><tt:Port>0</tt:Port><tt:TTL>0</tt:TTL><tt:AutoStart>false</tt:AutoStart></tt:Multicast><tt:SessionTimeout>PT10S</tt:SessionTimeout></tt:AudioEncoderConfiguration><tt:VideoAnalyticsConfiguration token=\"000\"><tt:Name>Analytics_000</tt:Name><tt:UseCount>2</tt:UseCount><tt:AnalyticsEngineConfiguration><tt:AnalyticsModule Type=\"tt:CellMotionEngine\" Name=\"MyCellMotionEngine\"><tt:Parameters><tt:SimpleItem Value=\"4\" Name=\"Sensitivity\"></tt:SimpleItem><tt:ElementItem Name=\"Layout\"><tt:CellLayout Columns=\"22\" Rows=\"18\"><tt:Transformation><tt:Translate x=\"-1.0\" y=\"-1.0\" /><tt:Scale x=\"0.09090\" y=\"0.111111\" /></tt:Transformation></tt:CellLayout></tt:ElementItem></tt:Parameters></tt:AnalyticsModule><tt:AnalyticsModule Type=\"tt:TamperEngine\" Name=\"MyTamperEngine\"><tt:Parameters><tt:SimpleItem Value=\"4\" Name=\"Sensitivity\"></tt:SimpleItem><tt:ElementItem Name=\"Field\"><tt:PolygonConfiguration><tt:Polygon><tt:Point x=\"0\" y=\"0\"/><tt:Point x=\"0\" y=\"0\"/><tt:Point x=\"0\" y=\"0\"/><tt:Point x=\"0\" y=\"0\"/></tt:Polygon></tt:PolygonConfiguration></tt:ElementItem><tt:ElementItem Name=\"Transform\"><tt:Transformation><tt:Translate x=\"-1.0\" y=\"-1.0\"/><tt:Scale x=\"0.001250\" y=\"0.001667\"/></tt:Transformation></tt:ElementItem></tt:Parameters></tt:AnalyticsModule></tt:AnalyticsEngineConfiguration><tt:RuleEngineConfiguration><tt:Rule Type=\"tt:CellMotionDetector\" Name=\"MyMotionDetectorRule\"><tt:Parameters><tt:SimpleItem Value=\"zwA\" Name=\"ActiveCells\"></tt:SimpleItem><tt:SimpleItem Value=\"1000\" Name=\"AlarmOffDelay\"></tt:SimpleItem><tt:SimpleItem Value=\"1000\" Name=\"AlarmOnDelay\"></tt:SimpleItem><tt:SimpleItem Value=\"4\" Name=\"MinCount\"></tt:SimpleItem></tt:Parameters></tt:Rule><tt:Rule Type=\"tt:TamperDetector\" Name=\"MyTamperDetectorRule\"><tt:Parameters><tt:ElementItem Name=\"Field\"><tt:PolygonConfiguration><tt:Polygon><tt:Point x=\"0\" y=\"0\"/><tt:Point x=\"0\" y=\"0\"/><tt:Point x=\"0\" y=\"0\"/><tt:Point x=\"0\" y=\"0\"/></tt:Polygon></tt:PolygonConfiguration></tt:ElementItem></tt:Parameters></tt:Rule></tt:RuleEngineConfiguration></tt:VideoAnalyticsConfiguration><tt:PTZConfiguration token=\"000\"><tt:Name>PTZ_000</tt:Name><tt:UseCount>2</tt:UseCount><tt:NodeToken>000</tt:NodeToken><tt:DefaultRelativePanTiltTranslationSpace>http://www.onvif.org/ver10/tptz/PanTiltSpaces/TranslationGenericSpace</tt:DefaultRelativePanTiltTranslationSpace><tt:DefaultRelativeZoomTranslationSpace>http://www.onvif.org/ver10/tptz/ZoomSpaces/TranslationGenericSpace</tt:DefaultRelativeZoomTranslationSpace><tt:DefaultContinuousPanTiltVelocitySpace>http://www.onvif.org/ver10/tptz/PanTiltSpaces/VelocityGenericSpace</tt:DefaultContinuousPanTiltVelocitySpace><tt:DefaultContinuousZoomVelocitySpace>http://www.onvif.org/ver10/tptz/ZoomSpaces/VelocityGenericSpace</tt:DefaultContinuousZoomVelocitySpace><tt:DefaultPTZSpeed><tt:PanTilt space=\"http://www.onvif.org/ver10/tptz/PanTiltSpaces/GenericSpeedSpace\" y=\"1\" x=\"1\"></tt:PanTilt><tt:Zoom space=\"http://www.onvif.org/ver10/tptz/ZoomSpaces/ZoomGenericSpeedSpace\" x=\"1\"></tt:Zoom></tt:DefaultPTZSpeed><tt:DefaultPTZTimeout>PT1S</tt:DefaultPTZTimeout><tt:PanTiltLimits><tt:Range><tt:URI>http://www.onvif.org/ver10/tptz/PanTiltSpaces/PositionGenericSpace</tt:URI><tt:XRange><tt:Min>-1</tt:Min><tt:Max>1</tt:Max></tt:XRange><tt:YRange><tt:Min>-1</tt:Min><tt:Max>1</tt:Max></tt:YRange></tt:Range></tt:PanTiltLimits><tt:ZoomLimits><tt:Range><tt:URI>http://www.onvif.org/ver10/tptz/ZoomSpaces/PositionGenericSpace</tt:URI><tt:XRange><tt:Min>-1</tt:Min><tt:Max>1</tt:Max></tt:XRange></tt:Range></tt:ZoomLimits></tt:PTZConfiguration></trt:Profiles><trt:Profiles fixed=\"true\" token=\"002\"><tt:Name>Profile_002</tt:Name><tt:VideoSourceConfiguration token=\"000\"><tt:Name>VideoS_000</tt:Name><tt:UseCount>3</tt:UseCount><tt:SourceToken>000</tt:SourceToken><tt:Bounds height=\"1080\" width=\"1920\" y=\"0\" x=\"0\"></tt:Bounds></tt:VideoSourceConfiguration><tt:VideoEncoderConfiguration token=\"002\"><tt:Name>VideoE_002</tt:Name><tt:UseCount>1</tt:UseCount><tt:Encoding>JPEG</tt:Encoding><tt:Resolution><tt:Width>704</tt:Width><tt:Height>576</tt:Height></tt:Resolution><tt:Quality>4</tt:Quality><tt:RateControl><tt:FrameRateLimit>-3600</tt:FrameRateLimit><tt:EncodingInterval>1</tt:EncodingInterval><tt:BitrateLimit>512</tt:BitrateLimit></tt:RateControl><tt:Multicast><tt:Address><tt:Type>IPv4</tt:Type><tt:IPv4Address>224.1.2.3</tt:IPv4Address></tt:Address><tt:Port>0</tt:Port><tt:TTL>0</tt:TTL><tt:AutoStart>false</tt:AutoStart></tt:Multicast><tt:SessionTimeout>PT10S</tt:SessionTimeout></tt:VideoEncoderConfiguration></trt:Profiles></trt:GetProfilesResponse></SOAP-ENV:Body></SOAP-ENV:Envelope>";
            configure_post(
                &mut mock,
                "test_inner_get_device_profiles-url",
                &get_action(MEDIA_WSDL, "GetProfiles"),
                GET_PROFILES_TEMPLATE,
                response,
            );
        }
        let mut actual_profiles =
            inner_get_device_profiles("test_inner_get_device_profiles-url", &mock)
                .await
                .unwrap();
        actual_profiles.sort();
        assert_eq!(
            vec!["000".to_string(), "001".to_string(), "002".to_string()],
            actual_profiles,
        );
    }

    #[tokio::test]
    async fn test_inner_get_device_profile_streaming_uri() {
        let _ = env_logger::builder().is_test(true).try_init();

        let expected_result = vec![
            "rtsp://192.168.0.36:554/user=admin_password=tlJwpbo6_channel=1_stream=0.sdp?real_stream".to_string(),
            "rtsp://192.168.1.36:554/user=admin_password=tlJwpbo6_channel=1_stream=0.sdp?real_stream".to_string(),
            "rtsp://192.168.2.36:554/user=admin_password=tlJwpbo6_channel=1_stream=0.sdp?real_stream".to_string()
        ];

        for (i, expected_uri) in expected_result.iter().enumerate().take(3) {
            let mut mock = MockHttp::new();
            let profile = format!("00{}", i).to_string();
            let message = get_stream_uri_message(&profile);
            let response = format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<SOAP-ENV:Envelope xmlns:SOAP-ENV=\"http://www.w3.org/2003/05/soap-envelope\" xmlns:SOAP-ENC=\"http://www.w3.org/2003/05/soap-encoding\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xmlns:xsd=\"http://www.w3.org/2001/XMLSchema\" xmlns:xs=\"http://www.w3.org/2000/10/XMLSchema\" xmlns:wsse=\"http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-secext-1.0.xsd\" xmlns:wsa5=\"http://www.w3.org/2005/08/addressing\" xmlns:xop=\"http://www.w3.org/2004/08/xop/include\" xmlns:wsa=\"http://schemas.xmlsoap.org/ws/2004/08/addressing\" xmlns:tt=\"http://www.onvif.org/ver10/schema\" xmlns:ns1=\"http://www.w3.org/2005/05/xmlmime\" xmlns:wstop=\"http://docs.oasis-open.org/wsn/t-1\" xmlns:ns7=\"http://docs.oasis-open.org/wsrf/r-2\" xmlns:ns2=\"http://docs.oasis-open.org/wsrf/bf-2\" xmlns:dndl=\"http://www.onvif.org/ver10/network/wsdl/DiscoveryLookupBinding\" xmlns:dnrd=\"http://www.onvif.org/ver10/network/wsdl/RemoteDiscoveryBinding\" xmlns:d=\"http://schemas.xmlsoap.org/ws/2005/04/discovery\" xmlns:dn=\"http://www.onvif.org/ver10/network/wsdl\" xmlns:ns10=\"http://www.onvif.org/ver10/replay/wsdl\" xmlns:ns11=\"http://www.onvif.org/ver10/search/wsdl\" xmlns:ns13=\"http://www.onvif.org/ver20/analytics/wsdl/RuleEngineBinding\" xmlns:ns14=\"http://www.onvif.org/ver20/analytics/wsdl/AnalyticsEngineBinding\" xmlns:tan=\"http://www.onvif.org/ver20/analytics/wsdl\" xmlns:ns15=\"http://www.onvif.org/ver10/events/wsdl/PullPointSubscriptionBinding\" xmlns:ns16=\"http://www.onvif.org/ver10/events/wsdl/EventBinding\" xmlns:tev=\"http://www.onvif.org/ver10/events/wsdl\" xmlns:ns17=\"http://www.onvif.org/ver10/events/wsdl/SubscriptionManagerBinding\" xmlns:ns18=\"http://www.onvif.org/ver10/events/wsdl/NotificationProducerBinding\" xmlns:ns19=\"http://www.onvif.org/ver10/events/wsdl/NotificationConsumerBinding\" xmlns:ns20=\"http://www.onvif.org/ver10/events/wsdl/PullPointBinding\" xmlns:ns21=\"http://www.onvif.org/ver10/events/wsdl/CreatePullPointBinding\" xmlns:ns22=\"http://www.onvif.org/ver10/events/wsdl/PausableSubscriptionManagerBinding\" xmlns:wsnt=\"http://docs.oasis-open.org/wsn/b-2\" xmlns:ns3=\"http://www.onvif.org/ver10/analyticsdevice/wsdl\" xmlns:ns4=\"http://www.onvif.org/ver10/deviceIO/wsdl\" xmlns:ns5=\"http://www.onvif.org/ver10/display/wsdl\" xmlns:ns8=\"http://www.onvif.org/ver10/receiver/wsdl\" xmlns:ns9=\"http://www.onvif.org/ver10/recording/wsdl\" xmlns:tds=\"http://www.onvif.org/ver10/device/wsdl\" xmlns:timg=\"http://www.onvif.org/ver20/imaging/wsdl\" xmlns:tptz=\"http://www.onvif.org/ver20/ptz/wsdl\" xmlns:trt=\"http://www.onvif.org/ver10/media/wsdl\" xmlns:trt2=\"http://www.onvif.org/ver20/media/wsdl\" xmlns:ter=\"http://www.onvif.org/ver10/error\" xmlns:tns1=\"http://www.onvif.org/ver10/topics\" xmlns:tnsn=\"http://www.eventextension.com/2011/event/topics\"><SOAP-ENV:Header></SOAP-ENV:Header><SOAP-ENV:Body><trt:GetStreamUriResponse><trt:MediaUri><tt:Uri>rtsp://192.168.{}.36:554/user=admin_password=tlJwpbo6_channel=1_stream=0.sdp?real_stream</tt:Uri><tt:InvalidAfterConnect>false</tt:InvalidAfterConnect><tt:InvalidAfterReboot>false</tt:InvalidAfterReboot><tt:Timeout>PT10S</tt:Timeout></trt:MediaUri></trt:GetStreamUriResponse></SOAP-ENV:Body></SOAP-ENV:Envelope>",
                i
            );
            configure_post(
                &mut mock,
                "test_inner_get_device_profile_streaming_uri-url",
                &get_action(MEDIA_WSDL, "GetStreamUri"),
                &message,
                &response.to_string(),
            );

            assert_eq!(
                expected_uri.to_string(),
                inner_get_device_profile_streaming_uri(
                    "test_inner_get_device_profile_streaming_uri-url",
                    &profile,
                    &mock
                )
                .await
                .unwrap()
            );
        }
    }

    #[tokio::test]
    async fn test_inner_is_device_responding() {
        let _ = env_logger::builder().is_test(true).try_init();
        let mut mock = MockHttp::new();
        // empty response since do not check contents only successful response
        let empty_response = "<tds:GetSystemDateAndTimeResponse xmlns:tt=\"http://www.onvif.org/ver10/schema\" xmlns:tds=\"http://www.onvif.org/ver10/device/wsdl\"></tds:GetSystemDateAndTimeResponse>";
        let response = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<SOAP-ENV:Envelope xmlns:SOAP-ENV=\"http://www.w3.org/2003/05/soap-envelope\" xmlns:SOAP-ENC=\"http://www.w3.org/2003/05/soap-encoding\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xmlns:xsd=\"http://www.w3.org/2001/XMLSchema\" xmlns:xs=\"http://www.w3.org/2000/10/XMLSchema\" xmlns:wsse=\"http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-secext-1.0.xsd\" xmlns:wsa5=\"http://www.w3.org/2005/08/addressing\" xmlns:xop=\"http://www.w3.org/2004/08/xop/include\" xmlns:wsa=\"http://schemas.xmlsoap.org/ws/2004/08/addressing\" xmlns:tt=\"http://www.onvif.org/ver10/schema\" xmlns:ns1=\"http://www.w3.org/2005/05/xmlmime\" xmlns:wstop=\"http://docs.oasis-open.org/wsn/t-1\" xmlns:ns7=\"http://docs.oasis-open.org/wsrf/r-2\" xmlns:ns2=\"http://docs.oasis-open.org/wsrf/bf-2\" xmlns:dndl=\"http://www.onvif.org/ver10/network/wsdl/DiscoveryLookupBinding\" xmlns:dnrd=\"http://www.onvif.org/ver10/network/wsdl/RemoteDiscoveryBinding\" xmlns:d=\"http://schemas.xmlsoap.org/ws/2005/04/discovery\" xmlns:dn=\"http://www.onvif.org/ver10/network/wsdl\" xmlns:ns10=\"http://www.onvif.org/ver10/replay/wsdl\" xmlns:ns11=\"http://www.onvif.org/ver10/search/wsdl\" xmlns:ns13=\"http://www.onvif.org/ver20/analytics/wsdl/RuleEngineBinding\" xmlns:ns14=\"http://www.onvif.org/ver20/analytics/wsdl/AnalyticsEngineBinding\" xmlns:tan=\"http://www.onvif.org/ver20/analytics/wsdl\" xmlns:ns15=\"http://www.onvif.org/ver10/events/wsdl/PullPointSubscriptionBinding\" xmlns:ns16=\"http://www.onvif.org/ver10/events/wsdl/EventBinding\" xmlns:tev=\"http://www.onvif.org/ver10/events/wsdl\" xmlns:ns17=\"http://www.onvif.org/ver10/events/wsdl/SubscriptionManagerBinding\" xmlns:ns18=\"http://www.onvif.org/ver10/events/wsdl/NotificationProducerBinding\" xmlns:ns19=\"http://www.onvif.org/ver10/events/wsdl/NotificationConsumerBinding\" xmlns:ns20=\"http://www.onvif.org/ver10/events/wsdl/PullPointBinding\" xmlns:ns21=\"http://www.onvif.org/ver10/events/wsdl/CreatePullPointBinding\" xmlns:ns22=\"http://www.onvif.org/ver10/events/wsdl/PausableSubscriptionManagerBinding\" xmlns:wsnt=\"http://docs.oasis-open.org/wsn/b-2\" xmlns:ns3=\"http://www.onvif.org/ver10/analyticsdevice/wsdl\" xmlns:ns4=\"http://www.onvif.org/ver10/deviceIO/wsdl\" xmlns:ns5=\"http://www.onvif.org/ver10/display/wsdl\" xmlns:ns8=\"http://www.onvif.org/ver10/receiver/wsdl\" xmlns:ns9=\"http://www.onvif.org/ver10/recording/wsdl\" xmlns:tds=\"http://www.onvif.org/ver10/device/wsdl\" xmlns:timg=\"http://www.onvif.org/ver20/imaging/wsdl\" xmlns:tptz=\"http://www.onvif.org/ver20/ptz/wsdl\" xmlns:trt=\"http://www.onvif.org/ver10/media/wsdl\" xmlns:trt2=\"http://www.onvif.org/ver20/media/wsdl\" xmlns:ter=\"http://www.onvif.org/ver10/error\" xmlns:tns1=\"http://www.onvif.org/ver10/topics\" xmlns:tnsn=\"http://www.eventextension.com/2011/event/topics\"><SOAP-ENV:Header></SOAP-ENV:Header><SOAP-ENV:Body>{}</SOAP-ENV:Body></SOAP-ENV:Envelope>", empty_response);
        let url = "test_inner_is_device_responding-url".to_string();
        configure_post(
            &mut mock,
            &url,
            &get_action(DEVICE_WSDL, "GetSystemDateAndTime"),
            GET_SYSTEM_DATE_AND_TIME_TEMPLATE,
            &response,
        );
        assert_eq!(
            inner_is_device_responding("test_inner_is_device_responding-url", &mock)
                .await
                .unwrap(),
            url
        );
    }

    #[test]
    fn test_http_handle_request_body_no_panic() {
        assert!(HttpRequest::handle_request_body("\r\n").is_err());
    }
}
