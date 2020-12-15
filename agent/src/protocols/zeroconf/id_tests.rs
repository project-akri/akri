use super::constants::*;
use super::id::id;
use zeroconf::{browser::ServiceDiscoveryBuilder, TxtRecord};

#[test]
fn test_id() {
    let s = ServiceDiscoveryBuilder::default()
        .kind(KIND.to_string())
        .name(NAME.to_string())
        .domain(DOMAIN.to_string())
        .host_name(HOST.to_string())
        .address(ADDR.to_string())
        .port(PORT)
        .txt(None)
        .build()
        .expect("service");
    assert_eq!(id(&s), format!("{}.{}.{}:{}", NAME, KIND, HOST, PORT))
}
