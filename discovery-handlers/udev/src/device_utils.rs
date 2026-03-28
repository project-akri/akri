fn transform_resource_name(resource_name: &str) -> String {
    resource_name
        .chars()
        .map(|c| match c {
            '.' | '/' => '_',
            other => other.to_ascii_uppercase(),
        })
        .collect()
}

pub fn to_usb_resource_env_var(resource_name: &str) -> String {
    format!(
        "{}_{}",
        super::USB_RESOURCE_PREFIX,
        transform_resource_name(resource_name)
    )
}

pub fn to_pci_resource_env_var(resource_name: &str) -> String {
    format!(
        "{}_{}",
        super::PCI_RESOURCE_PREFIX,
        transform_resource_name(resource_name)
    )
}

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

fn is_pci_address(s: &str) -> bool {
    let parts: Vec<&str> = s.splitn(3, ':').collect();
    if parts.len() != 3 {
        return false;
    }
    let slot_func: Vec<&str> = parts[2].splitn(2, '.').collect();
    if slot_func.len() != 2 {
        return false;
    }
    [parts[0], parts[1], slot_func[0], slot_func[1]]
        .iter()
        .all(|seg| !seg.is_empty() && seg.chars().all(|c| c.is_ascii_hexdigit()))
}

pub fn extract_pci_address(sysfs_path: &str) -> Option<String> {
    sysfs_path
        .split('/')
        .filter(|s| is_pci_address(s))
        .next_back()
        .map(|s| s.to_string())
}

pub fn read_iommu_group(devpath: &str) -> Option<String> {
    let full_path = format!("/sys{devpath}");
    let iommu_link = std::path::Path::new(&full_path).join("iommu_group");
    std::fs::read_link(&iommu_link).ok().and_then(|target| {
        target
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_usb_resource_env_var() {
        assert_eq!(
            to_usb_resource_env_var("akri.sh/udev-usb-keyboard"),
            "USB_RESOURCE_AKRI_SH_UDEV-USB-KEYBOARD"
        );
        assert_eq!(
            to_usb_resource_env_var("akri.sh/udev-usb-mouse"),
            "USB_RESOURCE_AKRI_SH_UDEV-USB-MOUSE"
        );
        assert_eq!(
            to_usb_resource_env_var("example.com/my-device"),
            "USB_RESOURCE_EXAMPLE_COM_MY-DEVICE"
        );
    }

    #[test]
    fn test_to_pci_resource_env_var() {
        assert_eq!(
            to_pci_resource_env_var("example.com/my-gpu"),
            "PCI_RESOURCE_EXAMPLE_COM_MY-GPU"
        );
        assert_eq!(
            to_pci_resource_env_var("akri.sh/udev-gpu-t400e"),
            "PCI_RESOURCE_AKRI_SH_UDEV-GPU-T400E"
        );
        assert_eq!(
            to_pci_resource_env_var("vendor.io/device"),
            "PCI_RESOURCE_VENDOR_IO_DEVICE"
        );
    }

    #[test]
    fn test_extract_pci_address() {
        assert_eq!(
            extract_pci_address("/sys/devices/pci0000:00/0000:00:01.0/0000:01:00.0"),
            Some("0000:01:00.0".to_string())
        );
        assert_eq!(
            extract_pci_address("/sys/devices/pci0000:00/0000:03:00.0"),
            Some("0000:03:00.0".to_string())
        );
        assert_eq!(extract_pci_address("/dev/bus/usb/001/010"), None);
        assert_eq!(extract_pci_address("/dev/video0"), None);
    }

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
