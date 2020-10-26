mod util;

use arraydeque::ArrayDeque;
use std::{
    env,
    sync::{Arc, Mutex},
};
use tokio::{time, time::Duration};
use util::{nessie_service, FrameBuffer};

fn get_nessie_url() -> String {
    env::var("nessie_url").unwrap()
}

async fn get_nessie(nessie_url: &String, frame_buffer: Arc<Mutex<FrameBuffer>>) {
    match reqwest::get(nessie_url).await {
        Ok(res) => {
            println!("reqwest result: {:?}", res);
            let bytes = match res.bytes().await {
                Ok(bytes) => bytes,
                Err(err) => {
                    println!("Failed to get nessie bytes from {}", &nessie_url);
                    println!("Error: {}", err);
                    return;
                }
            };
            frame_buffer.lock().unwrap().push_back(bytes.to_vec());
        }
        Err(err) => {
            println!("Failed to establish connection to {}", &nessie_url);
            println!("Error: {}", err);
            return;
        }
    };
}

#[tokio::main]
async fn main() {
    let frame_buffer: Arc<Mutex<FrameBuffer>> = Arc::new(Mutex::new(ArrayDeque::new()));
    let nessie_url = get_nessie_url();
    println!("nessie url: {:?}", &nessie_url);

    nessie_service::serve(frame_buffer.clone()).await.unwrap();

    let mut tasks = Vec::new();
    tasks.push(tokio::spawn(async move {
        loop {
            time::delay_for(Duration::from_secs(10)).await;
            get_nessie(&nessie_url, frame_buffer.clone()).await;
        }
    }));
    futures::future::join_all(tasks).await;
}
