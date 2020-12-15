pub const BROKER_NAME: &str = "AKRI_ZEROCONF";
pub const DEVICE_KIND: &str = "AKRI_ZEROCONF_DEVICE_KIND";
pub const DEVICE_NAME: &str = "AKRI_ZEROCONF_DEVICE_NAME";
pub const DEVICE_HOST: &str = "AKRI_ZEROCONF_DEVICE_HOST";
pub const DEVICE_ADDR: &str = "AKRI_ZEROCONF_DEVICE_ADDR";
pub const DEVICE_PORT: &str = "AKRI_ZEROCONF_DEVICE_PORT";

// Prefix for environment variables created from discovered device's TXT records
pub const DEVICE_ENVS: &str = "AKRI_ZEROCONF_DEVICE";

pub const KIND: &str = "_rust._tcp";
pub const NAME: &str = "freddie";
pub const DOMAIN: &str = "local";
pub const ADDR: &str = "127.0.0.1";
pub const PORT: u16 = 8888;

// HOST = NAME.DOMAIN
pub const HOST: &str = "freddie.local";
