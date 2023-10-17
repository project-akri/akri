use std::collections::HashSet;

use super::wrappers::{
    udev_device::{
        get_attribute_value, get_devnode, get_devpath, get_driver, get_parent, get_property_value,
        get_subsystem, get_sysname, DeviceExt,
    },
    udev_enumerator::Enumerator,
};
use log::{error, info, trace};
use pest::iterators::Pair;
use pest::Parser;
use regex::Regex;

const TAGS: &str = "TAGS";

#[derive(Parser)]
#[grammar = "udev_rule_grammar.pest"]
pub struct UdevRuleParser;

#[derive(Debug, PartialEq)]
pub struct UdevFilter<'a> {
    field: Pair<'a, Rule>,
    operation: Rule,
    value: String,
}

/// A udev device is defined by its devpath and devnode (if exists)
pub(crate) type DeviceProperties = (String, Option<String>);

/// This parses the udev rule into UdevFilters and finds all devices that match those filters
pub fn do_parse_and_find(
    enumerator: impl Enumerator,
    udev_rule_string: &str,
) -> Result<Vec<DeviceProperties>, anyhow::Error> {
    let udev_filters = parse_udev_rule(udev_rule_string)?;
    let devices = find_devices(enumerator, udev_filters)?;
    trace!(
        "do_parse_and_find - returning discovered devices with devpaths: {:?}",
        devices
    );
    Ok(devices)
}

/// This parses a udev rule and returns a list of UdevFilter objects that specify which devices to search for.
/// This returns an error if the udev rule parameter does not fit the format specified in udev
/// man pages/wiki and therefore does not match the grammar specified in udev_rule_grammar.pest
/// A udev rule is made of a list of field-value pairs which have format field<operation>"value"
/// This function will only create UdevFilter objects for field-value pairs with supported fields and operations.
/// Udev discovery is only interested in match operations ("==",  "!="), so all action ("=" , "+=" , "-=" , ":=") operations
/// will be ignored.
/// Udev discovery is only interested in match fields, so all action fields, such as TEST, are ignored
fn parse_udev_rule(udev_rule_string: &str) -> Result<Vec<UdevFilter>, anyhow::Error> {
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
            return Err(anyhow::format_err!(
                "parse_udev_rule - unsupported field {}",
                inner_field.into_inner().next().unwrap().as_str()
            ));
        }

        let operation_rule = inner_rules
            .next()
            .unwrap()
            .into_inner()
            .next()
            .unwrap()
            .as_rule();
        let mut quoted_value = inner_rules.next().unwrap().into_inner();
        let value = quoted_value.next().unwrap().as_str();
        if operation_rule != Rule::action_operation {
            udev_filters.push(UdevFilter {
                field: inner_field,
                operation: operation_rule,
                value: value.to_string(),
            });
        } else {
            return Err(anyhow::format_err!("parse_udev_rule - unsupported action operation for rule with field [{}], operation [{:?}], and value[{}]",
            inner_field.into_inner().as_str(), operation_rule, value));
        }
    }
    Ok(udev_filters)
}

/// This searches for devices that match the UdevFilters and returns their devpaths
fn find_devices(
    enumerator: impl Enumerator,
    udev_filters: Vec<UdevFilter>,
) -> std::io::Result<Vec<DeviceProperties>> {
    let mut enumerator = enumerator;
    trace!("find_devices - enter with udev_filters {:?}", udev_filters);

    // Enumerator scans sys devices for its filters. Only certain filters can be applied to it.
    // Divide device fields by type of filter than can be applied to Enumerator, if any
    // (1) Enumerator can filter for field by equality/match
    // (2) Enumerator can filter for field by inequality/nomatch
    // (3) Enumerator cannot filter for field. Must manually filter by looking at each Device the filtered Enumerator returns.
    let match_fields = [
        Rule::devpath,
        Rule::kernel,
        Rule::tag,
        Rule::subsystem,
        Rule::attribute,
        Rule::property,
    ];
    let nomatch_fields = [Rule::attribute, Rule::subsystem];

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

    let device_devpaths: Vec<DeviceProperties> = final_devices
        .into_iter()
        .map(|device| {
            (
                get_devpath(&device).to_str().unwrap().to_string(),
                get_devnode(&device).map(|devnode| devnode.to_str().unwrap().to_string()),
            )
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
                mutable_devices.retain(|device| {
                    let devpath = get_devpath(device).to_str().unwrap();
                    !is_regex_match(devpath, &value_regex)
                });
            }
            Rule::kernel => {
                // Filter for inequality. Equality already accounted for in filter_by_match_udev_filters
                mutable_devices.retain(|device| {
                    let sysname = get_sysname(device).to_str().unwrap();
                    !is_regex_match(sysname, &value_regex)
                });
            }
            Rule::tag => {
                mutable_devices.retain(|device| {
                    if let Some(tags) = get_property_value(device, TAGS) {
                        let tags = tags.to_str().unwrap().split(':');
                        // Filter for inequality. Equality already accounted for in filter_by_match_udev_filters
                        // Return false if discover a tag that should be excluded
                        let mut include = true;
                        for tag in tags {
                            if is_regex_match(tag, &value_regex) {
                                include = false;
                                break;
                            }
                        }
                        include
                    } else {
                        true
                    }
                });
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
                mutable_devices.retain(|device| {
                    if let Some(property_value) = get_property_value(device, key) {
                        let property_value_str = property_value.to_str().unwrap();
                        !is_regex_match(property_value_str, &value_regex)
                    } else {
                        true
                    }
                });
            }
            Rule::driver => {
                mutable_devices.retain(|device| match get_driver(device) {
                    Some(driver) => {
                        let driver = driver.to_str().unwrap();
                        filter_equality_check(is_equality, is_regex_match(driver, &value_regex))
                    }
                    None => !is_equality,
                });
            }
            Rule::subsystems => {
                mutable_devices.retain(|device| {
                    filter_equality_check(
                        is_equality,
                        device_or_parents_have_subsystem(device, &value_regex),
                    )
                });
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
                mutable_devices.retain(|device| {
                    filter_equality_check(
                        is_equality,
                        device_or_parents_have_attribute(device, key, &value_regex),
                    )
                });
            }
            Rule::drivers => {
                mutable_devices.retain(|device| {
                    filter_equality_check(
                        is_equality,
                        device_or_parents_have_driver(device, &value_regex),
                    )
                });
            }
            Rule::kernels => {
                mutable_devices.retain(|device| {
                    filter_equality_check(
                        is_equality,
                        device_or_parents_have_sysname(device, &value_regex),
                    )
                });
            }
            Rule::tags => {
                mutable_devices.retain(|device| {
                    filter_equality_check(
                        is_equality,
                        device_or_parents_have_tag(device, &value_regex),
                    )
                });
            }
            _ => {
                error!("filter_by_remaining_udev_filters - encountered unsupported field");
            }
        }
    }
    mutable_devices
}

/// Check whether the device should be selected based on equality and field matching
fn filter_equality_check(is_equality: bool, is_match: bool) -> bool {
    (is_equality && is_match) || (!is_equality && !is_match)
}

/// Check to see if the current value is a regex match of the requested value.
/// Ensure that the match is exclusively on the value to be tested. For example, for the regex `video[0-9]*`,
/// the values `video0` and `video10` should match; however, `blahvideo0blah` should not be accepted as a match.
fn is_regex_match(test_value: &str, value_regex: &Regex) -> bool {
    if let Some(value_containing_match) = value_regex.find(test_value) {
        value_containing_match.start() == 0 && value_containing_match.end() == test_value.len()
    } else {
        false
    }
}

/// Recursively look up a device's hierarchy to see if it or one of its ancestors has a specified subsystem.
fn device_or_parents_have_subsystem(device: &impl DeviceExt, value_regex: &Regex) -> bool {
    match get_subsystem(device) {
        Some(subsystem) => {
            let subsystem_str = subsystem.to_str().unwrap();
            if is_regex_match(subsystem_str, value_regex) {
                true
            } else {
                match get_parent(device) {
                    Some(parent) => device_or_parents_have_subsystem(&parent, value_regex),
                    None => false,
                }
            }
        }
        None => match get_parent(device) {
            Some(parent) => device_or_parents_have_subsystem(&parent, value_regex),
            None => false,
        },
    }
}

/// Recursively look up a device's hierarchy to see if it or one of its ancestors has a specified attribute.
fn device_or_parents_have_attribute(
    device: &impl DeviceExt,
    key: &str,
    value_regex: &Regex,
) -> bool {
    match get_attribute_value(device, key) {
        Some(attribute_value) => {
            let attribute_value_str = attribute_value.to_str().unwrap();
            if is_regex_match(attribute_value_str, value_regex) {
                true
            } else {
                match get_parent(device) {
                    Some(parent) => device_or_parents_have_attribute(&parent, key, value_regex),
                    None => false,
                }
            }
        }
        None => match get_parent(device) {
            Some(parent) => device_or_parents_have_attribute(&parent, key, value_regex),
            None => false,
        },
    }
}

/// Recursively look up a device's hierarchy to see if it or one of its ancestors has a specified driver.
fn device_or_parents_have_driver(device: &impl DeviceExt, value_regex: &Regex) -> bool {
    match get_driver(device) {
        Some(driver) => {
            let driver_str = driver.to_str().unwrap();
            if is_regex_match(driver_str, value_regex) {
                true
            } else {
                match get_parent(device) {
                    Some(parent) => device_or_parents_have_driver(&parent, value_regex),
                    None => false,
                }
            }
        }
        None => match get_parent(device) {
            Some(parent) => device_or_parents_have_driver(&parent, value_regex),
            None => false,
        },
    }
}

/// Recursively look up a device's hierarchy to see if it or one of its ancestors has a specified sysname aka kernel.
fn device_or_parents_have_sysname(device: &impl DeviceExt, value_regex: &Regex) -> bool {
    let sysname = get_sysname(device).to_str().unwrap();
    if is_regex_match(sysname, value_regex) {
        true
    } else {
        match get_parent(device) {
            Some(parent) => device_or_parents_have_sysname(&parent, value_regex),
            None => false,
        }
    }
}

/// Recursively look up a device's hierarchy to see if or one of its ancestors has a specified tag.
fn device_or_parents_have_tag(device: &impl DeviceExt, value_regex: &Regex) -> bool {
    if let Some(tags) = get_property_value(device, TAGS) {
        let tags = tags.to_str().unwrap().split(':');
        let mut has_tag = false;
        for tag in tags {
            if is_regex_match(tag, value_regex) {
                has_tag = true;
                break;
            }
        }
        if has_tag {
            true
        } else {
            match get_parent(device) {
                Some(parent) => device_or_parents_have_tag(&parent, value_regex),
                None => false,
            }
        }
    } else {
        match get_parent(device) {
            Some(parent) => device_or_parents_have_tag(&parent, value_regex),
            None => false,
        }
    }
}

/// Retrieve Parent or Children of a device using their sysfs path.
fn get_device_relatives<'a>(
    device_path: &str,
    possible_relatives: impl Iterator<Item = &'a String>,
) -> (Option<String>, Vec<String>) {
    let mut childrens = Vec::new();
    for relative in possible_relatives {
        match relative {
            parent if device_path.starts_with(relative.as_str()) => {
                return (Some(parent.clone()), vec![])
            }
            child if relative.starts_with(device_path) => childrens.push(child.clone()),
            _ => (),
        }
    }
    (None, childrens)
}

pub fn insert_device_with_relatives(
    devpaths: &mut std::collections::HashMap<String, HashSet<DeviceProperties>>,
    path: DeviceProperties,
) {
    match get_device_relatives(&path.0, devpaths.keys()) {
        (Some(parent), _) => {
            let _ = devpaths.get_mut(&parent).unwrap().insert(path);
        }
        (None, children) => {
            let id = path.0.clone();
            let mut children_devices: HashSet<DeviceProperties> = children
                .into_iter()
                .flat_map(|child| devpaths.remove(&child).unwrap().into_iter())
                .collect();
            children_devices.insert(path);
            let _ = devpaths.insert(id, children_devices);
        }
    }
}

#[cfg(test)]
mod discovery_tests {
    use super::super::wrappers::udev_enumerator::{create_enumerator, MockEnumerator};
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
        pub subsystem: Option<&'a OsStr>,
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
        fn mockable_subsystem(&self) -> Option<&OsStr> {
            self.subsystem
        }
        fn mockable_parent(&self) -> Option<Self> {
            *self.parent.clone()
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn create_mock_device<'a>(
        devpath: &str,
        devnode: &str,
        sysname: &str,
        properties: HashMap<String, String>,
        attributes: HashMap<String, String>,
        driver: Option<&'a OsStr>,
        subsystem: Option<&'a OsStr>,
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
        let file_path = "../../test/example.rules";
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
        let file_path = "../../test/example-unsupported.rules";
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

    // Test that hierarchy fields also check for match with device OR parent device
    #[test]
    fn test_filter_by_hierarchy_field() {
        let rule = "SUBSYSTEMS==\"usb\", ATTRS{someKey}==\"value\", TAGS==\"tag[0-9]*\", KERNELS==\"usb[0-9]*\", DRIVERS==\"some driver\"";
        let mut attributes = std::collections::HashMap::new();
        attributes.insert("someKey".to_string(), "value".to_string());
        let mut properties = std::collections::HashMap::new();
        properties.insert("TAGS".to_string(), "tag0:middle_tag:tag".to_string());
        let mock_device = create_mock_device(
            "/devices/path/usb",
            "/dev/node",
            "usb1",
            properties,
            attributes,
            Some(OsStr::new("some driver")),
            Some(OsStr::new("usb")),
            None,
        );
        let udev_filters = parse_udev_rule(rule).unwrap();
        let udev_filters: Vec<&UdevFilter> = udev_filters.iter().collect();
        let filtered_devices =
            filter_by_remaining_udev_filters(vec![mock_device.clone()], udev_filters);

        assert_eq!(filtered_devices.len(), 1);
        assert_eq!(get_sysname(&filtered_devices[0]).to_str().unwrap(), "usb1");

        let rule = "SUBSYSTEMS==\"usb\", ATTRS{someKey}==\"value\", TAGS==\"tag[0-9]*\", KERNELS==\"usb[0-9]*\", DRIVERS!=\"some driver\"";
        let udev_filters = parse_udev_rule(rule).unwrap();
        let udev_filters: Vec<&UdevFilter> = udev_filters.iter().collect();
        let filtered_devices = filter_by_remaining_udev_filters(vec![mock_device], udev_filters);
        assert_eq!(filtered_devices.len(), 0);
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
            Some(OsStr::new("usb")),
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
            Some(mock_usb_grandparent.clone()),
        );
        let mock_pci_parent = create_mock_device(
            "/devices/path",
            "/dev/node",
            "pci-parent",
            HashMap::new(),
            HashMap::new(),
            None,
            Some(OsStr::new("pci")),
            None,
        );
        let mock_device_pci_child = create_mock_device(
            "/devices/path",
            "/dev/node",
            "pci-child",
            HashMap::new(),
            HashMap::new(),
            None,
            Some(OsStr::new("random")),
            Some(mock_pci_parent),
        );
        let mock_device_usb_child = create_mock_device(
            "/devices/path",
            "/dev/node",
            "usb-child",
            HashMap::new(),
            HashMap::new(),
            Some(OsStr::new("driver")),
            Some(OsStr::new("random")),
            Some(mock_usb_parent.clone()),
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
        let filtered_devices = filter_by_remaining_udev_filters(devices.clone(), udev_filters);
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
            Some(OsStr::new("usb")),
            Some(mock_usb_grandparent),
        );
        let mock_pci_parent = create_mock_device(
            "/devices/path",
            "/dev/node",
            "pci-parent",
            HashMap::new(),
            attributes2,
            None,
            Some(OsStr::new("pci")),
            None,
        );
        let mock_device_pci_child = create_mock_device(
            "/devices/path",
            "/dev/node",
            "pci-child",
            HashMap::new(),
            HashMap::new(),
            None,
            Some(OsStr::new("random")),
            Some(mock_pci_parent),
        );
        let mock_device_usb_child = create_mock_device(
            "/devices/path",
            "/dev/node",
            "usb-child",
            HashMap::new(),
            HashMap::new(),
            Some(OsStr::new("driver")),
            Some(OsStr::new("random")),
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
            Some(OsStr::new("usb")),
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
            Some(OsStr::new("pci")),
            None,
        );
        let mock_device_pci_child = create_mock_device(
            "/devices/path",
            "/dev/node",
            "pci-child",
            HashMap::new(),
            HashMap::new(),
            None,
            Some(OsStr::new("random")),
            Some(mock_pci_parent),
        );
        let mock_device_usb_child = create_mock_device(
            "/devices/path",
            "/dev/node",
            "usb-child",
            HashMap::new(),
            HashMap::new(),
            Some(OsStr::new("driver")),
            Some(OsStr::new("random")),
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
            Some(OsStr::new("usb")),
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
            Some(OsStr::new("pci")),
            None,
        );
        let mock_device_pci_child = create_mock_device(
            "/devices/path",
            "/dev/node",
            "pci-child",
            HashMap::new(),
            HashMap::new(),
            None,
            Some(OsStr::new("random")),
            Some(mock_pci_parent),
        );
        let mock_device_usb_child = create_mock_device(
            "/devices/path",
            "/dev/node",
            "usb-child",
            HashMap::new(),
            HashMap::new(),
            Some(OsStr::new("driver")),
            Some(OsStr::new("random")),
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
            Some(OsStr::new("usb")),
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
            Some(OsStr::new("pci")),
            None,
        );
        let mock_device_pci_child = create_mock_device(
            "/devices/path",
            "/dev/node",
            "pci-child",
            HashMap::new(),
            HashMap::new(),
            None,
            Some(OsStr::new("random")),
            Some(mock_pci_parent),
        );
        let mock_device_usb_child = create_mock_device(
            "/devices/path",
            "/dev/node",
            "usb-child",
            HashMap::new(),
            HashMap::new(),
            Some(OsStr::new("driver")),
            Some(OsStr::new("random")),
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

    #[test]
    fn test_get_device_relatives() {
        let device_path = "/devices/pci0/usb0/0-1/0-1.1";
        let children_paths = vec![
            "/devices/pci0/usb0/0-1/0-1.1/0-1.1:1.0/video4linux/video0".to_string(),
            "/devices/pci0/usb0/0-1/0-1.1/0-1.1:1.0/video4linux/video1".to_string(),
            "/devices/pci0/usb0/0-1/0-1.1/0-1.1:1.1".to_string(),
        ];
        let unrelated_paths = vec![
            "/devices/pci0/usb0/0-1/0-1.2/0-1.2:1.1".to_string(),
            "/devices/pci0/usb1/0-1/0-1.2/0-1.2:1.1".to_string(),
        ];
        let parent_path = vec!["/devices/pci0/usb0".to_string()];
        let empty: Vec<String> = Vec::new();

        // Test with children devices
        let (parent_0, childrens_0) = get_device_relatives(
            device_path,
            unrelated_paths.iter().chain(children_paths.iter()),
        );
        assert!(parent_0.is_none());
        assert_eq!(childrens_0, children_paths);

        // Test with no possible relative devices
        let (parent_1, childrens_1) = get_device_relatives(device_path, empty.iter());
        assert!(parent_1.is_none());
        assert_eq!(childrens_1, empty);

        // Test with no related devices
        let (parent_2, childrens_2) = get_device_relatives(device_path, unrelated_paths.iter());
        assert!(parent_2.is_none());
        assert_eq!(childrens_2, empty);

        // Test with a parent device
        let (parent_4, childrens_4) =
            get_device_relatives(device_path, children_paths.iter().chain(parent_path.iter()));
        assert_eq!(parent_4, Some(parent_path[0].clone()));
        assert_eq!(childrens_4, empty);
    }

    #[test]
    fn test_insert_device_with_relatives() {
        let mut devpaths: HashMap<String, HashSet<DeviceProperties>> = HashMap::default();
        let related_devices = vec![
            ("/sys/device/parent".to_string(), None),
            (
                "/sys/device/parent/child1".to_string(),
                Some("/dev/dev1".to_string()),
            ),
            (
                "/sys/device/parent/child1/child2".to_string(),
                Some("/dev/dev2".to_string()),
            ),
        ];
        let unrelated_device = (
            "/sys/device/other".to_string(),
            Some("/dev/other".to_string()),
        );

        // Add first device
        insert_device_with_relatives(&mut devpaths, related_devices[1].clone());
        assert_eq!(
            devpaths,
            HashMap::from([(
                related_devices[1].0.clone(),
                HashSet::from([related_devices[1].clone()])
            )])
        );

        // Add its child
        insert_device_with_relatives(&mut devpaths, related_devices[2].clone());
        assert_eq!(
            devpaths,
            HashMap::from([(
                related_devices[1].0.clone(),
                HashSet::from([related_devices[1].clone(), related_devices[2].clone()])
            )])
        );

        // Add its parent
        insert_device_with_relatives(&mut devpaths, related_devices[0].clone());
        assert_eq!(
            devpaths,
            HashMap::from([(
                related_devices[0].0.clone(),
                HashSet::from([
                    related_devices[1].clone(),
                    related_devices[2].clone(),
                    related_devices[0].clone()
                ])
            )])
        );

        // Add it again
        insert_device_with_relatives(&mut devpaths, related_devices[0].clone());
        assert_eq!(
            devpaths,
            HashMap::from([(
                related_devices[0].0.clone(),
                HashSet::from([
                    related_devices[1].clone(),
                    related_devices[2].clone(),
                    related_devices[0].clone()
                ])
            )])
        );

        // Add a completely unrelated device
        insert_device_with_relatives(&mut devpaths, unrelated_device.clone());
        assert_eq!(
            devpaths,
            HashMap::from([
                (
                    related_devices[0].0.clone(),
                    HashSet::from([
                        related_devices[1].clone(),
                        related_devices[2].clone(),
                        related_devices[0].clone()
                    ])
                ),
                (
                    unrelated_device.0.clone(),
                    HashSet::from([unrelated_device])
                ),
            ])
        );
    }
}
