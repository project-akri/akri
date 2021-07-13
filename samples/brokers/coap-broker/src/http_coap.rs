use coap_lite::{ContentFormat, MessageClass, Packet};
use warp::hyper::{Response, StatusCode};

pub fn coap_to_http(packet: Packet) -> Response<Vec<u8>> {
    let coap_status_code = packet.header.code;
    let content_format = match packet.get_content_format() {
        Some(c) => c,
        None => ContentFormat::ApplicationOctetStream,
    };
    let content_type = content_format_to_content_type(content_format);

    let http_status_code = coap_code_to_http_code(coap_status_code);
    let http_status = StatusCode::from_u16(http_status_code).unwrap();

    // TODO: Convert and copy over headers from CoAP to HTTP
    Response::builder()
        .status(http_status)
        .header("Content-Type", content_type)
        .body(packet.payload)
        .unwrap()
}

/// Converts a CoAP status code to HTTP status code. The CoAP status code field is described in
/// RFC 7252 Section 3.
///
/// Put simply, a CoAP status code is 8 bits, where the first 3 bits indicate the class and the
/// remaining 5 bits the type. For instance a status code 0x84 is 0b100_01000, which is 4_04 aka
/// NotFound in HTTP :)
fn coap_code_to_http_code(coap_code: MessageClass) -> u16 {
    let binary_code = u8::from(coap_code);
    let class = binary_code >> 5;
    let class_type = binary_code & 0b00011111;

    let http_code = (class as u16) * 100 + (class_type as u16);

    http_code
}

fn content_format_to_content_type(content_format: ContentFormat) -> String {
    match content_format {
        ContentFormat::TextPlain => "text/plain".to_string(),
        ContentFormat::ApplicationLinkFormat => "application/link-format".to_string(),
        ContentFormat::ApplicationXML => "application/xml".to_string(),
        ContentFormat::ApplicationOctetStream => "application/octet-stream".to_string(),
        ContentFormat::ApplicationEXI => "application/exi".to_string(),
        ContentFormat::ApplicationJSON => "application/json".to_string(),
        ContentFormat::ApplicationCBOR => "application/cbor".to_string(),
        ContentFormat::ApplicationSenmlJSON => "application/senml+json".to_string(),
        ContentFormat::ApplicationSensmlJSON => "application/sensml+json".to_string(),
        ContentFormat::ApplicationSenmlCBOR => "application/senml+cbor".to_string(),
        ContentFormat::ApplicationSensmlCBOR => "application/sensml+cbor".to_string(),
        ContentFormat::ApplicationSenmlExi => "application/senml+exi".to_string(),
        ContentFormat::ApplicationSensmlExi => "application/sensml+exi".to_string(),
        ContentFormat::ApplicationSenmlXML => "application/senml+xml".to_string(),
        ContentFormat::ApplicationSensmlXML => "application/sensml+xml".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use coap_lite::{MessageClass, ResponseType};

    #[test]
    fn test_status_code_conversion() {
        let coap_status = MessageClass::Response(ResponseType::NotFound);
        let http_status = coap_code_to_http_code(coap_status);

        assert_eq!(http_status, 404);
    }
}
