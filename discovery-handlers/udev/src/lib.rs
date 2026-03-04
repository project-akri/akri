extern crate pest;
#[macro_use]
extern crate pest_derive;
extern crate udev;
#[macro_use]
extern crate serde_derive;

pub mod discovery_handler;
mod discovery_impl;
pub mod usb_utils;
mod wrappers;

/// Name of environment variable that is set in udev brokers. Contains devnode for udev device
/// the broker should use.
pub const UDEV_DEVNODE_LABEL_ID: &str = "UDEV_DEVNODE";
/// Name of environment variable that is set in udev brokers. Contains devpath for udev device
/// the broker should connect to.
pub const UDEV_DEVPATH_LABEL_ID: &str = "UDEV_DEVPATH";
/// Name of environment variable for USB bus number (KubeVirt integration)
pub const USB_BUS_LABEL_ID: &str = "USB_BUS";
/// Name of environment variable for USB device number (KubeVirt integration)
pub const USB_DEVICE_LABEL_ID: &str = "USB_DEVICE";
/// Name that udev discovery handlers use when registering with the Agent
pub const DISCOVERY_HANDLER_NAME: &str = "udev";
/// Defines whether this discovery handler discovers local devices on nodes rather than ones visible to multiple nodes
pub const SHARED: bool = false;
