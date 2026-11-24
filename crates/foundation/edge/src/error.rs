//! Why a network completion, or a call into the runtime, has no value.

use std::time::Duration;

/// Why a network completion, or a call into the runtime, has no value.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum EdgeError {
    /// The runtime, or the scope the work came through, stopped accepting
    /// work.
    #[error("the network runtime is shutting down")]
    Closed,
    /// The completion's deadline passed before the work finished.
    #[error("network work did not finish within {0:?}")]
    Deadline(Duration),
    /// The work panicked. Only its own completion fails.
    #[error("network work panicked: {0}")]
    Panicked(String),
    /// The work was stopped before it finished.
    #[error("network work was stopped before it finished")]
    Cancelled,
    /// The runtime could not be started.
    #[error("cannot start the network runtime: {0}")]
    Start(String),
}

/// For callers whose errors are text: the error's message.
impl From<EdgeError> for String {
    fn from(error: EdgeError) -> Self {
        error.to_string()
    }
}
