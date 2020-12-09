pub mod http {
    tonic::include_proto!("http");
}

use clap::{App, Arg};
use http::{
    device_service_server::{DeviceService, DeviceServiceServer},
    ReadSensorRequest, ReadSensorResponse,
};
use reqwest::get;
use std::env;
use std::net::SocketAddr;
use tonic::{transport::Server, Code, Request, Response, Status};

const DEVICE_ENDPOINT: &str = "AKRI_HTTP_DEVICE_ENDPOINT";

#[derive(Default)]
pub struct Device {
    device_url: String,
}

#[tonic::async_trait]
impl DeviceService for Device {
    async fn read_sensor(
        &self,
        _rqst: Request<ReadSensorRequest>,
    ) -> Result<Response<ReadSensorResponse>, Status> {
        println!("[read_sensor] Entered");
        match get(&self.device_url).await {
            Ok(resp) => {
                println!("[read_sensor] Response status: {:?}", resp.status());
                let body = resp.text().await.unwrap();
                println!("[read_sensor] Response body: {:?}", body);
                Ok(Response::new(ReadSensorResponse { value: body }))
            }
            Err(err) => {
                println!("[read_sensor] Error: {:?}", err);
                Err(Status::new(Code::Unavailable, "device is unavailable"))
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("[main] Entered");

    let matches = App::new("broker")
        .arg(
            Arg::with_name("grpc_endpoint")
                .long("grpc_endpoint")
                .value_name("ENDPOINT")
                .help("Endpoint address that the gRPC server will listen on.")
                .required(true),
        )
        .get_matches();
    let grpc_endpoint = matches.value_of("grpc_endpoint").unwrap();

    let addr: SocketAddr = grpc_endpoint.parse().unwrap();
    println!("[main] gRPC service endpoint: {}", addr);

    let device_url = env::var(DEVICE_ENDPOINT)?;
    println!("[main] gRPC service proxying: {}", device_url);

    let device_service = Device { device_url };
    let service = DeviceServiceServer::new(device_service);

    println!("[main] gRPC service starting");
    Server::builder()
        .add_service(service)
        .serve(addr)
        .await
        .expect("unable to start http-prtocol gRPC server");

    Ok(())
}
