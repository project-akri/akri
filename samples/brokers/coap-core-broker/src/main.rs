use akri_shared::os::env_var::{ActualEnvVarQuery, EnvVarQuery};
use coap::CoAPClient;
use coap_lite::{MessageClass, ResponseType};
use futures::{FutureExt, SinkExt, StreamExt};
use log::{debug, info};
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_stream::wrappers::UnboundedReceiverStream;
use warp::hyper::{Response, StatusCode};
use warp::path::FullPath;
use warp::ws::Message;
use warp::{Filter, Reply};

static CLIENT_COUNTER: AtomicU16 = AtomicU16::new(0);

fn get_client_id() -> u16 {
    CLIENT_COUNTER.fetch_add(1, Ordering::Relaxed)
}

pub const COAP_RESOURCE_TYPES_LABEL_ID: &str = "COAP_RESOURCE_TYPES";
pub const COAP_IP_LABEL_ID: &str = "COAP_IP";

async fn handle_health() -> Result<impl Reply, Infallible> {
    Ok(String::from("Healthy"))
}

async fn handle_proxy(req: FullPath, state: Arc<AppState>) -> Result<impl Reply, Infallible> {
    let path = req.as_str();
    let ip_address = state.ip_address.clone();
    let resource_uris = state.resource_uris.clone();
    let endpoint = format!("coap://{}:5683{}", ip_address, path);
    info!("Proxing request to {}", endpoint);

    if !resource_uris.contains(&path.to_string()) {
        let response = Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(vec![])
            .unwrap();

        return Ok(response);
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
            let proxy_res = Response::builder()
                .status(http_status)
                .body(response.message.payload)
                .unwrap();

            // TODO: Convert and copy over headers from CoAP to HTTP

            Ok(proxy_res)
        }
        Err(e) => {
            info!("Error while trying to request the device {}", e);

            let cache = state.cache.lock().unwrap();
            let cached_value = cache.get(&path.to_string());

            match cached_value {
                Some(payload) => {
                    debug!("Found response in the cache");
                    let response = Response::builder().body(payload.clone()).unwrap();

                    Ok(response)
                }
                None => {
                    let response = Response::builder()
                        .status(StatusCode::SERVICE_UNAVAILABLE)
                        .body(e.to_string().into_bytes())
                        .unwrap();

                    Ok(response)
                }
            }
        }
    }
}

async fn handle_stream(
    req: FullPath,
    state: Arc<AppState>,
    websocket: warp::ws::WebSocket,
) -> anyhow::Result<()> {
    let path = req.as_str().to_string();
    let ip_address = state.ip_address.clone();
    let addr = format!("{}:5683", ip_address);
    let resource_uris = state.resource_uris.clone();
    info!("Streaming from {}", addr);

    let mut client = CoAPClient::new(addr)
        .map_err(|e| anyhow::anyhow!("Invalid client address while creating the client: {}", e))?;
    let client_id = get_client_id();

    let (mut socket_tx, mut socket_rx) = websocket.split();
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let rx_stream = UnboundedReceiverStream::new(rx);

    if !resource_uris.contains(&path) {
        let message = Message::close_with(StatusCode::NOT_FOUND, "Not found");
        socket_tx
            .send(message)
            .await
            .map_err(|e| anyhow::anyhow!("Error sending the close message: {}", e))?;
        socket_tx
            .close()
            .await
            .map_err(|e| anyhow::anyhow!("Error closing the websocket connection: {}", e))?;
        return Ok(());
    }

    tokio::task::spawn(rx_stream.forward(socket_tx).map(|result| {
        if let Err(e) = result {
            log::error!("websocket send error: {}", e);
        }
    }));

    let tx_clone = tx.clone();
    tokio::task::spawn(async move {
        while let Some(message) = socket_rx.next().await {
            match message {
                Ok(msg) => {
                    if msg.is_close() {
                        tx_clone.send(Ok(Message::close())).unwrap();
                    }
                }
                Err(e) => {
                    log::error!("Websocket received an error: {}", e);
                    break;
                }
            }
        }
    });

    let observe_state = state.clone();

    client
        .observe_with_timeout(
            path.as_str(),
            move |packet| {
                let message = Message::binary(packet.payload);

                match tx.send(Ok(message)) {
                    Ok(()) => {}
                    Err(e) => {
                        log::error!("Error sending the device message: {}", e);

                        let state_clone = observe_state.clone();

                        std::thread::spawn(move || {
                            let mut clients = state_clone.clients.lock().unwrap();
                            clients.remove(&client_id);
                        });
                    }
                }
            },
            Duration::from_secs(5),
        )
        .map_err(|e| anyhow::anyhow!("Error observing the request: {}", e))?;

    let mut clients = state.clients.lock().unwrap();
    clients.insert(client_id, client);

    Ok(())
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
    resource_uris: Vec<String>,
    cache: Mutex<HashMap<String, Vec<u8>>>,
    clients: Mutex<HashMap<u16, CoAPClient>>,
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let env_var_query = ActualEnvVarQuery {};
    let device_ip = get_device_ip(&env_var_query);
    let resource_types = get_resources_types(&env_var_query);
    let resource_uris: Vec<String> = resource_types
        .iter()
        .map(|rtype| get_resource_uri(&env_var_query, rtype))
        .collect();

    info!(
        "Found device IP {} with resource types {:?}",
        device_ip, resource_types
    );

    let state = Arc::new(AppState {
        ip_address: device_ip,
        resource_uris,
        cache: Mutex::new(HashMap::new()),
        clients: Mutex::new(HashMap::new()),
    });

    let health = warp::get()
        .and(warp::path("healthz"))
        .and_then(handle_health);
    let proxy = warp::get()
        .and(warp::path::full())
        .and(with_state(state.clone()))
        .and_then(handle_proxy);
    let stream = warp::get()
        .and(warp::path::full())
        .and(with_state(state.clone()))
        .and(warp::ws())
        .map(|path: FullPath, state: Arc<AppState>, ws: warp::ws::Ws| {
            ws.on_upgrade(move |websocket| async {
                match handle_stream(path, state, websocket).await {
                    Ok(()) => {}
                    Err(e) => {
                        log::error!("Error handling the websocket stream: {}", e);
                    }
                }
            })
        });

    let routes = health.or(stream).or(proxy).with(warp::log("api"));

    warp::serve(routes).run(([0, 0, 0, 0], 8083)).await;
}

fn with_state(
    state: Arc<AppState>,
) -> impl warp::Filter<Extract = (Arc<AppState>,), Error = Infallible> + Clone {
    warp::any().map(move || state.clone())
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
    let value = env_var_query.get_env_var(resource_type).expect(
        format!(
            "Device resource URI for type {} not set in environment variable",
            resource_type
        )
        .as_str(),
    );

    value
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
