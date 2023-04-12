pub mod udev_device {
    extern crate udev;
    use std::{ffi::OsStr, path::Path};

    /// Extension Trait for udev::Device. Enables creation of MockDevice for testing.
    pub trait DeviceExt: Sized {
        fn mockable_devpath(&self) -> &OsStr;
        fn mockable_devnode(&self) -> Option<&Path>;
        fn mockable_sysname(&self) -> &OsStr;
        fn mockable_property_value(&self, property: &str) -> Option<&OsStr>;
        fn mockable_attribute_value(&self, attribute: &str) -> Option<&OsStr>;
        fn mockable_driver(&self) -> Option<&OsStr>;
        fn mockable_subsystem(&self) -> Option<&OsStr>;
        fn mockable_parent(&self) -> Option<Self>
        where
            Self: Sized;
    }

    impl DeviceExt for udev::Device {
        fn mockable_devpath(&self) -> &OsStr {
            self.devpath()
        }
        fn mockable_devnode(&self) -> Option<&Path> {
            self.devnode()
        }
        fn mockable_sysname(&self) -> &OsStr {
            self.sysname()
        }
        fn mockable_property_value(&self, property: &str) -> Option<&OsStr> {
            self.property_value(property)
        }
        fn mockable_attribute_value(&self, attribute: &str) -> Option<&OsStr> {
            self.attribute_value(attribute)
        }
        fn mockable_driver(&self) -> Option<&OsStr> {
            self.driver()
        }
        fn mockable_subsystem(&self) -> Option<&OsStr> {
            self.subsystem()
        }
        fn mockable_parent(&self) -> Option<Self> {
            self.parent()
        }
    }

    pub fn get_devpath(device: &impl DeviceExt) -> &OsStr {
        device.mockable_devpath()
    }

    pub fn get_devnode(device: &impl DeviceExt) -> Option<&Path> {
        device.mockable_devnode()
    }

    pub fn get_sysname(device: &impl DeviceExt) -> &OsStr {
        device.mockable_sysname()
    }

    pub fn get_property_value<'a>(device: &'a impl DeviceExt, property: &str) -> Option<&'a OsStr> {
        device.mockable_property_value(property)
    }

    pub fn get_attribute_value<'a>(
        device: &'a impl DeviceExt,
        attribute: &str,
    ) -> Option<&'a OsStr> {
        device.mockable_attribute_value(attribute)
    }

    pub fn get_driver(device: &impl DeviceExt) -> Option<&OsStr> {
        device.mockable_driver()
    }

    pub fn get_subsystem(device: &impl DeviceExt) -> Option<&OsStr> {
        device.mockable_subsystem()
    }

    pub fn get_parent(device: &impl DeviceExt) -> Option<impl DeviceExt> {
        device.mockable_parent()
    }
}

pub mod udev_enumerator {
    extern crate udev;
    #[cfg(test)]
    use mockall::{automock, predicate::*};

    /// Wrap udev::Enumerator functions in a trait to enable mocking for testing.
    #[cfg_attr(test, automock)]
    pub trait Enumerator {
        fn match_subsystem(&mut self, value: &str) -> std::io::Result<()>;
        fn nomatch_subsystem(&mut self, value: &str) -> std::io::Result<()>;
        fn match_attribute(&mut self, key: &str, value: &str) -> std::io::Result<()>;
        fn nomatch_attribute(&mut self, key: &str, value: &str) -> std::io::Result<()>;
        fn match_sysname(&mut self, value: &str) -> std::io::Result<()>;
        fn match_property(&mut self, key: &str, value: &str) -> std::io::Result<()>;
        fn match_tag(&mut self, value: &str) -> std::io::Result<()>;
        fn add_syspath(&mut self, value: &str) -> std::io::Result<()>;
        fn scan_devices(&mut self) -> std::io::Result<udev::Devices>;
    }

    pub fn create_enumerator() -> impl Enumerator {
        EnumeratorImpl::new()
    }

    pub struct EnumeratorImpl {
        inner_enumerator: udev::Enumerator,
    }

    impl EnumeratorImpl {
        fn new() -> Self {
            EnumeratorImpl {
                inner_enumerator: udev::Enumerator::new().unwrap(),
            }
        }
    }

    impl Enumerator for EnumeratorImpl {
        fn match_subsystem(&mut self, value: &str) -> std::io::Result<()> {
            self.inner_enumerator.match_subsystem(value)
        }
        fn nomatch_subsystem(&mut self, value: &str) -> std::io::Result<()> {
            self.inner_enumerator.nomatch_subsystem(value)
        }
        fn match_attribute(&mut self, key: &str, value: &str) -> std::io::Result<()> {
            self.inner_enumerator.match_attribute(key, value)
        }
        fn nomatch_attribute(&mut self, key: &str, value: &str) -> std::io::Result<()> {
            self.inner_enumerator.nomatch_attribute(key, value)
        }
        fn match_sysname(&mut self, value: &str) -> std::io::Result<()> {
            self.inner_enumerator.match_sysname(value)
        }
        fn match_property(&mut self, key: &str, value: &str) -> std::io::Result<()> {
            self.inner_enumerator.match_property(key, value)
        }
        fn match_tag(&mut self, value: &str) -> std::io::Result<()> {
            self.inner_enumerator.match_tag(value)
        }
        fn add_syspath(&mut self, value: &str) -> std::io::Result<()> {
            self.inner_enumerator.add_syspath(value)
        }
        fn scan_devices(&mut self) -> std::io::Result<udev::Devices> {
            self.inner_enumerator.scan_devices()
        }
    }
}
