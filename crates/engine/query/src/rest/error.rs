//! Error types for REST operations.

use crate::error::QueryError;

/// Result type for REST operations.
pub type RestResult<T> = std::result::Result<T, RestError>;

/// Error type for REST operations.
#[derive(Debug, Clone, thiserror::Error)]
pub enum RestError {
    /// Collection not found.
    #[error("collection not found: {0}")]
    CollectionNotFound(String),
    /// Document not found.
    #[error("document not found: {0}")]
    DocumentNotFound(String),
    /// Invalid document ID format.
    #[error("invalid document ID: {0}")]
    InvalidDocId(String),
    /// Invalid input data.
    #[error("invalid input: {0}")]
    InvalidInput(String),
    /// Permission denied (ACP check failed).
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    /// A transaction conflicted and may be retried.
    #[error("{}", crate::executor::TXN_CONFLICT_MESSAGE)]
    TransactionConflict,
    /// Storage or execution error.
    #[error("internal error: {0}")]
    Internal(String),
}

impl RestError {
    pub fn collection_not_found(name: impl Into<String>) -> Self {
        Self::CollectionNotFound(name.into())
    }

    pub fn document_not_found(id: impl Into<String>) -> Self {
        Self::DocumentNotFound(id.into())
    }

    pub fn invalid_doc_id(id: impl Into<String>) -> Self {
        Self::InvalidDocId(id.into())
    }

    pub fn invalid_input(msg: impl Into<String>) -> Self {
        Self::InvalidInput(msg.into())
    }

    pub fn permission_denied(msg: impl Into<String>) -> Self {
        Self::PermissionDenied(msg.into())
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }
}

impl From<QueryError> for RestError {
    fn from(err: QueryError) -> Self {
        match err {
            QueryError::TransactionConflict(_) => Self::TransactionConflict,
            QueryError::Storage(source) if source.is_txn_conflict() => Self::TransactionConflict,
            // Not found errors
            QueryError::CollectionNotFound(name) => Self::CollectionNotFound(name),
            QueryError::DocumentNotFound(id) => Self::DocumentNotFound(id),
            // Invalid input errors (user-fixable, should be 400 Bad Request)
            QueryError::InvalidDocId(id) => Self::InvalidDocId(id),
            QueryError::InvalidMutationInput(msg) => Self::InvalidInput(msg),
            QueryError::Parse(msg) | QueryError::UnknownMutationCollection(msg) => {
                Self::InvalidInput(format!("parse error: {}", msg))
            }
            QueryError::InvalidFilter(msg) => {
                Self::InvalidInput(format!("invalid filter: {}", msg))
            }
            QueryError::FilterFieldNotSelected { field, collection } => {
                Self::InvalidInput(format!(
                    "filter field '{}' must be in select list for '{}'",
                    field, collection
                ))
            }
            QueryError::UnknownField(name) => Self::InvalidInput(name.to_string()),
            QueryError::TypeMismatch { expected, actual } => Self::InvalidInput(format!(
                "type mismatch: expected {}, got {}",
                expected, actual
            )),
            QueryError::RequiredFieldMissing(field) => {
                Self::InvalidInput(format!("required field missing: {}", field))
            }
            QueryError::InvalidAggregateTarget(msg) => {
                Self::InvalidInput(format!("invalid aggregate target: {}", msg))
            }
            // Permission errors (should be 403 Forbidden)
            QueryError::PermissionDenied(msg) => Self::PermissionDenied(msg),
            QueryError::AcpRegistrationFailed { doc_id, message } => Self::PermissionDenied(
                format!("ACP registration failed for '{}': {}", doc_id, message),
            ),
            // True internal errors (500 Internal Server Error)
            other => Self::Internal(other.to_string()),
        }
    }
}
