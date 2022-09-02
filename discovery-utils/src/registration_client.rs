use super::discovery::v0::{
    registration_client::RegistrationClient, RegisterDiscoveryHandlerRequest,
    QueryDeviceInfoRequest,Device,Mount
};
use log::{info, trace};
use std::convert::TryFrom;
use tonic::{
    transport::{Endpoint, Uri, Channel},
    Request,
};
use std::collections::HashMap;


async fn get_client() -> Result<RegistrationClient<Channel>, Box<dyn std::error::Error + Send + Sync + 'static>> {
    let channel= Endpoint::try_from("dummy://[::]:50051")?
    .connect_with_connector(tower::service_fn(move |_: Uri| {
        tokio::net::UnixStream::connect(super::get_registration_socket())
    }))
    .await?;
    Ok(RegistrationClient::new(channel))
}


//It invokes akri agent rpc method register_discovery_handler to register a discovery handler
pub async fn register_discovery_handler(
    register_request: &RegisterDiscoveryHandlerRequest,
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    info!("register_discovery_handler - entered");
    loop {
        // We will ignore this dummy uri because UDS does not use it.
        if let Ok(channel) = Endpoint::try_from("http://[::]:50051")?
            .connect_with_connector(tower::service_fn(move |_: Uri| {
                tokio::net::UnixStream::connect(super::get_registration_socket())
            }))
            .await
        {
            let mut client = RegistrationClient::new(channel);
            let request = Request::new(register_request.clone());
            client.register_discovery_handler(request).await?;
            break;
        }
        trace!("register_discovery_handler - sleeping for 10 seconds and trying again");
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    }
    info!("register_discovery_handler - Successfully");
    Ok(())
}

/// Continually waits for message to re-register with an Agent
pub async fn register_discovery_handler_again(
    mut register_receiver: tokio::sync::mpsc::Receiver<()>,
    register_request: &RegisterDiscoveryHandlerRequest,
) {
    loop {
        match register_receiver.recv().await {
            Some(_) => {
                info!("register_again - received signal ... registering with Agent again");
                register_discovery_handler(register_request).await.unwrap();
            }
            None => {
                info!("register_again - connection to register_again_sender closed ... error")
            }
        }
    }
}


//After discovery handler finds a list of devices. Discovery handler can use query_devices to fetch additional device information and put that into Device properties
#[derive(Clone)]
pub struct DeviceQueryInput {
    pub id: String,
    pub properties: HashMap<String,String>,
    pub query_device_payload: Option<String>,
    pub mounts: Vec<Mount>,
}

async fn query_device_info(
     mut query_input: DeviceQueryInput, query_http: String
) -> Device {
    let result = get_client().await;
    if let Ok(mut client) = result{
        if let Some(query_payload)=query_input.query_device_payload {
            let request = Request::new(QueryDeviceInfoRequest{
                query_device_payload:query_payload,
                query_device_http:query_http
            });
        
            let result = client.query_device_info(request).await;
            if let Ok(query_device_response) = result {
                let device_ext_info=query_device_response.into_inner().query_device_result;
                if device_ext_info.len()>0 {
                    query_input.properties.insert(crate::DEVICE_EXT_INFO_LABEL.to_string(),device_ext_info);
                }
            }
        }
    }

    Device {
        id: query_input.id,
        properties:query_input.properties,
        mounts: query_input.mounts,
        device_specs: Vec::default(),
    }
    
}

pub async fn query_devices(devices_query:Vec<DeviceQueryInput>,query_device_http:Option<String>) -> Vec<Device>{
    // in case there is query http for asking for additional device information, raise multiple rpc calls to query akri agent
    if let Some(query_http)=query_device_http{
        let mut fetch_devices_tasks = Vec::new();
        for device_query in devices_query.clone() {
            fetch_devices_tasks.push(tokio::spawn(query_device_info(device_query,query_http.clone())));    
        }
        let result= futures::future::try_join_all(fetch_devices_tasks).await;
        if let Ok(query_devices_result) = result{
            return query_devices_result;
        }
    }
    
    //if there is no need to query additional device information or the previous device query is not completely successful, simply assemeble Devices and return
    devices_query.iter()
    .map(|device_query| {
        Device {
            id: device_query.id.clone(),
            properties:device_query.properties.clone(),
            mounts: Vec::default(),
            device_specs: Vec::default(),
        }
    }).collect::<Vec<Device>>()
}

 