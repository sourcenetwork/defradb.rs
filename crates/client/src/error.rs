/// Why an operation was refused. Each variant's text is the underlying
/// message unchanged, so the client sees the same words Go's DefraDB gives.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The caller may not do this on the node.
    #[error("{0}")]
    NotAuthorized(String),
    /// The caller's acting identity is malformed.
    #[error("{0}")]
    InvalidIdentity(String),
    /// The SDL did not parse.
    #[error("{0}")]
    Parse(String),
    /// The SDL parsed but the schema it defines is not valid.
    #[error("{0}")]
    Validation(String),
    /// A `@policy` directive names a policy or resource that cannot be used.
    #[error("{0}")]
    Policy(String),
    /// The collections could not be created.
    #[error("{0}")]
    Create(String),
    /// The node's own storage failed while serving the request.
    #[error("{0}")]
    Storage(String),
}

pub type Result<T> = std::result::Result<T, Error>;
