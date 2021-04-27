pub mod discovery;
pub mod filtering;
pub mod registration_client;

#[macro_use]
extern crate serde_derive;

/// Path of the Agent registration socket
pub const AGENT_REGISTRATION_SOCKET_NAME: &str = "agent-registration.sock";

/// Name of the environment variable that holds the directory containing the Agent registration
/// and Discovery Handler sockets
pub const DISCOVERY_HANDLERS_DIRECTORY_LABEL: &str = "DISCOVERY_HANDLERS_DIRECTORY";

/// Returns the socket address for the Agent registration service
pub fn get_registration_socket() -> String {
    std::path::Path::new(&std::env::var(DISCOVERY_HANDLERS_DIRECTORY_LABEL).unwrap())
        .join(AGENT_REGISTRATION_SOCKET_NAME)
        .to_str()
        .unwrap()
        .to_string()
}
