use mockall::{automock, predicate::*};
use std::{env, env::VarError};

/// This provides a mockable way to query an env var.
#[automock]
pub trait EnvVarQuery {
    fn get_env_var(&self, name: &'static str) -> Result<String, VarError>;
    fn get_env_vars(&self) -> Vec<(String, String)>;
}

pub struct ActualEnvVarQuery;
impl EnvVarQuery for ActualEnvVarQuery {
    /// Gets an environment variable using std::env::var
    ///
    /// Example
    /// ```
    /// use akri_shared::os::env_var::EnvVarQuery;
    ///
    /// let env_query = akri_shared::os::env_var::ActualEnvVarQuery{};
    /// assert_eq!(
    ///     std::env::var("HOSTNAME"),
    ///     env_query.get_env_var("HOSTNAME")
    /// );
    /// ```
    fn get_env_var(&self, name: &'static str) -> Result<String, VarError> {
        env::var(name)
    }

    fn get_env_vars(&self) -> Vec<(String, String)> {
        env::vars()
            .map(|(n, v)| (n, v))
            .collect::<Vec<(String, String)>>()
    }
}
