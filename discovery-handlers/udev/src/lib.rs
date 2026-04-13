extern crate pest;
#[macro_use]
extern crate pest_derive;
extern crate udev;
#[macro_use]
extern crate serde_derive;

pub mod device_utils;
pub mod discovery_handler;
mod discovery_impl;
mod wrappers;

/// Name of environment variable that is set in udev brokers. Contains devnode for udev device
/// the broker should use.
pub const UDEV_DEVNODE_LABEL_ID: &str = "UDEV_DEVNODE";
/// Name of environment variable that is set in udev brokers. Contains devpath for udev device
/// the broker should connect to.
pub const UDEV_DEVPATH_LABEL_ID: &str = "UDEV_DEVPATH";
/// Prefix for USB resource ENV variable (e.g. USB_RESOURCE_AKRI_SH_UDEV_USB_GENERIC)
pub const USB_RESOURCE_PREFIX: &str = "USB_RESOURCE";
/// Prefix for PCI resource ENV variable (e.g. PCI_RESOURCE_AKRI_SH_UDEV_GPU_T400E)
pub const PCI_RESOURCE_PREFIX: &str = "PCI_RESOURCE";
/// Key used to pass the Kubernetes Device Plugin resource name
pub const DEVICE_PLUGIN_RESOURCE_PROPERTY_KEY: &str = "devicePluginResourceName";
/// Key used to enable VFIO PCI passthrough DeviceSpec injection.
pub const VFIO_PASSTHROUGH_PROPERTY_KEY: &str = "vfioPassthrough";
/// Name that udev discovery handlers use when registering with the Agent
pub const DISCOVERY_HANDLER_NAME: &str = "udev";
/// Defines whether this discovery handler discovers local devices on nodes rather than ones visible to multiple nodes
pub const SHARED: bool = false;
