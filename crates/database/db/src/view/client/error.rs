/// Why a view operation was refused. HTTP, the CLI, FFI and the embedded node
/// all report these texts unchanged.
#[derive(Debug, thiserror::Error)]
pub enum ViewError {
    /// The node refused the caller, or its storage failed.
    #[error(transparent)]
    Db(#[from] crate::Error),
    /// The caller's acting identity is malformed.
    #[error("malformed acting identity: {0}")]
    InvalidIdentity(#[source] identity::Error),
    /// The target SDL did not parse.
    #[error("view SDL parse error: {0}")]
    SchemaParse(#[source] query::QueryError),
    /// The source query did not parse.
    #[error("view query parse error: {0}")]
    QueryParse(#[source] query::QueryError),
    /// The source query selects nothing.
    #[error("invalid view query: no selections found")]
    QueryEmpty,
    /// The source query selects more than one source type.
    #[error("invalid view query: multiple selections found")]
    QueryMultipleSources,
    /// The lens transform CID is empty, which Go's `cid.Decode` refuses.
    #[error("cid too short")]
    TransformCidEmpty,
    /// The lens transform CID names no lens the node holds.
    #[error("lens CID not found: {0}")]
    TransformMissing(String),
    /// A `@policy` names a resource its policy cannot guard a collection with.
    #[error("schema policy validation error: {0}")]
    Policy(String),
    /// A `@policy` could not be checked.
    #[error(transparent)]
    DacPolicy(crate::DacPolicyError),
    /// A downsample definition is invalid.
    #[error("invalid downsample definition: {0}")]
    InvalidDownsample(#[source] crate::Error),
    /// The view collections could not be created.
    #[error("create view collection error: {0}")]
    Create(#[source] crate::Error),
    /// The materialized views could not be built or refreshed.
    #[error("refresh materialized views error: {0}")]
    Refresh(#[source] crate::Error),
    /// The downsample collections could not be bootstrapped.
    #[error("bootstrap downsample collections error: {0}")]
    DownsampleBootstrap(#[source] crate::Error),
}

impl ViewError {
    /// A failed create, naming a missing lens transform as Go does.
    pub(super) fn create(error: crate::Error) -> Self {
        match error {
            crate::Error::ViewTransformMissing(cid) => Self::TransformMissing(cid),
            error => Self::Create(error),
        }
    }

    /// A refused `@policy`.
    pub(super) fn policy(error: crate::DacPolicyError) -> Self {
        match error {
            crate::DacPolicyError::ResourceInterface(message) => Self::Policy(message),
            error => Self::DacPolicy(error),
        }
    }
}
