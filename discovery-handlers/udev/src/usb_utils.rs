pub fn extract_usb_address(devnode: &str) -> Option<(String, String)> {
    if !devnode.starts_with("/dev/bus/usb/") {
        return None;
    }
    
    let parts: Vec<&str> = devnode.split('/').collect();
    if parts.len() >= 2 {
        let bus = parts[parts.len() - 2];
        let device = parts[parts.len() - 1];
        
        if let (Ok(bus_num), Ok(dev_num)) = (bus.parse::<u32>(), device.parse::<u32>()) {
            return Some((bus_num.to_string(), dev_num.to_string()));
        }
    }
    
    None
}

pub fn to_kubevirt_resource_env_var(resource_name: &str) -> String {
    let transformed: String = resource_name
        .chars()
        .map(|c| match c {
            '.' | '/' => '_',
            other => other.to_ascii_uppercase(),
        })
        .collect();
    format!("USB_RESOURCE_{}", transformed)
}

pub fn has_hid_boot_interface(sysfs_devpath: &str, hid_protocol: &str) -> bool {
    use std::fs;
    use std::path::Path;

    let sysfs_path = format!("/sys{}", sysfs_devpath);
    let dev_dir = Path::new(&sysfs_path);

    let Ok(entries) = fs::read_dir(dev_dir) else {
        return false;
    };

    for entry in entries.flatten() {
        let intf_path = entry.path();
        let name = intf_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !name.contains(':') {
            continue;
        }

        let Ok(class) = fs::read_to_string(intf_path.join("bInterfaceClass")) else {
            continue;
        };
        if class.trim() != "03" {
            continue;
        }

        let Ok(subclass) = fs::read_to_string(intf_path.join("bInterfaceSubClass")) else {
            continue;
        };
        if subclass.trim() != "01" {
            continue;
        }

        let Ok(protocol) = fs::read_to_string(intf_path.join("bInterfaceProtocol")) else {
            continue;
        };
        if protocol.trim() == hid_protocol {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_usb_address_valid() {
        assert_eq!(
            extract_usb_address("/dev/bus/usb/001/010"),
            Some(("1".to_string(), "10".to_string()))
        );
        
        assert_eq!(
            extract_usb_address("/dev/bus/usb/002/005"),
            Some(("2".to_string(), "5".to_string()))
        );
        
        assert_eq!(
            extract_usb_address("/dev/bus/usb/003/127"),
            Some(("3".to_string(), "127".to_string()))
        );
    }

    #[test]
    fn test_extract_usb_address_invalid() {
        assert_eq!(extract_usb_address("/dev/video0"), None);
        assert_eq!(extract_usb_address("/dev/sda"), None);
        assert_eq!(extract_usb_address("/dev/ttyUSB0"), None);
        
        assert_eq!(extract_usb_address("/dev/bus/usb/"), None);
        assert_eq!(extract_usb_address("/dev/bus/usb/abc/def"), None);
        
        assert_eq!(extract_usb_address(""), None);
        assert_eq!(extract_usb_address("/"), None);
    }

    #[test]
    fn test_to_kubevirt_resource_env_var() {
        assert_eq!(
            to_kubevirt_resource_env_var("akri.sh/udev-usb-generic"),
            "USB_RESOURCE_AKRI_SH_UDEV-USB-GENERIC"
        );
        assert_eq!(
            to_kubevirt_resource_env_var("akri.sh/udev-usb-keyboard"),
            "USB_RESOURCE_AKRI_SH_UDEV-USB-KEYBOARD"
        );
        assert_eq!(
            to_kubevirt_resource_env_var("akri.sh/udev-usb-mouse"),
            "USB_RESOURCE_AKRI_SH_UDEV-USB-MOUSE"
        );
        assert_eq!(
            to_kubevirt_resource_env_var("example.com/my-device"),
            "USB_RESOURCE_EXAMPLE_COM_MY-DEVICE"
        );
    }

    #[test]
    fn test_has_hid_boot_interface_not_usb() {
        assert!(!has_hid_boot_interface("/devices/nonexistent/path", "01"));
    }

    #[test]
    fn test_extract_usb_address_edge_cases() {
        assert_eq!(
            extract_usb_address("/dev/bus/usb/1/5"),
            Some(("1".to_string(), "5".to_string()))
        );

        assert_eq!(
            extract_usb_address("/dev/bus/usb/001/001"),
            Some(("1".to_string(), "1".to_string()))
        );
    }
}
