use crate::error::Error;

/// Why a node access control operation was refused or failed. Each refusal
/// Go's DefraDB has carries Go's text.
#[derive(Debug, thiserror::Error)]
pub enum NodeAcpError {
    /// Node access control was never set up on this node, or was purged.
    #[error("node acp is not configured")]
    NotConfigured,
    /// Disabling a node access control that is already off.
    #[error("node acp is already disabled")]
    AlreadyDisabled,
    /// Re-enabling a node access control that is already on.
    #[error("node acp is already enabled")]
    AlreadyEnabled,
    /// A relationship operation while node access control is off.
    #[error("operation requires ACP, but ACP not available")]
    AcpNotAvailable,
    /// A relationship operation with no identity acting in the request.
    #[error("node acp relationship operation requires identity")]
    RequiresIdentity,
    /// The relation is not one the node's policy has.
    #[error("relation not in resource")]
    RelationNotInResource,
    /// The target actor is neither a DID nor `*`.
    #[error("actor must be a valid did")]
    InvalidActor,
    /// The node access control store refused or failed.
    #[error(transparent)]
    Nac(#[from] crate::nac::Error),
    /// The caller may not run the operation.
    #[error(transparent)]
    Db(#[from] Error),
}
