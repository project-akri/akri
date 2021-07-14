mod http_coap;

use akri_shared::os::env_var::{ActualEnvVarQuery, EnvVarQuery};
use coap::CoAPClient;
use coap_lite::{ContentFormat, MessageClass, Packet, ResponseType};
use futures::{FutureExt, SinkExt, StreamExt};
use http_coap::coap_to_http;
use log::{debug, error, info};
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
    info!("handle_proxy - proxing request to {}", endpoint);

    if !resource_uris.contains(&path.to_string()) {
        let response = Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(vec![])
            .unwrap();

        return Ok(response);
    }

    // TODO: should some HTTP headers copied to the CoAP request? E.g. 'Forwarded'
    let response = CoAPClient::get_with_timeout(endpoint.as_str(), Duration::from_secs(5));

    match response {
        Ok(response) => {
            let coap_status_code = response.message.header.code.clone();
            let proxy_res = coap_to_http(response.message.clone());

            // Save the response to the cache only if the response is 205 Content
            // See RFC 7252 Ch. 5.9 for cachable responses.
            if coap_status_code == MessageClass::Response(ResponseType::Content) {
                debug!("Saving response of {} to cache", path);

                let mut cache = state.cache.lock().unwrap();
                cache.insert(path.to_string(), response.message);
            }

            Ok(proxy_res)
        }
        Err(e) => {
            info!(
                "handle_proxy - error while trying to send the request to the device {}",
                e
            );

            let cache = state.cache.lock().unwrap();
            let cached_packet = cache.get(&path.to_string());

            match cached_packet {
                Some(packet) => {
                    debug!("Found response in the cache");
                    let response = coap_to_http(packet.clone());

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
    info!("handle_stream - streaming from {}", addr);

    let mut client = CoAPClient::new(addr)
        .map_err(|e| anyhow::anyhow!("Invalid client address while creating the client: {}", e))?;
    let client_id = get_client_id();

    let (mut ws_tx, mut ws_rx) = websocket.split();
    let (coap_tx, coap_rx) = tokio::sync::mpsc::unbounded_channel();
    let coap_rx_stream = UnboundedReceiverStream::new(coap_rx);

    if !resource_uris.contains(&path) {
        let message = Message::close_with(StatusCode::NOT_FOUND, "Not found");
        ws_tx
            .send(message)
            .await
            .map_err(|e| anyhow::anyhow!("Error sending the close message: {}", e))?;
        ws_tx
            .close()
            .await
            .map_err(|e| anyhow::anyhow!("Error closing the websocket connection: {}", e))?;
        return Ok(());
    }

    tokio::task::spawn(coap_rx_stream.forward(ws_tx).map(|result| {
        if let Err(e) = result {
            error!("handle_stream - websocket send error: {}", e);
        }
    }));

    let state_clone = state.clone();
    let coap_tx_clone = coap_tx.clone();

    tokio::task::spawn(async move {
        while let Some(message) = ws_rx.next().await {
            match message {
                Ok(msg) => {
                    if msg.is_close() {
                        coap_tx_clone.send(Ok(Message::close())).unwrap();

                        unobserve(state_clone.clone(), client_id);
                    }
                }
                Err(e) => {
                    error!("handle_stream - websocket received an error: {}", e);
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
                let content_format = match packet.get_content_format() {
                    Some(c) => c,
                    None => ContentFormat::ApplicationOctetStream,
                };
                let message = match content_format {
                    ContentFormat::TextPlain | ContentFormat::ApplicationJSON => {
                        let content = String::from_utf8_lossy(&packet.payload[..]);

                        Message::text(content)
                    }
                    _ => Message::binary(packet.payload),
                };

                match coap_tx.send(Ok(message)) {
                    Ok(()) => {}
                    Err(e) => {
                        error!("handle_stream - error sending the CoAP message: {}", e);
                        unobserve(observe_state.clone(), client_id);
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

fn unobserve(state: Arc<AppState>, client_id: u16) {
    // Unsubscription must be done in a new OS thread because it internally joins on the thread handle,
    // which would throw if `unsubscribe` is called within the `observe_with_timeout` closure
    std::thread::spawn(move || {
        let mut clients = state.clients.lock().unwrap();
        let client = clients.remove(&client_id);

        if let Some(mut client) = client {
            client.unobserve();
        }
    });
}

struct AppState {
    ip_address: String,
    resource_uris: Vec<String>,
    // The CoAP packet is saved instead of the HTTP response because the latter doesn't implement
    // Clone and cannot thus returned multiple times
    cache: Mutex<HashMap<String, Packet>>,
    clients: Mutex<HashMap<u16, CoAPClient>>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let env_var_query = ActualEnvVarQuery {};
    let device_ip = get_device_ip(&env_var_query);
    let resource_types = get_resources_types(&env_var_query);
    let resource_uris: Vec<String> = resource_types.and_then(|resource_types| {
        info!(
            "main - found device IP {} with resource types {:?}",
            device_ip, resource_types
        );

        let uris: anyhow::Result<Vec<String>> = resource_types
            .iter()
            .map(|rtype| get_resource_uri(&env_var_query, rtype))
            .collect();

        uris
    })?;

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
                        error!("main - error handling the websocket stream: {}", e);
                    }
                }
            })
        });

    let routes = health.or(stream).or(proxy).with(warp::log("api"));

    warp::serve(routes).run(([0, 0, 0, 0], 8083)).await;

    Ok(())
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

fn get_resources_types(env_var_query: &impl EnvVarQuery) -> anyhow::Result<Vec<String>> {
    let types_string: anyhow::Result<String> = env_var_query
        .get_env_var(COAP_RESOURCE_TYPES_LABEL_ID)
        .map_err(|_e| anyhow::anyhow!("Device resource types not set in environment variable"));
    let resource_types: anyhow::Result<Vec<String>> =
        types_string.map(|types_string| types_string.split(',').map(|s| s.to_string()).collect());

    resource_types
}

fn get_resource_uri(
    env_var_query: &impl EnvVarQuery,
    resource_type: &str,
) -> anyhow::Result<String> {
    env_var_query.get_env_var(resource_type).map_err(|_e| {
        anyhow::anyhow!(
            "Device resource URI for type {} not set in environment variable",
            resource_type
        )
    })
}
