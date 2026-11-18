use crate::PolicyIdError;

/// Why a policy command cannot be typed, encoded or digested.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CommandError {
    /// The command's policy identifier names no policy.
    #[error(transparent)]
    PolicyId(#[from] PolicyIdError),

    /// The relationship's subject is not one vera.rs takes a command for.
    #[error("relationship subject: {0}")]
    Subject(String),

    #[error("policy command: {0}")]
    Encode(#[source] serde_json::Error),

    #[error("policy command digest: {0}")]
    Digest(String),
}
