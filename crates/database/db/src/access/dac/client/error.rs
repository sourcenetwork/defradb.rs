/// Why a document relationship operation was refused or failed. HTTP, the
/// CLI, FFI and the embedded node all report these texts unchanged; each
/// refusal Go's DefraDB has carries Go's text.
#[derive(Debug, thiserror::Error)]
pub enum DocAcpError {
    /// The node refused the caller, or its storage failed.
    #[error(transparent)]
    Db(#[from] crate::Error),
    /// The node runs without document ACP, or its policies could not be read.
    #[error(transparent)]
    DacPolicy(#[from] crate::DacPolicyError),
    /// The document ACP refused or failed.
    #[error(transparent)]
    Acp(#[from] acp::Error),
    /// A step inside the transaction failed.
    #[error(transparent)]
    Query(#[from] query::QueryError),
    /// The transaction could not be begun, run or committed.
    #[error(transparent)]
    Transaction(#[from] query::TransactionError),
    /// An identity string is not a DID.
    #[error("invalid identity DID: {0}")]
    InvalidIdentityDid(#[source] identity::Error),
    /// Looking a collection up failed.
    #[error("failed to get collection '{collection}': {source}")]
    CollectionLookup {
        /// The collection that was looked up.
        collection: String,
        /// Why the lookup failed.
        #[source]
        source: crate::Error,
    },
    /// The named collection does not exist.
    #[error("collection '{0}' does not exist")]
    CollectionMissing(String),
    /// The collection has no policy to manage relationships on.
    #[error("operation requires ACP, but collection has no policy")]
    CollectionWithoutPolicy,
    /// A relationship change named no collection.
    #[error("collection name can't be empty")]
    CollectionNameEmpty,
    /// A relationship change is missing its identity, document, relation or
    /// target (Go: `ErrMissingReqArgToAddRelationship` and its delete twin).
    #[error("missing a required argument needed to {0} doc actor relationship.")]
    RelationshipArgumentMissing(&'static str),
    /// The policy side of a relationship change failed.
    #[error("failed to {verb} document actor relationship with acp: {source}")]
    RelationshipAcp {
        /// `add` or `delete`.
        verb: &'static str,
        /// What failed.
        #[source]
        source: Box<DocAcpError>,
    },
    /// A relationship target did not parse.
    #[error("invalid target: {0}")]
    InvalidTarget(#[source] acp::Error),
    /// The named policy does not exist.
    #[error("policy '{0}' not found")]
    PolicyNotFound(String),
    /// The policy has no such resource.
    #[error("resource '{0}' not found in policy")]
    ResourceNotFound(String),
    /// The policy resource has no such relation.
    #[error("relation '{relation}' not found in policy resource '{resource}'")]
    RelationNotFound {
        /// The missing relation.
        relation: String,
        /// The resource that was searched.
        resource: String,
    },
    /// Owner relations are managed by the system only.
    #[error("OPERATION_FORBIDDEN: cannot add owner relation")]
    OwnerRelationAdd,
    /// Owner relations are managed by the system only.
    #[error("OPERATION_FORBIDDEN: cannot delete owner relation")]
    OwnerRelationDelete,
}
