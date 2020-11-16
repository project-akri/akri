extern crate udev;

use super::udev_device::{
    get_attribute_value, get_devnode, get_devpath, get_driver, get_parent,
    get_parent_with_subsystem, get_property_value, get_sysname, DeviceExt,
};
use super::udev_enumerator::Enumerator;
use pest::iterators::Pair;
use pest::Parser;
use regex::Regex;

const TAGS: &str = "TAGS";

#[derive(Parser)]
#[grammar = "protocols/udev/udev_rule_grammar.pest"]
pub struct UdevRuleParser;

#[derive(Debug, PartialEq)]
pub struct UdevFilter<'a> {
    field: Pair<'a, Rule>,
    operation: Rule,
    value: String,
}

/// This parses the udev rule into UdevFilters and finds all devices that match those filters
pub fn do_parse_and_find(
    enumerator: impl Enumerator,
    udev_rule_string: &str,
) -> Result<Vec<String>, failure::Error> {
    let udev_filters = parse_udev_rule(udev_rule_string)?;
    let devpaths = find_devices(enumerator, udev_filters)?;
    trace!(
        "do_parse_and_find - returning discovered devices with devpaths: {:?}",
        devpaths
    );
    Ok(devpaths)
}

/// This parses a udev rule and returns a list of UdevFilter objects that specify which devices to search for.
/// This returns an error if the udev rule parameter does not fit the format specified in udev
/// man pages/wiki and therefore does not match the grammar specified in udev_rule_grammar.pest
/// A udev rule is made of a list of field-value pairs which have format field<operation>"value"
/// This function will only create UdevFilter objects for field-value pairs with supported fields and operations.
/// Udev discovery is only interested in match operations ("==",  "!="), so all action ("=" , "+=" , "-=" , ":=") operations
/// will be ignored.
/// Udev discovery is only interested in match fields, so all action fields, such as TEST, are ignored
fn parse_udev_rule(udev_rule_string: &str) -> Result<Vec<UdevFilter>, failure::Error> {
    info!(
        "parse_udev_rule - enter for udev rule string {}",
        udev_rule_string
    );
    let mut udev_filters: Vec<UdevFilter> = Vec::new();

    // So long as parse succeeds, subsequent unwraps will not fails, since they are following the
    // format specified in the grammar
    let udev_rule = UdevRuleParser::parse(Rule::udev_rule, udev_rule_string)?
        .next() // move to first rule within udev_rule aka inner_rule
        .unwrap() // does not panic because udev_rule always has inner_rule
        .into_inner() // go into inner_rule which has format { udev_filter ~ ("," ~ udev_filter)* }
        .next() // move to first rule in inner_rule aka udev_filter
        .unwrap(); // does not panic because inner_rule always has udev_filter

    trace!(
        "parse_udev_rule - parsing udev_rule {:?}",
        udev_rule.as_str()
    );
    for udev_filter in udev_rule.into_inner() {
        let mut inner_rules = udev_filter.into_inner();
        let field_pair = inner_rules.next().unwrap();
        let inner_field = field_pair.into_inner().next().unwrap();
        if inner_field.as_rule() == Rule::unsupported_field {
            return Err(failure::format_err!(
                "parse_udev_rule - unsupported field {}",
                inner_field.into_inner().next().unwrap().as_str()
            ));
        }

        let operation = inner_rules
            .next()
            .unwrap()
            .into_inner()
            .next()
            .unwrap()
            .as_rule();
        let mut quoted_value = inner_rules.next().unwrap().into_inner();
        let value = quoted_value.next().unwrap().as_str();
        if operation != Rule::action_operation {
            udev_filters.push(UdevFilter {
                field: inner_field,
                operation,
                value: value.to_string(),
            });
        } else {
            return Err(failure::format_err!("parse_udev_rule - unsupported action operation for rule with field [{}], operation [{:?}], and value[{}]",
            inner_field.into_inner().as_str(), operation, value));
        }
    }
    Ok(udev_filters)
}

/// This searches for devices that match the UdevFilters and returns their devpaths
fn find_devices(
    enumerator: impl Enumerator,
    udev_filters: Vec<UdevFilter>,
) -> std::io::Result<Vec<String>> {
    let mut enumerator = enumerator;
    trace!("find_devices - enter with udev_filters {:?}", udev_filters);

    // Enumerator scans sys devices for its filters. Only certain filters can be applied to it.
    // Divide device fields by type of filter than can be applied to Enumerator, if any
    // (1) Enumerator can filter for field by equality/match
    // (2) Enumerator can filter for field by inequality/nomatch
    // (3) Enumerator cannot filter for field. Must manually filter by looking at each Device the filtered Enumerator returns.
    let match_fields = vec![
        Rule::devpath,
        Rule::kernel,
        Rule::tag,
        Rule::subsystem,
        Rule::attribute,
        Rule::property,
    ];
    let nomatch_fields = vec![Rule::attribute, Rule::subsystem];

    let mut match_udev_filters: Vec<&UdevFilter> = Vec::new();
    let mut nomatch_udev_filters: Vec<&UdevFilter> = Vec::new();
    let mut remaining_udev_filters: Vec<&UdevFilter> = Vec::new();

    // Sort UdevFilters based off of which group they belong to
    udev_filters.iter().for_each(|udev_filter| {
        if udev_filter.operation == Rule::equality
            && match_fields.contains(&udev_filter.field.as_rule())
        {
            match_udev_filters.push(udev_filter);
        } else if udev_filter.operation == Rule::inequality
            && nomatch_fields.contains(&udev_filter.field.as_rule())
        {
            nomatch_udev_filters.push(udev_filter);
        } else {
            remaining_udev_filters.push(udev_filter);
        }
    });

    // Apply UdevFilters of groups in 1,2,3 order
    filter_by_match_udev_filters(&mut enumerator, match_udev_filters);
    filter_by_nomatch_udev_filters(&mut enumerator, nomatch_udev_filters);
    let devices: Vec<udev::Device> = enumerator.scan_devices()?.collect();
    let final_devices = filter_by_remaining_udev_filters(devices, remaining_udev_filters);

    let device_devpaths: Vec<String> = final_devices
        .into_iter()
        .filter_map(|device| {
            if let Some(devnode) = get_devnode(&device) {
                Some(devnode.to_str().unwrap().to_string())
            } else {
                None
            }
        })
        .collect();

    Ok(device_devpaths)
}

/// This adds equality filters to the Enumerator
fn filter_by_match_udev_filters(enumerator: &mut impl Enumerator, udev_filters: Vec<&UdevFilter>) {
    trace!(
        "enumerator_match_udev_filters - enter with udev_filters {:?}",
        udev_filters
    );
    for udev_filter in udev_filters {
        match udev_filter.field.as_rule() {
            Rule::devpath => {
                let mut syspath: String = "/sys".to_owned();
                syspath.push_str(&udev_filter.value);
                enumerator.add_syspath(&syspath).unwrap();
            }
            Rule::kernel => {
                enumerator.match_sysname(&udev_filter.value).unwrap();
            }
            Rule::tag => {
                enumerator.match_tag(&udev_filter.value).unwrap();
            }
            Rule::subsystem => {
                enumerator.match_subsystem(&udev_filter.value).unwrap();
            }
            Rule::attribute => {
                let key = udev_filter
                    .field
                    .clone()
                    .into_inner()
                    .next()
                    .unwrap()
                    .into_inner()
                    .next()
                    .unwrap()
                    .as_str();
                enumerator.match_attribute(key, &udev_filter.value).unwrap();
            }
            Rule::property => {
                let key = udev_filter
                    .field
                    .clone()
                    .into_inner()
                    .next()
                    .unwrap()
                    .into_inner()
                    .next()
                    .unwrap()
                    .as_str();
                enumerator.match_property(key, &udev_filter.value).unwrap();
            }
            _ => {
                error!("enumerator_match_udev_filters - encountered unsupported field");
            }
        }
    }
}

/// This adds inequality filters to the Enumerator
fn filter_by_nomatch_udev_filters(
    enumerator: &mut impl Enumerator,
    udev_filters: Vec<&UdevFilter>,
) {
    trace!(
        "enumerator_nomatch_udev_filters - enter with udev_filters {:?}",
        udev_filters
    );
    for udev_filter in udev_filters {
        match udev_filter.field.as_rule() {
            Rule::attribute => {
                let key = udev_filter
                    .field
                    .clone()
                    .into_inner()
                    .next()
                    .unwrap()
                    .into_inner()
                    .next()
                    .unwrap()
                    .as_str();
                enumerator
                    .nomatch_attribute(key, &udev_filter.value)
                    .unwrap();
            }
            Rule::subsystem => {
                enumerator.nomatch_subsystem(&udev_filter.value).unwrap();
            }
            _ => {
                error!("enumerator_nomatch_udev_filters - encountered unsupported field");
            }
        }
    }
}

/// This iterates over devices returned by filtered Enumerator and inspects the device's fields to see if they match/don't match
/// the fields in the remaining UdevFilters that could not be applied to Enumerator.
fn filter_by_remaining_udev_filters(
    devices: Vec<impl DeviceExt>,
    udev_filters: Vec<&UdevFilter>,
) -> Vec<impl DeviceExt> {
    trace!(
        "filter_by_remaining_udev_filters - enter with udev_filters {:?}",
        udev_filters
    );
    let mut mutable_devices = devices;
    for udev_filter in udev_filters {
        let value_regex = Regex::new(&udev_filter.value).unwrap();
        let is_equality = udev_filter.operation == Rule::equality;
        match udev_filter.field.as_rule() {
            Rule::devpath => {
                // Filter for inequality. Equality already accounted for in filter_by_match_udev_filters
                mutable_devices = mutable_devices
                    .into_iter()
                    .filter(|device| {
                        let devpath = get_devpath(device).to_str().unwrap();
                        match value_regex.find(devpath) {
                            Some(found_string) => {
                                found_string.start() != 0 || found_string.end() != devpath.len()
                            }
                            None => true,
                        }
                    })
                    .collect();
            }
            Rule::kernel => {
                // Filter for inequality. Equality already accounted for in filter_by_match_udev_filters
                mutable_devices = mutable_devices
                    .into_iter()
                    .filter(|device| {
                        let sysname = get_sysname(device).to_str().unwrap();
                        match value_regex.find(sysname) {
                            Some(found_string) => {
                                found_string.start() != 0 || found_string.end() != sysname.len()
                            }
                            None => true,
                        }
                    })
                    .collect();
            }
            Rule::tag => {
                mutable_devices = mutable_devices
                    .into_iter()
                    .filter(|device| {
                        if let Some(tags) = get_property_value(device, TAGS) {
                            let tags = tags.to_str().unwrap().split(':');
                            // Filter for inequality. Equality already accounted for in filter_by_match_udev_filters
                            // Return false if discover a tag that should be excluded
                            let mut include = true;
                            for tag in tags {
                                if let Some(found_string) = value_regex.find(tag) {
                                    if found_string.start() == 0 && found_string.end() == tag.len()
                                    {
                                        include = false;
                                        break;
                                    }
                                }
                            }
                            include
                        } else {
                            true
                        }
                    })
                    .collect();
            }
            Rule::property => {
                let key = udev_filter
                    .field
                    .clone()
                    .into_inner()
                    .next()
                    .unwrap()
                    .into_inner()
                    .next()
                    .unwrap()
                    .as_str();
                // Filter for inequality. Equality already accounted for in filter_by_match_udev_filters
                mutable_devices = mutable_devices
                    .into_iter()
                    .filter(|device| {
                        if let Some(property_value) = get_property_value(device, key) {
                            let property_value_str = property_value.to_str().unwrap();
                            match value_regex.find(property_value_str) {
                                Some(found_string) => {
                                    found_string.start() != 0
                                        || found_string.end() != property_value_str.len()
                                }
                                None => true,
                            }
                        } else {
                            true
                        }
                    })
                    .collect();
            }
            Rule::driver => {
                mutable_devices = mutable_devices
                    .into_iter()
                    .filter(|device| match get_driver(device) {
                        Some(driver) => {
                            let driver = driver.to_str().unwrap();
                            match value_regex.find(driver) {
                                Some(found_string) => {
                                    let is_match = found_string.start() == 0
                                        && found_string.end() == driver.len();
                                    (is_equality && is_match) || (!is_equality && !is_match)
                                }
                                None => !is_equality,
                            }
                        }
                        None => !is_equality,
                    })
                    .collect();
            }
            Rule::subsystems => {
                mutable_devices = mutable_devices
                    .into_iter()
                    .filter(|device| {
                        let is_match = device_has_parent_with_subsystem(device, &udev_filter.value);
                        (is_equality && is_match) || (!is_equality && !is_match)
                    })
                    .collect();
            }
            Rule::attributes => {
                let key = udev_filter
                    .field
                    .clone()
                    .into_inner()
                    .next()
                    .unwrap()
                    .into_inner()
                    .next()
                    .unwrap()
                    .as_str();
                mutable_devices = mutable_devices
                    .into_iter()
                    .filter(|device| {
                        let is_match =
                            device_has_parent_with_attribute(get_parent(device), key, &value_regex);
                        (is_equality && is_match) || (!is_equality && !is_match)
                    })
                    .collect();
            }
            Rule::drivers => {
                mutable_devices = mutable_devices
                    .into_iter()
                    .filter(|device| {
                        let is_match =
                            device_has_parent_with_driver(get_parent(device), &value_regex);
                        (is_equality && is_match) || (!is_equality && !is_match)
                    })
                    .collect();
            }
            Rule::kernels => {
                mutable_devices = mutable_devices
                    .into_iter()
                    .filter(|device| {
                        let is_match =
                            device_has_parent_with_sysname(get_parent(device), &value_regex);
                        (is_equality && is_match) || (!is_equality && !is_match)
                    })
                    .collect();
            }
            Rule::tags => {
                mutable_devices = mutable_devices
                    .into_iter()
                    .filter(|device| {
                        let is_match = device_has_parent_with_tag(get_parent(device), &value_regex);
                        (is_equality && is_match) || (!is_equality && !is_match)
                    })
                    .collect();
            }
            _ => {
                error!("filter_by_remaining_udev_filters - encountered unsupported field");
            }
        }
    }
    mutable_devices
}

/// Recursively look up a device's hierarchy to see if it has an ancestor with a specified subsystem.
pub fn device_has_parent_with_subsystem(device: &impl DeviceExt, subsystem: &str) -> bool {
    match get_parent_with_subsystem(device, subsystem).unwrap() {
        Some(_) => {
            println!(
                "true for subsystem {} and parent {:?}",
                subsystem,
                get_sysname(device)
            );
            true
        }
        None => {
            println!(
                "None for subsystem {} and parent {:?}",
                subsystem,
                get_sysname(device)
            );
            if let Some(parent) = get_parent(device) {
                device_has_parent_with_subsystem(&parent, subsystem)
            } else {
                false
            }
        }
    }
}

/// Recursively look up a device's hierarchy to see if it has an ancestor with a specified attribute.
pub fn device_has_parent_with_attribute(
    parent_option: Option<impl DeviceExt>,
    key: &str,
    value_regex: &Regex,
) -> bool {
    match parent_option {
        None => false,
        Some(parent) => {
            if let Some(attribute_value) = get_attribute_value(&parent, key) {
                let attribute_value_str = attribute_value.to_str().unwrap();
                if let Some(found_string) = value_regex.find(attribute_value_str) {
                    found_string.start() == 0 && found_string.end() == attribute_value_str.len()
                } else {
                    false
                }
            } else {
                device_has_parent_with_attribute(get_parent(&parent), key, value_regex)
            }
        }
    }
}

/// Recursively look up a device's hierarchy to see if it has an ancestor with a specified driver.
pub fn device_has_parent_with_driver(
    parent_option: Option<impl DeviceExt>,
    value_regex: &Regex,
) -> bool {
    match parent_option {
        None => false,
        Some(parent) => {
            if let Some(driver) = get_driver(&parent) {
                let driver_str = driver.to_str().unwrap();
                if let Some(found_string) = value_regex.find(driver_str) {
                    found_string.start() == 0 && found_string.end() == driver_str.len()
                } else {
                    false
                }
            } else {
                device_has_parent_with_driver(get_parent(&parent), value_regex)
            }
        }
    }
}

/// Recursively look up a device's hierarchy to see if it has an ancestor with a specified sysname aka kernel.
pub fn device_has_parent_with_sysname(
    parent_option: Option<impl DeviceExt>,
    value_regex: &Regex,
) -> bool {
    match parent_option {
        None => false,
        Some(parent) => {
            let sysname = get_sysname(&parent).to_str().unwrap();
            if let Some(found_string) = value_regex.find(sysname) {
                found_string.start() == 0 && found_string.end() == sysname.len()
            } else {
                device_has_parent_with_sysname(get_parent(&parent), value_regex)
            }
        }
    }
}

/// Recursively look up a device's hierarchy to see if it has an ancestor with a specified tag.
pub fn device_has_parent_with_tag(
    parent_option: Option<impl DeviceExt>,
    value_regex: &Regex,
) -> bool {
    match parent_option {
        None => false,
        Some(parent) => {
            if let Some(tags) = get_property_value(&parent, TAGS) {
                let tags = tags.to_str().unwrap().split(':');
                let mut has_tag = false;
                for tag in tags {
                    if let Some(found_string) = value_regex.find(tag) {
                        if found_string.start() == 0 && found_string.end() == tag.len() {
                            has_tag = true;
                            break;
                        }
                    }
                }
                if has_tag {
                    true
                } else {
                    device_has_parent_with_tag(get_parent(&parent), value_regex)
                }
            } else {
                device_has_parent_with_tag(get_parent(&parent), value_regex)
            }
        }
    }
}

#[cfg(test)]
mod discovery_tests {
    use super::super::udev_enumerator::{create_enumerator, MockEnumerator};
    use super::*;
    use std::{
        collections::HashMap,
        ffi::OsStr,
        fs::File,
        io::{prelude::*, BufReader},
        path::Path,
    };
    #[derive(Clone)]
    pub struct MockDevice<'a> {
        pub devpath: String,
        pub devnode: String,
        pub sysname: String,
        pub properties: std::collections::HashMap<String, String>,
        pub driver: Option<&'a OsStr>,
        pub subsystem: Option<String>,
        pub attributes: std::collections::HashMap<String, String>,
        pub parent: Box<Option<MockDevice<'a>>>,
    }

    impl<'a> DeviceExt for MockDevice<'a> {
        fn mockable_devpath(&self) -> &OsStr {
            OsStr::new(&self.devpath)
        }
        fn mockable_devnode(&self) -> Option<&Path> {
            Some(Path::new(&self.devnode))
        }
        fn mockable_sysname(&self) -> &OsStr {
            OsStr::new(&self.sysname)
        }
        fn mockable_property_value(&self, property: &str) -> Option<&OsStr> {
            if let Some(value) = self.properties.get(property) {
                Some(OsStr::new(value))
            } else {
                None
            }
        }
        fn mockable_attribute_value(&self, property: &str) -> Option<&OsStr> {
            if let Some(value) = self.attributes.get(property) {
                Some(OsStr::new(value))
            } else {
                None
            }
        }
        fn mockable_driver(&self) -> Option<&OsStr> {
            self.driver
        }
        fn mockable_parent_with_subsystem(&self, subsystem: &str) -> std::io::Result<Option<Self>> {
            match *self.parent.clone() {
                Some(parent) => {
                    if parent.subsystem.is_some() && &parent.subsystem.clone().unwrap() == subsystem
                    {
                        Ok(Some(parent))
                    } else {
                        Ok(None)
                    }
                }
                None => Ok(None),
            }
        }
        fn mockable_parent(&self) -> Option<Self> {
            *self.parent.clone()
        }
    }

    fn create_mock_device<'a>(
        devpath: &str,
        devnode: &str,
        sysname: &str,
        properties: HashMap<String, String>,
        attributes: HashMap<String, String>,
        driver: Option<&'a OsStr>,
        subsystem: Option<String>,
        parent: Option<MockDevice<'a>>,
    ) -> MockDevice<'a> {
        MockDevice {
            devpath: devpath.to_string(),
            devnode: devnode.to_string(),
            sysname: sysname.to_string(),
            properties,
            attributes,
            driver,
            subsystem,
            parent: Box::new(parent),
        }
    }

    #[test]
    fn test_parse_udev_rule_detailed() {
        let _ = env_logger::builder().is_test(true).try_init();
        let rule = "KERNEL==\"video[0-9]*\",SUBSYSTEM==\"video4linux\", ATTR{idVendor}==\"05a9\"";
        let udev_filters = parse_udev_rule(rule).unwrap();
        assert_eq!(udev_filters.len(), 3);
        assert_eq!(udev_filters[0].field.as_str(), "KERNEL");
        assert_eq!(udev_filters[0].operation, Rule::equality);
        assert_eq!(&udev_filters[0].value, "video[0-9]*");

        assert_eq!(udev_filters[1].field.as_str(), "SUBSYSTEM");
        assert_eq!(udev_filters[1].operation, Rule::equality);
        assert_eq!(&udev_filters[1].value, "video4linux");

        assert_eq!(udev_filters[2].field.as_str(), "ATTR{idVendor}");
        assert_eq!(udev_filters[2].operation, Rule::equality);
        assert_eq!(&udev_filters[2].value, "05a9");
    }

    #[test]
    fn test_parse_udev_rule_error() {
        // Throws error if unknown field (TYPO)
        let rule = "KERNEL==\"video[0-9]*\", TYPO==\"blah\", ATTR{idVendor}==\"05a9\", ATTRS{idProduct}==\"4519\"";
        assert!(parse_udev_rule(rule).is_err());

        // Throws error if leading space
        let rule = " KERNEL==\"video[0-9]*\", TYPO==\"blah\", ATTR{idVendor}==\"05a9\", ATTRS{idProduct}==\"4519\"";
        assert!(parse_udev_rule(rule).is_err());
    }

    #[test]
    fn test_parse_udev_rule_empty() {
        // Assert that doesn't throw error on empty rules
        let rule = "";
        let result = parse_udev_rule(rule);
        assert!(result.is_ok());
        let udev_filters = result.unwrap();
        assert_eq!(udev_filters.len(), 0);
    }

    #[test]
    fn test_parse_udev_rule_from_file() {
        let _ = env_logger::builder().is_test(true).try_init();
        let file_path = "../test/example.rules";
        let file = File::open(file_path).expect("no such file");
        let buf = BufReader::new(file);
        let mut num_udev_filters: Vec<usize> = Vec::new();
        let lines: Vec<String> = buf
            .lines()
            .map(|l| {
                let unwrapped = l.expect("Could not parse line");
                num_udev_filters.push(unwrapped[0..1].parse::<usize>().unwrap());
                unwrapped[2..].to_string()
            })
            .collect();
        for x in 0..lines.len() {
            let line = &lines[x];
            let udev_filters = parse_udev_rule(line).unwrap();
            assert_eq!(udev_filters.len(), num_udev_filters[x]);
        }
    }

    #[test]
    fn test_parse_unsupported_udev_rule_from_file() {
        let _ = env_logger::builder().is_test(true).try_init();
        let file_path = "../test/example-unsupported.rules";
        let file = File::open(file_path).expect("no such file");
        let buf = BufReader::new(file);
        buf.lines().for_each(|line| {
            let unwrapped = line.expect("Could not parse line");
            assert!(parse_udev_rule(&unwrapped).is_err());
        });
    }

    #[test]
    fn test_filter_by_match_udev_filters() {
        let rule = "SUBSYSTEM==\"video4linux\", ATTR{someKey}==\"1000\", KERNEL==\"video0\", ENV{ID}==\"1\", TAG==\"some_tag\", DEVPATH==\"/devices/path\"";
        let mut mock = MockEnumerator::new();
        mock.expect_match_subsystem()
            .times(1)
            .withf(move |value: &str| value == "video4linux")
            .returning(|_| Ok(()));
        mock.expect_match_attribute()
            .times(1)
            .withf(move |key: &str, value: &str| key == "someKey" && value == "1000")
            .returning(|_, _| Ok(()));
        mock.expect_match_sysname()
            .times(1)
            .withf(move |value: &str| value == "video0")
            .returning(|_| Ok(()));
        mock.expect_match_property()
            .times(1)
            .withf(move |key: &str, value: &str| key == "ID" && value == "1")
            .returning(|_, _| Ok(()));
        mock.expect_match_tag()
            .times(1)
            .withf(move |value: &str| value == "some_tag")
            .returning(|_| Ok(()));
        mock.expect_add_syspath()
            .times(1)
            .withf(move |value: &str| value == "/sys/devices/path")
            .returning(|_| Ok(()));
        let udev_filters = parse_udev_rule(rule).unwrap();
        let udev_filters: Vec<&UdevFilter> = udev_filters.iter().collect();
        filter_by_match_udev_filters(&mut mock, udev_filters);
    }

    #[test]
    fn test_filter_by_nomatch_udev_filters() {
        let rule = "SUBSYSTEM!=\"usb\", ATTR{someKey}!=\"1000\"";
        let mut mock = MockEnumerator::new();
        mock.expect_nomatch_subsystem()
            .times(1)
            .withf(move |value: &str| value == "usb")
            .returning(|_| Ok(()));
        mock.expect_nomatch_attribute()
            .times(1)
            .withf(move |key: &str, value: &str| key == "someKey" && value == "1000")
            .returning(|_, _| Ok(()));
        let udev_filters = parse_udev_rule(rule).unwrap();
        let udev_filters: Vec<&UdevFilter> = udev_filters.iter().collect();
        filter_by_nomatch_udev_filters(&mut mock, udev_filters);
    }

    #[test]
    fn test_filter_by_remaining_udev_filters() {
        let rule = "KERNEL!=\"video0\", TAG!=\"tag_exclude\", ENV{ID}!=\"id_num\", TAG!=\"tag[3-9]\", DEVPATH!=\"/devices/path/exclude\", DRIVER!=\"exclude\"";
        let mut include_properties = std::collections::HashMap::new();
        include_properties.insert("TAGS".to_string(), "tag0:tag_excluded:tag2".to_string());
        let mut tag_exclude_properties = std::collections::HashMap::new();
        tag_exclude_properties.insert("TAGS".to_string(), "tag3:other:tag2".to_string());
        let mut id_exclude_properties = std::collections::HashMap::new();
        id_exclude_properties.insert("ID".to_string(), "id_num".to_string());
        let mock_device_to_exclude0 = create_mock_device(
            "/devices/path/exclude",
            "/dev/exclude",
            "mock0",
            HashMap::new(),
            HashMap::new(),
            Some(OsStr::new("include")),
            None,
            None,
        );
        let mock_device_to_exclude1 = create_mock_device(
            "/devices/path/include",
            "/dev/exclude",
            "mock1",
            HashMap::new(),
            HashMap::new(),
            Some(OsStr::new("exclude")),
            None,
            None,
        );
        let mock_device_to_include1 = create_mock_device(
            "/devices/path/include",
            "/dev/include",
            "mock2",
            HashMap::new(),
            HashMap::new(),
            Some(OsStr::new("include")),
            None,
            None,
        );
        let mock_device_to_exclude3 = create_mock_device(
            "/devices/path/include",
            "/dev/include",
            "mock3",
            tag_exclude_properties,
            HashMap::new(),
            Some(OsStr::new("include")),
            None,
            None,
        );
        let mock_device_to_include2 = create_mock_device(
            "/devices/path/include",
            "/dev/include",
            "mock4",
            HashMap::new(),
            HashMap::new(),
            Some(OsStr::new("include")),
            None,
            None,
        );
        let mock_device_to_exclude4 = create_mock_device(
            "/devices/path/include",
            "/dev/include",
            "mock5",
            id_exclude_properties,
            HashMap::new(),
            Some(OsStr::new("include")),
            None,
            None,
        );
        let devices = vec![
            mock_device_to_exclude0,
            mock_device_to_exclude1,
            mock_device_to_include1,
            mock_device_to_exclude3,
            mock_device_to_include2,
            mock_device_to_exclude4,
        ];
        let udev_filters = parse_udev_rule(rule).unwrap();
        let udev_filters: Vec<&UdevFilter> = udev_filters.iter().collect();
        let filtered_devices = filter_by_remaining_udev_filters(devices, udev_filters);

        assert_eq!(filtered_devices.len(), 2);
        assert_eq!(get_sysname(&filtered_devices[0]).to_str().unwrap(), "mock2");
        assert_eq!(get_sysname(&filtered_devices[1]).to_str().unwrap(), "mock4");
    }

    #[test]
    fn test_filter_by_driver() {
        let match_rule = "DRIVER==\"some driver\"";
        let mock_device = create_mock_device(
            "/devices/path/include",
            "/dev/include",
            "mock",
            HashMap::new(),
            HashMap::new(),
            Some(OsStr::new("another driver")),
            None,
            None,
        );
        let udev_filters = parse_udev_rule(match_rule).unwrap();
        let udev_filters_ref: Vec<&UdevFilter> = udev_filters.iter().collect();
        let filtered_devices =
            filter_by_remaining_udev_filters(vec![mock_device.clone()], udev_filters_ref);
        assert_eq!(filtered_devices.len(), 0);

        let nomatch_rule = "DRIVER!=\"some driver\"";
        let udev_filters = parse_udev_rule(nomatch_rule).unwrap();
        let udev_filters_ref: Vec<&UdevFilter> = udev_filters.iter().collect();
        let filtered_devices =
            filter_by_remaining_udev_filters(vec![mock_device], udev_filters_ref);
        assert_eq!(filtered_devices.len(), 1);
    }

    #[test]
    fn test_filter_by_subsystems() {
        let rule = "SUBSYSTEMS==\"usb\"";
        let mock_usb_grandparent = create_mock_device(
            "/devices/path/usb",
            "/dev/node",
            "usb-grandparent",
            HashMap::new(),
            HashMap::new(),
            None,
            Some("usb".to_string()),
            None,
        );

        let mock_usb_parent = create_mock_device(
            "/devices/path/usb",
            "/dev/node",
            "usb-parent",
            HashMap::new(),
            HashMap::new(),
            None,
            None,
            Some(mock_usb_grandparent),
        );
        let mock_pci_parent = create_mock_device(
            "/devices/path",
            "/dev/node",
            "pci-parent",
            HashMap::new(),
            HashMap::new(),
            None,
            Some("pci".to_string()),
            None,
        );
        let mock_device_pci_child = create_mock_device(
            "/devices/path",
            "/dev/node",
            "pci-child",
            HashMap::new(),
            HashMap::new(),
            None,
            Some("random".to_string()),
            Some(mock_pci_parent),
        );
        let mock_device_usb_child = create_mock_device(
            "/devices/path",
            "/dev/node",
            "usb-child",
            HashMap::new(),
            HashMap::new(),
            Some(OsStr::new("driver")),
            Some("random".to_string()),
            Some(mock_usb_parent),
        );
        let devices = vec![mock_device_pci_child, mock_device_usb_child];
        let udev_filters = parse_udev_rule(rule).unwrap();
        let udev_filters: Vec<&UdevFilter> = udev_filters.iter().collect();
        let filtered_devices = filter_by_remaining_udev_filters(devices.clone(), udev_filters);

        assert_eq!(filtered_devices.len(), 1);
        assert_eq!(
            get_sysname(&filtered_devices[0]).to_str().unwrap(),
            "usb-child"
        );

        let rule = "SUBSYSTEMS==\"pci\"";
        let udev_filters = parse_udev_rule(rule).unwrap();
        let udev_filters: Vec<&UdevFilter> = udev_filters.iter().collect();
        let filtered_devices = filter_by_remaining_udev_filters(devices.clone(), udev_filters);
        assert_eq!(filtered_devices.len(), 1);
        assert_eq!(
            get_sysname(&filtered_devices[0]).to_str().unwrap(),
            "pci-child"
        );

        let rule = "SUBSYSTEMS!=\"pci\"";
        let udev_filters = parse_udev_rule(rule).unwrap();
        let udev_filters: Vec<&UdevFilter> = udev_filters.iter().collect();
        let filtered_devices = filter_by_remaining_udev_filters(devices, udev_filters);
        assert_eq!(filtered_devices.len(), 1);
        assert_eq!(
            get_sysname(&filtered_devices[0]).to_str().unwrap(),
            "usb-child"
        );
    }

    #[test]
    fn test_filter_by_attrs() {
        let rule = "ATTRS{someKey}==\"value\"";
        let mut attributes = std::collections::HashMap::new();
        attributes.insert("someKey".to_string(), "value".to_string());
        let mut attributes2 = std::collections::HashMap::new();
        attributes2.insert("someKey".to_string(), "value2".to_string());
        let mock_usb_grandparent = create_mock_device(
            "/devices/path",
            "/dev/node",
            "usb-grandparent",
            HashMap::new(),
            attributes,
            None,
            None,
            None,
        );
        let mock_usb_parent = create_mock_device(
            "/devices/path",
            "/dev/node",
            "usb-parent",
            HashMap::new(),
            HashMap::new(),
            None,
            Some("usb".to_string()),
            Some(mock_usb_grandparent),
        );
        let mock_pci_parent = create_mock_device(
            "/devices/path",
            "/dev/node",
            "pci-parent",
            HashMap::new(),
            attributes2,
            None,
            Some("pci".to_string()),
            None,
        );
        let mock_device_pci_child = create_mock_device(
            "/devices/path",
            "/dev/node",
            "pci-child",
            HashMap::new(),
            HashMap::new(),
            None,
            Some("random".to_string()),
            Some(mock_pci_parent),
        );
        let mock_device_usb_child = create_mock_device(
            "/devices/path",
            "/dev/node",
            "usb-child",
            HashMap::new(),
            HashMap::new(),
            Some(OsStr::new("driver")),
            Some("random".to_string()),
            Some(mock_usb_parent),
        );
        let devices = vec![mock_device_pci_child, mock_device_usb_child];
        let udev_filters = parse_udev_rule(rule).unwrap();
        let udev_filters: Vec<&UdevFilter> = udev_filters.iter().collect();
        let filtered_devices = filter_by_remaining_udev_filters(devices.clone(), udev_filters);

        assert_eq!(filtered_devices.len(), 1);
        assert_eq!(
            get_sysname(&filtered_devices[0]).to_str().unwrap(),
            "usb-child"
        );

        let rule = "ATTRS{someKey}!=\"value\"";
        let udev_filters = parse_udev_rule(rule).unwrap();
        let udev_filters: Vec<&UdevFilter> = udev_filters.iter().collect();
        let filtered_devices = filter_by_remaining_udev_filters(devices, udev_filters);
        assert_eq!(filtered_devices.len(), 1);
        assert_eq!(
            get_sysname(&filtered_devices[0]).to_str().unwrap(),
            "pci-child"
        );
    }

    #[test]
    fn test_filter_by_drivers() {
        let rule = "DRIVERS==\"some driver\"";
        let mock_usb_grandparent = create_mock_device(
            "/devices/path",
            "/dev/node",
            "usb1",
            HashMap::new(),
            HashMap::new(),
            Some(OsStr::new("some driver")),
            Some("usb".to_string()),
            None,
        );
        let mock_parent = create_mock_device(
            "/devices/path",
            "/dev/node",
            "random",
            HashMap::new(),
            HashMap::new(),
            None,
            None,
            Some(mock_usb_grandparent),
        );
        let mock_pci_parent = create_mock_device(
            "/devices/path",
            "/dev/node",
            "pci1",
            HashMap::new(),
            HashMap::new(),
            None,
            Some("pci".to_string()),
            None,
        );
        let mock_device_pci_child = create_mock_device(
            "/devices/path",
            "/dev/node",
            "pci-child",
            HashMap::new(),
            HashMap::new(),
            None,
            Some("random".to_string()),
            Some(mock_pci_parent),
        );
        let mock_device_usb_child = create_mock_device(
            "/devices/path",
            "/dev/node",
            "usb-child",
            HashMap::new(),
            HashMap::new(),
            Some(OsStr::new("driver")),
            Some("random".to_string()),
            Some(mock_parent),
        );
        let devices = vec![mock_device_pci_child, mock_device_usb_child];
        let udev_filters = parse_udev_rule(rule).unwrap();
        let udev_filters: Vec<&UdevFilter> = udev_filters.iter().collect();
        let filtered_devices = filter_by_remaining_udev_filters(devices.clone(), udev_filters);

        assert_eq!(filtered_devices.len(), 1);
        assert_eq!(
            get_sysname(&filtered_devices[0]).to_str().unwrap(),
            "usb-child"
        );

        let rule = "DRIVERS!=\"some driver\"";
        let udev_filters = parse_udev_rule(rule).unwrap();
        let udev_filters: Vec<&UdevFilter> = udev_filters.iter().collect();
        let filtered_devices = filter_by_remaining_udev_filters(devices, udev_filters);
        assert_eq!(filtered_devices.len(), 1);
        assert_eq!(
            get_sysname(&filtered_devices[0]).to_str().unwrap(),
            "pci-child"
        );
    }

    #[test]
    fn test_filter_by_tags() {
        let rule = "TAGS==\"tag[0-9]*\"";
        let mut properties = std::collections::HashMap::new();
        properties.insert("TAGS".to_string(), "tag0:middle_tag:tag".to_string());
        let mock_usb_grandparent = create_mock_device(
            "/devices/path",
            "/dev/node",
            "usb1",
            properties,
            HashMap::new(),
            Some(OsStr::new("some driver")),
            Some("usb".to_string()),
            None,
        );
        let mock_parent = create_mock_device(
            "/devices/path",
            "/dev/node",
            "random",
            HashMap::new(),
            HashMap::new(),
            None,
            None,
            Some(mock_usb_grandparent),
        );
        let mock_pci_parent = create_mock_device(
            "/devices/path",
            "/dev/node",
            "pci1",
            HashMap::new(),
            HashMap::new(),
            None,
            Some("pci".to_string()),
            None,
        );
        let mock_device_pci_child = create_mock_device(
            "/devices/path",
            "/dev/node",
            "pci-child",
            HashMap::new(),
            HashMap::new(),
            None,
            Some("random".to_string()),
            Some(mock_pci_parent),
        );
        let mock_device_usb_child = create_mock_device(
            "/devices/path",
            "/dev/node",
            "usb-child",
            HashMap::new(),
            HashMap::new(),
            Some(OsStr::new("driver")),
            Some("random".to_string()),
            Some(mock_parent),
        );
        let devices = vec![mock_device_pci_child, mock_device_usb_child];
        let udev_filters = parse_udev_rule(rule).unwrap();
        let udev_filters: Vec<&UdevFilter> = udev_filters.iter().collect();
        let filtered_devices = filter_by_remaining_udev_filters(devices.clone(), udev_filters);

        assert_eq!(filtered_devices.len(), 1);
        assert_eq!(
            get_sysname(&filtered_devices[0]).to_str().unwrap(),
            "usb-child"
        );

        let rule = "TAGS!=\"tag0\"";
        let udev_filters = parse_udev_rule(rule).unwrap();
        let udev_filters: Vec<&UdevFilter> = udev_filters.iter().collect();
        let filtered_devices = filter_by_remaining_udev_filters(devices, udev_filters);
        assert_eq!(filtered_devices.len(), 1);
        assert_eq!(
            get_sysname(&filtered_devices[0]).to_str().unwrap(),
            "pci-child"
        );
    }

    #[test]
    fn test_filter_by_kernels() {
        let rule = "KERNELS==\"usb[0-9]*\"";
        let mock_usb_grandparent = create_mock_device(
            "/devices/path",
            "/dev/node",
            "usb1",
            HashMap::new(),
            HashMap::new(),
            None,
            Some("usb".to_string()),
            None,
        );
        let mock_parent = create_mock_device(
            "/devices/path",
            "/dev/node",
            "random",
            HashMap::new(),
            HashMap::new(),
            None,
            None,
            Some(mock_usb_grandparent),
        );
        let mock_pci_parent = create_mock_device(
            "/devices/path",
            "/dev/node",
            "pci1",
            HashMap::new(),
            HashMap::new(),
            None,
            Some("pci".to_string()),
            None,
        );
        let mock_device_pci_child = create_mock_device(
            "/devices/path",
            "/dev/node",
            "pci-child",
            HashMap::new(),
            HashMap::new(),
            None,
            Some("random".to_string()),
            Some(mock_pci_parent),
        );
        let mock_device_usb_child = create_mock_device(
            "/devices/path",
            "/dev/node",
            "usb-child",
            HashMap::new(),
            HashMap::new(),
            Some(OsStr::new("driver")),
            Some("random".to_string()),
            Some(mock_parent),
        );
        let devices = vec![mock_device_pci_child, mock_device_usb_child];
        let udev_filters = parse_udev_rule(rule).unwrap();
        let udev_filters: Vec<&UdevFilter> = udev_filters.iter().collect();
        let filtered_devices = filter_by_remaining_udev_filters(devices.clone(), udev_filters);

        assert_eq!(filtered_devices.len(), 1);
        assert_eq!(
            get_sysname(&filtered_devices[0]).to_str().unwrap(),
            "usb-child"
        );

        let rule = "KERNELS!=\"usb[0-9]*\"";
        let udev_filters = parse_udev_rule(rule).unwrap();
        let udev_filters: Vec<&UdevFilter> = udev_filters.iter().collect();
        let filtered_devices = filter_by_remaining_udev_filters(devices, udev_filters);
        assert_eq!(filtered_devices.len(), 1);
        assert_eq!(
            get_sysname(&filtered_devices[0]).to_str().unwrap(),
            "pci-child"
        );
    }

    // Only tests that proper match calls were made
    #[test]
    fn test_do_parse_and_find() {
        let rule = "KERNEL==\"video[0-9]*\",ATTR{someKey}!=\"1000\", SUBSYSTEM==\"video4linux\"";
        let mut mock = MockEnumerator::new();
        mock.expect_match_subsystem()
            .times(1)
            .withf(move |value: &str| value == "video4linux")
            .returning(|_| Ok(()));
        mock.expect_nomatch_attribute()
            .times(1)
            .withf(move |key: &str, value: &str| key == "someKey" && value == "1000")
            .returning(|_, _| Ok(()));
        mock.expect_match_sysname()
            .times(1)
            .withf(move |value: &str| value == "video[0-9]*")
            .returning(|_| Ok(()));
        mock.expect_scan_devices().times(1).returning(|| {
            let mut enumerator = create_enumerator();
            enumerator
                .match_attribute("random", "attribute_that_should_not_be_found")
                .unwrap();
            enumerator.scan_devices()
        });
        assert_eq!(do_parse_and_find(mock, rule).unwrap().len(), 0);
    }
}
