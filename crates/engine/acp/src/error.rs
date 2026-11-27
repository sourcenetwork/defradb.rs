//! Error types for the ACP crate

use thiserror::Error;

/// Result type alias for ACP operations
pub type Result<T> = std::result::Result<T, Error>;

/// ACP-specific error types
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    #[error("a different policy already exists under ID {0}")]
    PolicyAlreadyExists(String),

    /// JSON serialization/deserialization errors
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),

    /// Permission denied for the requested operation
    #[error("permission denied: {0}")]
    PermissionDenied(String),

    /// Document is not registered with ACP
    #[error("document not registered: {0}")]
    DocumentNotRegistered(String),

    /// Document is already registered with ACP
    #[error("document already registered: {0}")]
    DocumentAlreadyRegistered(String),

    /// Invalid relation name
    #[error("invalid relation: {0}")]
    InvalidRelation(String),

    /// The backend cannot represent the requested subject (e.g. a cross-object
    /// or userset edge on the Local or Vera backend, which store bare DIDs).
    #[error("unsupported subject for this backend: {0}")]
    UnsupportedSubject(String),

    #[error("certified replica group membership is unsupported by this backend")]
    UnsupportedReplicaMembership,

    /// The backend cannot evaluate named document permissions.
    #[error("named document permission checks are unsupported by this backend")]
    UnsupportedNamedPermission,

    /// The backend archives no object for an owed change.
    #[error("archiving an object is unsupported by this backend")]
    UnsupportedArchive,

    /// The backend cannot list a document's relationships.
    #[error("listing document relationships is unsupported by this backend")]
    UnsupportedRelationshipListing,

    /// This provider cannot bind native node-administrator relationship writes.
    #[error("transactional node-administrator relationship management is unsupported")]
    UnsupportedAdministrativeRelationship,

    /// The provider refused to register a document because its creator may
    /// not create it in its collection (the policy resource's
    /// `collection.create`). Final: sending the registration again cannot
    /// change the answer for the same relationships.
    #[error("not authorized to create document {doc_id} in its collection: {reason}")]
    CreateNotAuthorized { doc_id: String, reason: String },

    /// A create in a collection whose policy gates creates was made without
    /// an identity.
    #[error("not authorized to create a document in collection {collection} without an identity: its policy restricts who may create documents")]
    AnonymousCreate { collection: String },

    /// Not the owner of the document (UNAUTHORIZED)
    #[error("UNAUTHORIZED: not document owner, cannot {operation}")]
    NotOwner { operation: String },

    /// Actor is not a manager of the relation
    #[error("cannot {operation}: actor is not a manager of relation")]
    NotManager { operation: String },

    /// One transaction staged more relationship changes owed to an external
    /// provider than [`crate::outbox::MAX_OWED_CHANGES_PER_TRANSACTION`].
    #[error("a transaction may stage at most {limit} ACP changes owed to the provider")]
    OwedChangesPerTransaction { limit: u32 },

    /// The node already owes its external provider as many relationship
    /// changes as it may hold unsettled; staging resumes as they settle.
    #[error("{unsettled} ACP changes are owed to the provider, at the node's limit of {limit}")]
    OwedChangesPerNode { unsettled: u64, limit: u64 },

    /// The store holds relationship changes owed in the layout before the
    /// owed counts were split (`/acp-owed/`), which this version neither
    /// submits nor denies by. Nothing is served until they are settled with
    /// the version that staged them, or the store is created again.
    #[error(
        "the store holds {count} relationship changes owed to the ACP provider in an earlier layout; settle them with the version that staged them, or create the store again, before starting this version"
    )]
    LegacyOwedChanges { count: u64 },

    /// An owed relationship change does not fit its record.
    #[error("an owed ACP change takes {size} bytes, over the limit of {limit}")]
    OwedChangeTooLarge { size: usize, limit: usize },

    /// The node cannot submit a relationship change as its acting DID: it
    /// holds no key for that DID and the request carries no delegation the
    /// provider takes for it.
    #[error("no authority to submit ACP changes as {actor}: {reason}")]
    OwedAuthorityUnavailable { actor: String, reason: String },

    /// The delegation the request carries for `actor` does not hold at the
    /// provider's time, so nothing is sent under it. The request may be made
    /// again with a delegation issued at
    /// [`DelegationWindow::provider_time`](crate::DelegationWindow::provider_time).
    #[error("the delegation for {actor} is refused: {window}")]
    DelegationOutsideWindow {
        actor: String,
        window: crate::DelegationWindow,
    },

    /// An external provider could not be reached or did not answer in time.
    /// Nothing is known about what was asked; the same call may be made
    /// again.
    #[error("ACP provider unavailable: {0}")]
    ProviderUnavailable(String),

    /// A relationship delete cannot be staged while the provider cannot
    /// confirm its acting DID's authority: an owed delete denies its subject
    /// from the commit on, so it is never taken on an unchecked word. The
    /// same call succeeds once the provider answers.
    #[error("cannot check {actor}'s authority to revoke on {object} while the ACP provider is unavailable; try again")]
    OwedCheckUnavailable { actor: String, object: String },

    /// One acting DID already owes the provider as many relationship changes
    /// as one may hold unsettled; staging resumes as they settle.
    #[error("{unsettled} ACP changes by {actor} are owed to the provider, at the limit of {limit} for one actor")]
    OwedChangesPerActor {
        actor: String,
        unsettled: u64,
        limit: u64,
    },

    /// A relationship change made outside a transaction would reach the
    /// provider before a change of the same relationship that a transaction
    /// staged earlier and that is still owed. The same call succeeds once
    /// that change is settled.
    #[error(
        "an earlier change of {relation} on {object} is still owed to the ACP provider; try again"
    )]
    OwedChangesPending { object: String, relation: String },

    /// Invalid policy configuration
    #[error("invalid policy: {0}")]
    InvalidPolicy(String),

    /// Enabling NAC that is temporarily disabled: only an authorized
    /// re-enable turns it back on.
    #[error("node acp is temporarily disabled; re-enable it instead")]
    NacTemporarilyDisabled,

    /// Storage operation failed (generic)
    #[error("storage error: {0}")]
    Storage(String),

    /// Storage read operation failed
    #[error("storage read error: {operation} - {details}")]
    StorageRead { operation: String, details: String },

    /// Storage write operation failed
    #[error("storage write error: {operation} - {details}")]
    StorageWrite { operation: String, details: String },

    /// Storage transaction failed
    #[error("storage transaction error: {operation} - {details}")]
    StorageTransaction { operation: String, details: String },

    /// Storage iteration failed
    #[error("storage iteration error: {operation} - {details}")]
    StorageIteration { operation: String, details: String },

    /// Serialization/deserialization error
    #[error("serialization error: {0}")]
    Serialization(String),

    /// Policy not found
    #[error("policy not found: {0}")]
    PolicyNotFound(String),

    /// Relation not found in policy
    #[error("relation not found: {relation} in resource {resource}")]
    RelationNotFound { resource: String, relation: String },

    /// Cycle detected in permission evaluation
    #[error("cycle detected in permission evaluation: {0}")]
    CycleDetected(String),

    /// Nothing is owed under the reference its caller gave, or what is
    /// owed there belongs to another actor.
    #[error("no owed ACP entry {entry} for {actor}")]
    OwedEntryNotFound { entry: String, actor: String },

    /// A transaction that committed meanwhile changed what is owed on the
    /// entry's relationship; nothing was written. The same call may be made
    /// again.
    #[error("owed ACP {entry} was changed by a transaction that committed meanwhile; try again")]
    OwedEntryConflict { entry: String },

    /// Only a change the provider refused may be abandoned.
    #[error("owed ACP change {seq} is {state}, and only a refused change may be abandoned; present a fresh credential for it instead")]
    OwedChangeNotRefused { seq: u64, state: &'static str },

    /// Invalid expression
    #[error("invalid expression: {0}")]
    InvalidExpression(String),

    /// Resource not found in policy
    #[error("resource not found: {0}")]
    ResourceNotFound(String),

    /// Invalid EntitySet subject reference
    #[error("invalid EntitySet reference: resource '{resource}' relation '{relation}' does not exist in policy")]
    InvalidEntitySetReference { resource: String, relation: String },

    /// Subject restriction violation
    #[error("subject restriction violated: {message}")]
    SubjectRestrictionViolation { message: String },

    /// DPI compliance violation: missing owner relation
    #[error("DPI violation: resource '{resource}' must have an 'owner' relation")]
    DpiMissingOwner { resource: String },

    /// DPI compliance violation: expression doesn't include owner access
    #[error("DPI violation: permission '{relation}' on resource '{resource}' must include 'owner' in its expression")]
    DpiExpressionMissingOwner { resource: String, relation: String },

    /// DPI compliance violation: disallowed operation
    #[error("DPI violation: resource '{resource}' relation '{relation}' uses disallowed operation '{operation}'")]
    DpiDisallowedOperation {
        resource: String,
        relation: String,
        operation: String,
    },
}

impl From<zanzibar::error::Error> for Error {
    fn from(e: zanzibar::error::Error) -> Self {
        match e {
            zanzibar::error::Error::PolicyNotFound(s) => Error::PolicyNotFound(s),
            zanzibar::error::Error::PolicyAlreadyExists(s) => Error::PolicyAlreadyExists(s),
            zanzibar::error::Error::RelationNotFound { resource, relation } => {
                Error::RelationNotFound { resource, relation }
            }
            zanzibar::error::Error::ResourceNotFound(s) => Error::ResourceNotFound(s),
            zanzibar::error::Error::InvalidExpression(s) => Error::InvalidExpression(s),
            zanzibar::error::Error::InvalidPolicy(s) => Error::InvalidPolicy(s),
            zanzibar::error::Error::Serialization(s) => Error::Serialization(s),
            zanzibar::error::Error::InvalidDid(s) => Error::Storage(format!("invalid DID: {}", s)),
            zanzibar::error::Error::InvalidEntitySetReference { resource, relation } => {
                Error::InvalidEntitySetReference { resource, relation }
            }
            zanzibar::error::Error::SubjectRestrictionViolation { message } => {
                Error::SubjectRestrictionViolation { message }
            }
            zanzibar::error::Error::DpiMissingOwner { resource } => {
                Error::DpiMissingOwner { resource }
            }
            zanzibar::error::Error::DpiExpressionMissingOwner { resource, relation } => {
                Error::DpiExpressionMissingOwner { resource, relation }
            }
            zanzibar::error::Error::DpiDisallowedOperation {
                resource,
                relation,
                operation,
            } => Error::DpiDisallowedOperation {
                resource,
                relation,
                operation,
            },
            zanzibar::error::Error::InvalidRelationshipField { field, reason } => Error::Storage(
                format!("invalid relationship field '{}': {}", field, reason),
            ),
            _ => Error::Storage(e.to_string()),
        }
    }
}

impl Error {
    /// Create a storage read error with context.
    pub fn storage_read(operation: impl Into<String>, err: impl std::fmt::Display) -> Self {
        Self::StorageRead {
            operation: operation.into(),
            details: err.to_string(),
        }
    }

    /// Create a storage write error with context.
    pub fn storage_write(operation: impl Into<String>, err: impl std::fmt::Display) -> Self {
        Self::StorageWrite {
            operation: operation.into(),
            details: err.to_string(),
        }
    }

    /// Create a storage transaction error with context.
    pub fn storage_txn(operation: impl Into<String>, err: impl std::fmt::Display) -> Self {
        Self::StorageTransaction {
            operation: operation.into(),
            details: err.to_string(),
        }
    }

    /// Create a storage iteration error with context.
    pub fn storage_iter(operation: impl Into<String>, err: impl std::fmt::Display) -> Self {
        Self::StorageIteration {
            operation: operation.into(),
            details: err.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Error;

    #[test]
    fn json_errors_preserve_their_type() {
        let err = serde_json::from_str::<serde_json::Value>("{").unwrap_err();
        let error: Error = err.into();
        assert!(matches!(error, Error::Json(_)));
        assert!(error.to_string().starts_with("serialization error: "));
    }
}
