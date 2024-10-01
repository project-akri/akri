pub(crate) mod controller_ctx;
pub mod instance_action;
pub mod node_watcher;
mod pod_action;
pub mod pod_watcher;
mod shared_test_utils;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum ControllerError {
    #[error(transparent)]
    KubeError(#[from] kube::Error),

    #[error("Finalizer Error: {0}")]
    // NB: awkward type because finalizer::Error embeds the reconciler error (which is this)
    // so boxing this error to break cycles
    FinalizerError(#[source] Box<kube::runtime::finalizer::Error<ControllerError>>),

    #[error("Watcher Error: {0}")]
    WatcherError(#[from] kube::runtime::watcher::Error),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type Result<T, E = ControllerError> = std::result::Result<T, E>;
