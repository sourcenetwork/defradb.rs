use thiserror::Error;

/// Errors that can occur in the database layer.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    #[error("storage error: {0}")]
    Storage(#[from] storage::Error),

    #[error("datastore error: {0}")]
    Datastore(#[from] datastore::Error),

    #[error("schema error: {0}")]
    Schema(#[from] schema::SchemaError),

    #[error("document error: {0}")]
    Document(#[from] document::Error),

    #[error("failed to deserialize document at key {key:?}: {source}")]
    DocumentAtKey {
        key: String,
        source: document::Error,
    },

    #[error(
        "this store holds values of the custom CRDT type `{crdt}`, which this node does not \
         register: register it with `NodeBuilder::with_crdt` before opening the store"
    )]
    UnregisteredStoredCrdt { crdt: String },

    #[error("collection '{0}' not found")]
    CollectionNotFound(String),

    #[error("collection not found. collection version: {0}")]
    CollectionVersionNotFound(String),

    #[error("index '{index}' not found on collection '{collection}'")]
    IndexNotFound { collection: String, index: String },

    #[error("collection version ID can't be empty")]
    CollectionVersionIDEmpty,

    #[error(
        "collection version {version_id} binds policy {policy_cid}, which this node does not hold: \
         a synced definition carries the policy by CID only, so activating it would serve the \
         collection with no policy"
    )]
    CollectionVersionPolicyNotHeld {
        version_id: String,
        policy_cid: String,
    },

    #[error("collection already exists. Name: {0}")]
    CollectionAlreadyExists(String),

    #[error("invalid patch: {0}")]
    InvalidPatch(String),

    #[error("document not found: {0}")]
    DocumentNotFound(String),

    #[error("invalid document: {0}")]
    InvalidDocument(String),

    #[error("invalid collection name: {0}")]
    InvalidCollectionName(String),

    #[error("database is closed")]
    DatabaseClosed,

    #[error("action already in progress. CollectionID: {collection_id}, Action: {action}")]
    ActionInProgress { collection_id: String, action: u16 },

    #[error("transaction not active")]
    TxnNotActive,

    #[error("an open reader, writer or iterator still holds the transaction")]
    TxnInUse,

    #[error("explicit transaction must use force_commit/force_discard")]
    ExplicitTxnMustUseForce,

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("{context}: {source}")]
    CollectionSchemaJson {
        context: String,
        source: serde_json::Error,
    },

    #[error("{context}: {source}")]
    LensConfigJson {
        context: String,
        source: serde_json::Error,
    },

    #[error("{context}: {source}")]
    TextDecode {
        context: String,
        source: std::string::FromUtf8Error,
    },

    #[error("query error: {0}")]
    Query(#[from] query::error::QueryError),

    #[error("transaction not found: {0}")]
    TransactionNotFound(String),

    #[error("transaction registry lock poisoned: {0}")]
    LockPoisoned(String),

    #[error(
        "cache update failed after successful commit for collection '{0}' - call reload_cache() or restart to recover"
    )]
    CacheUpdateFailedAfterCommit(String),

    #[error("{0}")]
    Other(String),

    #[error("unsafe policy transition blocked: {0}")]
    UnsafePolicyTransition(String),

    #[error("acp error: {0}")]
    Acp(String),

    #[error("not authorized to perform operation. Permission: {permission}")]
    NotAuthorized { permission: String },

    #[error("lens error: {0}")]
    Lens(String),

    #[error("json patch error: {0}")]
    JsonPatch(#[from] crate::definition::patch::json::JsonPatchError),
}

impl From<acp::Error> for Error {
    fn from(err: acp::Error) -> Self {
        Error::Acp(err.to_string())
    }
}

impl From<lens::Error> for Error {
    fn from(err: lens::Error) -> Self {
        Error::Lens(err.to_string())
    }
}

impl From<crate::index::Error> for Error {
    fn from(err: crate::index::Error) -> Self {
        match err {
            crate::index::Error::Storage(e) => Error::Storage(e),
            crate::index::Error::InvalidDocument(msg) => Error::InvalidDocument(msg),
            other @ (crate::index::Error::VectorEntryPointNotFound { .. }
            | crate::index::Error::DiskAnnCodeLength { .. }
            | crate::index::Error::DiskAnnCodeSlots { .. }) => Error::Other(other.to_string()),
            other @ crate::index::Error::VectorDimensionMismatch { .. } => {
                Error::InvalidDocument(other.to_string())
            }
            crate::index::Error::Other(msg) => Error::Other(msg),
        }
    }
}

impl Error {
    pub fn other(message: impl Into<String>) -> Self {
        Self::Other(message.into())
    }

    pub fn is_txn_retry_exhausted(&self) -> bool {
        matches!(
            self,
            Error::Storage(storage::corekv::Error::TxnRetryExhausted(_))
        ) || matches!(
            self,
            Error::Datastore(datastore::Error::Storage(
                storage::corekv::Error::TxnRetryExhausted(_)
            ))
        )
    }

    pub fn is_txn_conflict(&self) -> bool {
        matches!(self, Error::Storage(source) if source.is_txn_conflict())
            || matches!(
                self,
                Error::Datastore(datastore::Error::Storage(source))
                    if source.is_txn_conflict()
            )
    }

    pub fn is_unique_constraint_violation(&self) -> bool {
        matches!(self, Error::Storage(source) if source.is_unique_constraint_violation())
    }

    pub fn is_storage_full(&self) -> bool {
        match self {
            Self::Storage(source) | Self::Datastore(datastore::Error::Storage(source)) => {
                source.is_storage_full()
            }
            _ => false,
        }
    }

    pub fn document_at_key(key: &[u8], source: document::Error) -> Self {
        Self::DocumentAtKey {
            key: String::from_utf8_lossy(key).into_owned(),
            source,
        }
    }

    pub fn collection_schema_json(context: impl Into<String>, source: serde_json::Error) -> Self {
        Self::CollectionSchemaJson {
            context: context.into(),
            source,
        }
    }

    pub fn text_decode(context: impl Into<String>, source: std::string::FromUtf8Error) -> Self {
        Self::TextDecode {
            context: context.into(),
            source,
        }
    }

    pub fn lens_config_json(context: impl Into<String>, source: serde_json::Error) -> Self {
        Self::LensConfigJson {
            context: context.into(),
            source,
        }
    }
}

pub fn index_write_query_error(operation: &str, error: Error) -> query::error::QueryError {
    if error.is_unique_constraint_violation() {
        query::error::QueryError::execution(storage::corekv::UNIQUE_CONSTRAINT_VIOLATION_MESSAGE)
    } else {
        storage_query_error(error, &format!("{operation} error"))
    }
}

pub fn commit_query_error(error: Error) -> query::error::QueryError {
    let message = format!("commit error: {error}");
    if error.is_txn_retry_exhausted() {
        query::error::QueryError::transaction_retry_exhausted(message)
    } else if error.is_txn_conflict() {
        query::error::QueryError::transaction_conflict(message)
    } else {
        storage_query_error(error, "commit error")
    }
}

fn storage_query_error(error: Error, context: &str) -> query::error::QueryError {
    match error {
        Error::Storage(storage::Error::Io(cause))
        | Error::Datastore(datastore::Error::Storage(storage::Error::Io(cause))) => {
            query::error::QueryError::Storage(storage::Error::Io(cause.with_context(context)))
        }
        other => query::error::QueryError::execution(format!("{context}: {other}")),
    }
}

pub(crate) fn transaction_error(error: Error) -> query::error::TransactionError {
    match error {
        Error::Storage(cause @ storage::Error::Io(_))
        | Error::Datastore(datastore::Error::Storage(cause @ storage::Error::Io(_))) => {
            query::error::TransactionError::Storage(cause)
        }
        other => query::error::TransactionError::execution(other.to_string()),
    }
}

/// Result type for database operations.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod storage_failure_tests {
    use super::*;

    #[test]
    fn commit_and_index_errors_keep_the_storage_full_wire_code() {
        for datastore in [false, true] {
            for commit in [false, true] {
                let cause = storage::Error::from(std::io::Error::new(
                    std::io::ErrorKind::StorageFull,
                    "full",
                ));
                let error = if datastore {
                    Error::Datastore(datastore::Error::Storage(cause))
                } else {
                    Error::Storage(cause)
                };
                assert!(error.is_storage_full());
                let error = if commit {
                    commit_query_error(error)
                } else {
                    index_write_query_error("index write", error)
                };
                let response = query::QueryResponseError::from_query_error(error);
                assert_eq!(
                    response.code(),
                    Some(query::executor::STORAGE_FULL_ERROR_CODE)
                );
                assert!(response.message.contains(if commit {
                    "commit error"
                } else {
                    "index write error"
                }));
            }
        }
    }
}
