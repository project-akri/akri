use super::configuration::{Configuration, ConfigurationSpec};
use log::{error, trace};

/// Information for managing the state of a Configuration
#[derive(Debug, Clone)]
pub struct ConfigState {
    /// Tracks the last generation of the `Configuration` resource (i.e. `.metadata.generation`).
    /// This is used to determine if the `Configuration` actually changed, or if only the metadata changed.
    /// The `.metadata.generation` value is incremented for all changes, except for changes to `.metadata` or `.status`.
    pub last_generation: Option<i64>,
    /// The last ConfigurationSpec of this Configuration. Enables tracking changes in events of Configuration modification.
    pub last_configuration_spec: ConfigurationSpec,
}

/// In most cases, the Akri Agent handles Configuration changes by deleting and recreating the Configuration.
/// However, the Agent will not recreate the Configuration if the `.metadata.generation` has NOT changed or if
/// the only change to the ConfigurationSpec is to the BrokerType.
/// The `.metadata.generation` value is incremented for all changes, except for changes to `.metadata` or `.status`.
/// If only the BrokerType changed, the Controller should recreate brokers for the same Configuration.
pub fn should_recreate_config(config: &Configuration, config_state: &ConfigState) -> bool {
    if config.metadata.generation < config_state.last_generation {
        error!(
            "should_recreate_config - configuration generation somehow went backwards {:?} < {:?}",
            config.metadata.generation, config.metadata.generation
        );
        true
    // Immediately return false if the generation has not changed
    } else if config.metadata.generation == config_state.last_generation {
        trace!("should_recreate_config - configuration generation has not changed");
        false
    } else {
        let previous_config = &config_state.last_configuration_spec;
        if previous_config != &config.spec {
            // Recreate config only if the broker or services have changed
            !(previous_config.discovery_handler == config.spec.discovery_handler
                && previous_config.broker_properties == config.spec.broker_properties
                && previous_config.capacity == config.spec.capacity
                && previous_config.instance_service_spec == config.spec.instance_service_spec
                && previous_config.configuration_service_spec
                    == config.spec.configuration_service_spec)
        } else {
            trace!("should_recreate_config - Configuration has not changed even though generation has.");
            // Should not reach this as generation check should catch this. Recreate Configuration.
            true
        }
    }
}

#[cfg(test)]
mod config_recreate_tests {
    use super::super::configuration::BrokerType;
    use super::*;
    // Tests that when a Configuration is updated,
    // if generation has increased, should return true
    #[test]
    fn test_should_recreate_config_new_generation() {
        let (mut config, config_state) = get_should_recreate_config_data();

        // using higher generation as what is already in config_state
        config.metadata.generation = Some(2);
        let do_recreate = should_recreate_config(&config, &config_state);

        assert!(do_recreate)
    }

    // Tests that when a Configuration is updated,
    // if generation has increased, should return false
    // so long as broker HAS changed
    #[test]
    fn test_should_recreate_config_broker_change() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (mut config, config_state) = get_should_recreate_config_data();
        config.metadata.generation = Some(2);
        config.spec.broker_type.as_mut().map(|b| {
            if let BrokerType::Pod(p) = b {
                p.containers[0].name = "new-name".to_string();
            } else {
                panic!("Expected Configuration to contain PodSpec");
            }
        });
        let do_recreate = should_recreate_config(&config, &config_state);
        assert!(!do_recreate)
    }

    // Tests that when a Configuration is updated,
    // if generation has NOT changed, should return false
    #[test]
    fn test_should_recreate_config_same_generation() {
        let (mut config, config_state) = get_should_recreate_config_data();
        // using same generation as what is already in config_map
        config.metadata.generation = Some(1);
        let do_recreate = should_recreate_config(&config, &config_state);

        assert!(!do_recreate)
    }

    // Tests that when a Configuration is updated,
    // if generation has increased, should return true
    // if any part of the spec has changed besides the deployment options
    #[test]
    fn test_should_recreate_config_config_change() {
        let (mut config, config_state) = get_should_recreate_config_data();

        // using higher generation as what is already in config_state
        config.metadata.generation = Some(2);
        config.spec.capacity = 3;
        let do_recreate = should_recreate_config(&config, &config_state);

        assert!(do_recreate)
    }

    // Tests that when a Configuration is updated,
    // if generation is older, should return true
    // Kubernetes should prevent this from ever happening
    #[test]
    fn test_should_recreate_config_older_generation() {
        let (mut config, config_state) = get_should_recreate_config_data();

        // using older generation than what is already in config_state
        config.metadata.generation = Some(0);
        let do_recreate = should_recreate_config(&config, &config_state);

        assert!(do_recreate)
    }

    fn get_should_recreate_config_data() -> (Configuration, ConfigState) {
        let path_to_config = "../test/yaml/config-a.yaml";
        let config_yaml = std::fs::read_to_string(path_to_config).expect("Unable to read file");
        let config: Configuration = serde_yaml::from_str(&config_yaml).unwrap();
        let config_state = ConfigState {
            last_generation: Some(1),
            last_configuration_spec: config.spec.clone(),
        };
        (config, config_state)
    }
}
