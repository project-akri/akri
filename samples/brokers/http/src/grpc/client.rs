pub mod http {
    tonic::include_proto!("http");
}

use clap::{App, Arg};
use http::{device_service_client::DeviceServiceClient, ReadSensorRequest};
use tokio::{time, time::Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("[main] Entered");

    let matches = App::new("client")
        .arg(
            Arg::with_name("grpc_endpoint")
                .long("grpc_endpoint")
                .value_name("ENDPOINT")
                .help("Endpoint address of the gRPC server.")
                .required(true),
        )
        .get_matches();
    let grpc_endpoint = matches.value_of("grpc_endpoint").unwrap();

    let endpoint = format!("http://{}", grpc_endpoint);
    println!("[main] gRPC client dialing: {}", endpoint);
    let mut client = DeviceServiceClient::connect(endpoint).await?;

    loop {
        println!("[main:loop] Constructing Request");
        let rqst = tonic::Request::new(ReadSensorRequest {
            name: "/".to_string(),
        });
        println!("[main:loop] Calling read_sensor");
        let resp = client.read_sensor(rqst).await?;
        println!("[main:loop] Response: {:?}", resp);

        println!("[main:loop] Sleep");
        time::delay_for(Duration::from_secs(10)).await;
    }

    Ok(())
}
