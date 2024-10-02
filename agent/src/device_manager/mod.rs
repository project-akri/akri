pub mod cdi;
mod in_memory;

pub use in_memory::InMemoryManager;

#[cfg(test)]
use mockall::automock;
#[cfg_attr(test, automock)]
pub trait DeviceManager: Send + Sync {
    fn get(&self, fqdn: &str) -> Option<cdi::Device>;
    #[allow(dead_code)]
    fn has_device(&self, fqdn: String) -> bool;
}
