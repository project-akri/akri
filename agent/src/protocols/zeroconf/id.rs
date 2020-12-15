use zeroconf::ServiceDiscovery;

pub fn id(s: &ServiceDiscovery) -> String {
    format!("{}.{}.{}:{}", s.name(), s.kind(), s.host_name(), s.port())
}
