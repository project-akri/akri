use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;
use actix_web::http::{Method, StatusCode};
use log::{info, debug};
use akri_shared::os::env_var::{EnvVarQuery, ActualEnvVarQuery};
use actix_web::{App, HttpRequest, HttpResponse, HttpServer, Responder, web};
use actix_web::middleware::Logger;
use coap::{CoAPClient};
use coap_lite::{MessageClass, ResponseType};

pub const COAP_RESOURCE_TYPES_LABEL_ID: &str = "COAP_RESOURCE_TYPES";
pub const COAP_IP_LABEL_ID: &str = "COAP_IP";

async fn health() -> impl Responder {
    HttpResponse::Ok().body("Healthy")
}

async fn proxy(req: HttpRequest, state: web::Data<AppState>) -> impl Responder {
    let path = req.path();
    let ip_address = state.ip_address.clone();
    let endpoint = format!("coap://{}:5683{}", ip_address, path);
    info!("Proxing request to {}", endpoint);

    if req.method() != &Method::GET {
        return HttpResponse::NotImplemented().body("Only GET requests are supported for now");
    }
    
    // TODO: should some HTTP headers to set or copied to the CoAP request? E.g. 'Forwarded'
    let response = CoAPClient::get_with_timeout(endpoint.as_str(), Duration::from_secs(5));

    match response {
        Ok(response) => {
            let coap_status_code = response.message.header.code;

            // Save the response to the cache only if the response is 205 Content
            // See RFC 7252 Ch. 5.9 for cachable responses.
            if coap_status_code == MessageClass::Response(ResponseType::Content) {
                debug!("Saving response of {} to cache", path.clone());

                let mut cache = state.cache.lock().unwrap();
                cache.insert(path.to_string(), response.message.payload.clone());
            }

            // Convert the response to HTTP
            let http_status_code = coap_code_to_http_code(coap_status_code);
            let http_status = StatusCode::from_u16(http_status_code).unwrap();
            let mut proxy_res = HttpResponse::build(http_status);

            // TODO: Convert and copy over headers from CoAP to HTTP

            proxy_res.body(response.message.payload)
        },
        Err(e) => {
            info!("Error while trying to request the device {}", e);

            let cache = state.cache.lock().unwrap();
            let cached_value = cache.get(&path.to_string());

            match cached_value {
                Some(payload) => {
                    debug!("Found response in the cache");

                    HttpResponse::Ok().body(payload.clone())
                },
                None => {
                    HttpResponse::ServiceUnavailable().body(e.to_string())
                }
            }
        }
    }
}

/// Converts a CoAP status code to HTTP status code. The CoAP status code field is described in
/// RFC 7252 Section 3. 
///
/// Put simply, a CoAP status code is 8bits, where the first 3 bits indicate the class and the 
/// remaining 5 bits the type. For instance a status code 0x84 is 0b100_01000, which is 4_04 aka
/// NotFound in HTTP :)
fn coap_code_to_http_code(coap_code: MessageClass) -> u16 {
    let binary_code = u8::from(coap_code);
    let class = binary_code >> 5;
    let class_type = binary_code & 0b00011111;

    let http_code = (class as u16) * 100 + (class_type as u16);

    http_code
}

struct AppState {
    ip_address: String,
    cache: Mutex<HashMap<String, Vec<u8>>>
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();

    let env_var_query = ActualEnvVarQuery {};
    let device_ip = get_device_ip(&env_var_query);
    let resource_types = get_resources_types(&env_var_query);
    let resource_uris: Vec<String> = resource_types.iter().map(|rtype| get_resource_uri(&env_var_query, rtype)).collect();
    
    info!("Found device IP {} with resource types {:?}", device_ip, resource_types);

    let state = web::Data::new(AppState {
        ip_address: device_ip,
        cache: Mutex::new(HashMap::new())
    });

    HttpServer::new(move || {
        let mut app = App::new()
            .wrap(Logger::default())
            .app_data(state.clone())
            .service(web::resource("/healthz").route(web::get().to(health)));

        for uri in resource_uris.iter() {
            app = app.service(web::resource(uri.as_str()).route(web::get().to(proxy)));
        }

        app
    })
        .bind("0.0.0.0:8083")?
        .run()
        .await
}

fn get_device_ip(env_var_query: &impl EnvVarQuery) -> String {
    let ip_address = env_var_query
        .get_env_var(COAP_IP_LABEL_ID)
        .expect("Device IP address not set in environment variable");

    ip_address
}

fn get_resources_types(env_var_query: &impl EnvVarQuery) -> Vec<String> {
    let types_string: String = env_var_query
        .get_env_var(COAP_RESOURCE_TYPES_LABEL_ID)
        .expect("Device resource types not set in environment variable");
    let resource_types: Vec<String> = types_string.split(",").map(|s| s.to_string()).collect();

    resource_types
}

fn get_resource_uri(env_var_query: &impl EnvVarQuery, resource_type: &str) -> String {
    let value = env_var_query
        .get_env_var(resource_type);

    match value {
        Ok(uri) => {
            info!("Found resource URI {} for resource type {}", uri, resource_type);

            uri
        },
        Err(e) => {
            panic!("Device resource URI for type {} not set in environment variable: {}", resource_type, e);
        }
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
