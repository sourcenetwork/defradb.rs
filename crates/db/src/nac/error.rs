use acp::nac::NodePermission;
use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    #[error("node relationship permission denied")]
    PermissionDenied,
    #[error("acp error: {0}")]
    Acp(String),

    #[error("{0}")]
    Other(String),

    /// Node access control was never set up on this node, or was purged.
    #[error("node acp is not configured")]
    NotConfigured,

    /// Disabling a node access control that is already off.
    #[error("node acp is already disabled")]
    AlreadyDisabled,

    /// Re-enabling a node access control that is already on.
    #[error("node acp is already enabled")]
    AlreadyEnabled,

    /// A relationship change while node access control is off.
    #[error("operation requires ACP, but ACP not available")]
    NotEnabled,

    /// A relationship change with no identity acting in the request.
    #[error("node acp relationship operation requires identity")]
    RequiresIdentity,

    /// The relation is not one the node's policy has.
    #[error("relation not in resource")]
    RelationNotInResource,

    /// The target actor is neither a DID nor `*`.
    #[error("actor must be a valid did")]
    InvalidActor,

    /// Every try of a NAC write lost its commit to another write.
    #[error("transaction conflict: {0}")]
    Conflict(String),

    /// The caller lacks the node permission the operation needs.
    #[error("not authorized to perform operation. Permission: {}", .0.as_str())]
    NotAuthorized(NodePermission),

    /// The caller may not manage the relation it asked to change.
    #[error("UNAUTHORIZED: not document owner, cannot {operation}")]
    NotOwner { operation: String },
}

impl Error {
    /// A NAC store failure during `operation`, keeping a refused caller typed.
    pub(crate) fn acp(operation: &str, error: acp::Error) -> Self {
        match error {
            acp::Error::NotOwner { operation } => Error::NotOwner { operation },
            other => Error::Acp(format!("failed to {operation}: {other}")),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
