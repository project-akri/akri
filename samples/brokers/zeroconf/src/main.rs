use std::{env, fmt, fmt::Debug};
use tokio::{time, time::Duration};

const BROKER_NAME: &str = "AKRI_ZEROCONF";

const DEVICE_KIND: &str = "AKRI_ZEROCONF_DEVICE_KIND";
const DEVICE_NAME: &str = "AKRI_ZEROCONF_DEVICE_NAME";
const DEVICE_HOST: &str = "AKRI_ZEROCONF_DEVICE_HOST";
const DEVICE_ADDR: &str = "AKRI_ZEROCONF_DEVICE_ADDR";
const DEVICE_PORT: &str = "AKRI_ZEROCONF_DEVICE_PORT";

// TODO(dazwilkin) Should this be zeroconf::ServiceDiscovery?
#[derive(Default, Debug)]
struct Service {
    kind: String,
    name: String,
    host: String,
    addr: String,
    port: u16,
    // txt: String,
}
impl Service {
    pub fn new() -> Self {
        println!("[zeroconf:new] Entered");
        let kind = env::var(DEVICE_KIND).unwrap();
        let name = env::var(DEVICE_NAME).unwrap();
        let host = env::var(DEVICE_HOST).unwrap();
        let addr = env::var(DEVICE_ADDR).unwrap();
        let port: u16 = env::var(DEVICE_PORT).unwrap().parse().unwrap();
        println!(
            "[zeroconf:new]\n  Kind: {}\n  Name: {}\n  Host: {}\n  Addr: {}\n  Port: {}",
            kind, name, host, addr, port
        );
        Self {
            kind,
            name,
            host,
            addr,
            port,
        }
    }
}
impl fmt::Display for Service {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "kind: {}\nname: {}\nhost: {}\naddr: {}\nport: {}",
            self.kind, self.name, self.host, self.addr, self.port
        )
    }
}

async fn check_device(service: &Service) {
    println!("[zeroconf:read_device] Entered: {:?}", service);
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("[zeroconf:main] Entered");

    let service: Service = Service::new();
    println!("[zeroconf:main] Service: {}", &service);

    let mut tasks = Vec::new();
    tasks.push(tokio::spawn(async move {
        loop {
            println!("[zeroconf:main:loop] Sleep");
            time::delay_for(Duration::from_secs(10)).await;
            println!("[zeroconf:main:loop] check_device({:?})", &service);
            check_device(&service).await;
        }
    }));
    futures::future::join_all(tasks).await;
    Ok(())
}
