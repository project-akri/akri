use reqwest::get;
use std::env;
use tokio::{time, time::Duration};

const DEVICE_ENDPOINT: &str = "AKRI_HTTP_DEVICE_ENDPOINT";

async fn read_sensor(device_url: &str) {
    println!("[http:read_sensor] Entered");
    match get(device_url).await {
        Ok(resp) => {
            println!("[main:read_sensor] Response status: {:?}", resp.status());
            let body = resp.text().await;
            println!("[main:read_sensor] Response body: {:?}", body);
        }
        Err(err) => println!("Error: {:?}", err),
    };
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("[http:main] Entered");

    let device_url = env::var(DEVICE_ENDPOINT)?;
    println!("[http:main] Device: {}", &device_url);

    let mut tasks = Vec::new();
    tasks.push(tokio::spawn(async move {
        loop {
            println!("[http:main:loop] Sleep");
            time::delay_for(Duration::from_secs(10)).await;
            println!("[http:main:loop] read_sensor({})", &device_url);
            read_sensor(&device_url[..]).await;
        }
    }));
    futures::future::join_all(tasks).await;
    Ok(())
}
