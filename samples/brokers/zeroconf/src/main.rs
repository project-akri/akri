use std::{collections::HashMap, env, fmt, fmt::Debug};
use tokio::{time, time::Duration};

const BROKER_NAME: &str = "AKRI_ZEROCONF";

const DEVICE_KIND: &str = "AKRI_ZEROCONF_DEVICE_KIND";
const DEVICE_NAME: &str = "AKRI_ZEROCONF_DEVICE_NAME";
const DEVICE_HOST: &str = "AKRI_ZEROCONF_DEVICE_HOST";
const DEVICE_ADDR: &str = "AKRI_ZEROCONF_DEVICE_ADDR";
const DEVICE_PORT: &str = "AKRI_ZEROCONF_DEVICE_PORT";

// Prefix for environment variables created from discovered device's TXT records
const DEVICE_ENVS: &str = "AKRI_ZEROCONF_DEVICE";

#[derive(Default, Debug)]
struct Service {
    kind: String,
    name: String,
    host: String,
    addr: String,
    port: u16,
    txts: Option<HashMap<String, String>>,
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
            txts: Service::txt_records(),
        }
    }
    fn txt_records() -> Option<HashMap<String, String>> {
        // `DEVICE_ENVS` includes known environment variables and any TXT records
        // Need to grab every candidate and then exclude known variables
        let result: HashMap<String, String> = env::vars()
            .filter(|(key, _)| key.contains(DEVICE_ENVS))
            .filter(|(key, _)| {
                !key.contains(DEVICE_KIND)
                    && !key.contains(DEVICE_NAME)
                    && !key.contains(DEVICE_HOST)
                    && !key.contains(DEVICE_ADDR)
                    && !key.contains(DEVICE_PORT)
            })
            .collect();
        if result.len() == 0 {
            None
        } else {
            Some(result)
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

#[cfg(test)]
mod test {
    use super::*;
    use std::env;

    // Every environment variable tests will have one of the following values
    const STR_VALUE: &str = "test-value";
    const U16_VALUE: u16 = 8888;

    // An array (!) containing test TXT records
    // These will be prefixed by `${DEVICE_ENVS}_`
    const TXTS: [&'static str; 5] = ["A", "B", "C", "D", "E"];

    // Create the service once
    fn new() -> Service {
        env::set_var(DEVICE_KIND, STR_VALUE);
        env::set_var(DEVICE_NAME, STR_VALUE);
        env::set_var(DEVICE_HOST, STR_VALUE);
        env::set_var(DEVICE_ADDR, STR_VALUE);
        env::set_var(DEVICE_PORT, U16_VALUE.to_string());

        for txt in TXTS.iter() {
            env::set_var(format!("{}_{}", DEVICE_ENVS, txt), STR_VALUE);
        }

        Service::new()
    }

    #[test]
    pub fn test_new_core() {
        let s = new();
        assert!(
            s.kind == STR_VALUE
                && s.name == STR_VALUE
                && s.host == STR_VALUE
                && s.addr == STR_VALUE
                && s.port == U16_VALUE
        )
    }
    #[test]
    pub fn test_new_txts() {
        let s = new();
        match s.txts {
            Some(txts) => assert!(TXTS.iter().all(|&e| {
                let key = format!("{}_{}", DEVICE_ENVS, e);
                println!("{} {} {:?}", e, txts.contains_key(&key), txts.get(&key));
                txts.contains_key(&key) && txts.get(&key) == Some(&STR_VALUE.to_string())
            })),
            None => panic!(),
        };
    }
}
