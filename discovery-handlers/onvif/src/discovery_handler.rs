
use super::discovery_impl::util;
use akri_shared::onvif::device_info::{
    OnvifQuery, OnvifQueryImpl, ONVIF_DEVICE_IP_ADDRESS_LABEL_ID,
    ONVIF_DEVICE_MAC_ADDRESS_LABEL_ID, ONVIF_DEVICE_SERVICE_URL_LABEL_ID,
};
use akri_discovery_utils::discovery::v0::{Device, DiscoverResponse, DiscoverRequest, discovery_server::{Discovery, DiscoveryServer}};
use akri_shared::akri::configuration::{FilterList, FilterType};
use anyhow::Error;
use async_trait::async_trait;
use std::{collections::HashMap, fs};
use tokio::sync::mpsc;
use tokio::time::delay_for;
use log::{error, info, trace};
use std::time::Duration;
use tonic::{transport::Server, Response, Status};

/// Protocol name that onvif discovery handlers use when registering with the Agent
pub const PROTOCOL_NAME: &str = "onvif";
pub const DISCOVERY_ENDPOINT: &str = "[::1]:10002";
// TODO: make this configurable
pub const DISCOVERY_INTERVAL_SECS: u64 = 10;
pub type DiscoverStream = mpsc::Receiver<Result<DiscoverResponse, Status>>;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub enum DiscoveryHandlerType {
    Onvif(OnvifDiscoveryHandlerConfig),
}

/// This defines the ONVIF data stored in the Configuration
/// CRD
///
/// The ONVIF discovery handler is structured to store a filter list for
/// ip addresses, mac addresses, and ONVIF scopes.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct OnvifDiscoveryHandlerConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ip_addresses: Option<FilterList>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mac_addresses: Option<FilterList>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scopes: Option<FilterList>,
    #[serde(default = "default_discovery_timeout_seconds")]
    pub discovery_timeout_seconds: i32,
}

fn default_discovery_timeout_seconds() -> i32 {
    1
}

/// `OnvifDiscoveryHandler` discovers the onvif instances as described by the filters `discover_handler_config.ip_addresses`,
/// `discover_handler_config.mac_addresses`, and `discover_handler_config.scopes`.
/// The instances it discovers are always shared.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct OnvifDiscoveryHandler {
}

impl OnvifDiscoveryHandler {
    pub fn new() -> Self {
        OnvifDiscoveryHandler {
        }
    }
}

#[async_trait]
impl Discovery for OnvifDiscoveryHandler {
    type DiscoverStream = DiscoverStream;
    async fn discover(&self, request: tonic::Request<DiscoverRequest>) -> Result<Response<Self::DiscoverStream>, Status> {
        info!("discover - called for ONVIF protocol");
        let discover_request = request.get_ref();
        let (mut tx, rx) = mpsc::channel(4);
        let discovery_handler_config = get_configuration(&discover_request.discovery_details).map_err(|e| {
            tonic::Status::new(
                tonic::Code::InvalidArgument,
                format!("Invalid ONVIF discovery handler configuration: {}", e),
            )
        })?;
        let mut cameras: Vec<Device> = Vec::new();
        tokio::spawn(async move {
            loop {
                let onvif_query = OnvifQueryImpl {};

                info!("discover - filters:{:?}", &discovery_handler_config,);
                let discovered_onvif_cameras = util::simple_onvif_discover(Duration::from_secs(
                    discovery_handler_config.discovery_timeout_seconds as u64,
                ))
                .await.unwrap();
                info!("discover - discovered:{:?}", &discovered_onvif_cameras,);
                // apply_filters never returns an error -- safe to unwrap
                let filtered_onvif_cameras = apply_filters(&discovery_handler_config, discovered_onvif_cameras, &onvif_query)
                    .await.unwrap();
                info!("discover - filtered:{:?}", &filtered_onvif_cameras);
                let mut changed_camera_list = false;
                let mut matching_camera_count = 0;
                filtered_onvif_cameras.iter().for_each(|camera| 
                    if !cameras.contains(camera) {
                        changed_camera_list = true;
                    } else {
                        matching_camera_count += 1;
                    }
                );
                if changed_camera_list || matching_camera_count != cameras.len() {
                    info!("discover - sending updated device list");
                    cameras = filtered_onvif_cameras.clone();
                    if let Err(e) = tx.send(Ok(DiscoverResponse{ devices: filtered_onvif_cameras })).await {
                        error!("discover - for ONVIF failed to send discovery response with error {}", e);
                        break;
                    }
                }
                delay_for(Duration::from_secs(DISCOVERY_INTERVAL_SECS)).await;
            }
        });
        Ok(Response::new(rx))
    }
}

fn execute_filter(filter_list: Option<&FilterList>, filter_against: &[String]) -> bool {
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

async fn apply_filters(
    discovery_handler_config: &OnvifDiscoveryHandlerConfig,
    device_service_uris: Vec<String>,
    onvif_query: &impl OnvifQuery,
) -> Result<Vec<Device>, anyhow::Error> {
    let mut result = Vec::new();
    for device_service_url in device_service_uris.iter() {
        trace!("apply_filters - device service url {}", &device_service_url);
        let (ip_address, mac_address) = match onvif_query
            .get_device_ip_and_mac_address(&device_service_url)
            .await
        {
            Ok(ip_and_mac) => ip_and_mac,
            Err(e) => {
                error!("apply_filters - error getting ip and mac address: {}", e);
                continue;
            }
        };

        // Evaluate camera ip address against ip filter if provided
        let ip_address_as_vec = vec![ip_address.clone()];
        if execute_filter(
            discovery_handler_config.ip_addresses.as_ref(),
            &ip_address_as_vec,
        ) {
            continue;
        }

        // Evaluate camera mac address against mac filter if provided
        let mac_address_as_vec = vec![mac_address.clone()];
        if execute_filter(
            discovery_handler_config.mac_addresses.as_ref(),
            &mac_address_as_vec,
        ) {
            continue;
        }

        let ip_and_mac_joined = format!("{}-{}", &ip_address, &mac_address);

        // Evaluate camera scopes against scopes filter if provided
        let device_scopes = match onvif_query.get_device_scopes(&device_service_url).await {
            Ok(scopes) => scopes,
            Err(e) => {
                error!("apply_filters - error getting scopes: {}", e);
                continue;
            }
        };
        if execute_filter(
            discovery_handler_config.scopes.as_ref(),
            &device_scopes,
        ) {
            continue;
        }

        let mut properties = HashMap::new();
        properties.insert(
            ONVIF_DEVICE_SERVICE_URL_LABEL_ID.to_string(),
            device_service_url.to_string(),
        );
        properties.insert(ONVIF_DEVICE_IP_ADDRESS_LABEL_ID.into(), ip_address);
        properties.insert(ONVIF_DEVICE_MAC_ADDRESS_LABEL_ID.into(), mac_address);

        trace!(
            "apply_filters - returns DiscoveryResult ip/mac: {:?}, props: {:?}",
            &ip_and_mac_joined,
            &properties
        );
        result.push(Device{
            id: ip_and_mac_joined,
            properties,
            mounts: Vec::default(),
            device_specs: Vec::default(),
        })
    }
    Ok(result)
}

fn get_configuration(
    discovery_details: &HashMap<String, String>,
)  -> Result<OnvifDiscoveryHandlerConfig, Error>{
    info!("inner_get_discovery_handler - for discovery details {:?}", discovery_details);
    // Determine whether it is an embedded protocol
    if let Some(discovery_handler_str) = discovery_details.get("protocolHandler") {
        info!("protocol handler {:?}",discovery_handler_str);
        if let Ok(discovery_handler) = serde_yaml::from_str(discovery_handler_str) {
            match discovery_handler {
                DiscoveryHandlerType::Onvif(onvif_discovery_handler_config) => Ok(onvif_discovery_handler_config),
                _ => Err(anyhow::format_err!("No protocol configured")),
            }
        } else {
            Err(anyhow::format_err!("Discovery details had protocol handler but does not have embedded support. Discovery details: {:?}", discovery_details))
        }
    } else {
        Err(anyhow::format_err!("Generic discovery handlers not supported. Discovery details: {:?}", discovery_details))
    }
}

pub async fn run_debug_echo_server(
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    info!("run_debug_echo_server - entered");
    let discovery_handler = OnvifDiscoveryHandler::new();
    let addr = DISCOVERY_ENDPOINT.parse()?;
    Server::builder().add_service(DiscoveryServer::new(discovery_handler)).serve(addr).await?;
    Ok(())
}