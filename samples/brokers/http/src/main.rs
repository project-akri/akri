use reqwest::get;
use std::env;
use tokio::{time, time::Duration};

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
async fn main() {
    println!("[http:main] Entered");

    // TODO(dazwilkin) Revise devices implementation so that DNS names are correct
    let device_url = "http://device-8000:8000";
    println!("[http:main] Device: {}", &device_url);

    let mut tasks = Vec::new();
    tasks.push(tokio::spawn(async move {
        loop {
            println!("[http:main:loop] Sleep");
            time::delay_for(Duration::from_secs(10)).await;
            println!(
                "[http:main:loop] read_sensor({})",
                device_url
            );
            read_sensor(device_url).await;
        }
    }));
    futures::future::join_all(tasks).await;
}
