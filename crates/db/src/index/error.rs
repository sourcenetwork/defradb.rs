//! Errors raised by index maintenance.

use thiserror::Error;

/// Result alias used throughout the index_manager module.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur during index operations.
#[derive(Debug, Error)]
pub enum Error {
    #[error("storage error: {0}")]
    Storage(#[from] storage::Error),

    #[error("invalid document: {0}")]
    InvalidDocument(String),

    #[error("vector index entry point {entry_point} is not in the store")]
    VectorEntryPointNotFound { entry_point: u64 },

    #[error("vector index holds {indexed}-dimension vectors, got {got}")]
    VectorDimensionMismatch { indexed: usize, got: usize },

    #[error("DiskANN code of node {id} is {got} bytes, the index's codes are {expected}")]
    DiskAnnCodeLength {
        id: u64,
        expected: usize,
        got: usize,
    },

    #[error("DiskANN resident codes outgrow a u32 slot at node {id}")]
    DiskAnnCodeSlots { id: u64 },

    #[error("{0}")]
    Other(String),
}

impl Error {
    /// Whether this is a unique-index rejection, as opposed to a transient
    /// storage failure that should retry.
    pub fn is_unique_constraint_violation(&self) -> bool {
        matches!(self, Error::Storage(source) if source.is_unique_constraint_violation())
    }
}
