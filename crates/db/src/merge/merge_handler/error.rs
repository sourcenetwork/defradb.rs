//! Error types for database merge operations.

use cid::Cid;
use defra_core::merge::MergeErrorDisposition;

/// A rejected history attempt and the exact cursor incarnation it observed.
#[derive(Debug, thiserror::Error)]
#[error("history {root} rejected: {reason}")]
pub struct HistoryRejection {
    pub(super) root: Cid,
    pub(super) context: [u8; 32],
    pub(super) generation: [u8; 16],
    pub(super) reason: String,
}

/// Error type for database merge operations.
#[derive(Debug, thiserror::Error)]
pub enum MergeError {
    /// Failed to decode block from DAG-CBOR.
    #[error("block decode failed: {0}")]
    BlockDecode(String),

    /// Unsupported CRDT delta type.
    #[error("unsupported delta type: {0}")]
    UnsupportedDelta(String),

    /// Missing metadata during non-recovery operation.
    #[error("missing metadata: {0}")]
    MissingMetadata(String),

    /// Explicit replay authorization does not apply to the supplied block.
    #[error("invalid explicit replay authorization: {0}")]
    InvalidReplayAuthorization(String),

    /// CRDT merge failed.
    #[error("merge failed: {0}")]
    MergeFailed(String),

    /// A merge would change an `@immutable` field. This is a deterministic
    /// rejection of the block's content, not a transient failure: the offending
    /// block must be marked terminal (skipped) so it is not retried forever.
    #[error("immutable field rejected: {0}")]
    ImmutableFieldChanged(String),

    /// A merge would violate a unique-index constraint. Like
    /// `ImmutableFieldChanged`, this is a deterministic rejection of the
    /// block's content (the same document content will violate the same
    /// constraint on every replay), not a transient failure.
    #[error("unique constraint violation during merge: {0}")]
    UniqueConstraintViolation(String),

    /// A single-block `_in_txn` merge rejected its content after staging
    /// writes; the enclosing attempt is discarded and the original reason is
    /// returned as the merge outcome.
    #[error("{0}")]
    RetryRejected(String),

    /// Database error.
    #[error("database error: {0}")]
    Database(#[from] crate::error::Error),

    /// Storage error.
    #[error("storage error: {0}")]
    Storage(String),

    /// KMS lookup or authorization error.
    #[error("KMS error: {0}")]
    Kms(#[from] kms::Error),

    /// The current merge attempt needs this DEK from KMS. The retry driver
    /// resolves it outside the engine attempt and starts a fresh attempt.
    #[error("merge requires prepared encryption key for {0}")]
    NeedsDecryptionKey(Cid),

    /// The batch retry driver needs this block's DEK resolved outside the
    /// engine-owned attempt. The index selects its exact authorization context.
    #[error(
        "batch merge requires prepared encryption key for block {block_index}: {encryption_cid}"
    )]
    NeedsBatchDecryptionKey {
        block_index: usize,
        encryption_cid: Cid,
    },

    /// Block signature verification failed — block MUST be rejected.
    #[error("block signature verification failed for cid={cid}: {reason}")]
    SignatureVerificationFailed { cid: Cid, reason: String },

    /// The caller must discard its shared transaction and resume per root.
    #[error("history requires a resumable merge turn")]
    HistoryRequired,

    /// Discard the data attempt before recording a history's rejection.
    #[error(transparent)]
    HistoryAborted(Box<HistoryRejection>),

    /// DAG traversal depth limit exceeded.
    ///
    /// A maliciously crafted deeply-nested DAG could otherwise consume unbounded resources.
    #[error("DAG merge depth limit exceeded at cid={cid} depth={depth}")]
    DepthExceeded {
        /// CID that triggered the depth check.
        cid: Cid,
        /// Depth at which the limit was hit.
        depth: usize,
    },
}

impl MergeError {
    /// Construct a `DepthExceeded` error.
    pub(crate) fn depth_exceeded(cid: &Cid, depth: usize) -> Self {
        MergeError::DepthExceeded { cid: *cid, depth }
    }

    /// Classify failures that cannot change while replaying the same block.
    pub(crate) fn disposition(&self) -> MergeErrorDisposition {
        match self {
            Self::BlockDecode(_)
            | Self::UnsupportedDelta(_)
            | Self::InvalidReplayAuthorization(_)
            | Self::ImmutableFieldChanged(_)
            | Self::UniqueConstraintViolation(_)
            | Self::RetryRejected(_)
            | Self::SignatureVerificationFailed { .. }
            | Self::DepthExceeded { .. } => MergeErrorDisposition::Terminal,
            Self::MissingMetadata(_)
            | Self::MergeFailed(_)
            | Self::Database(_)
            | Self::Storage(_)
            | Self::Kms(_)
            | Self::NeedsDecryptionKey(_)
            | Self::NeedsBatchDecryptionKey { .. }
            | Self::HistoryRequired
            | Self::HistoryAborted(_) => MergeErrorDisposition::Retryable,
        }
    }
}

/// A merge the worker pool could not run (it was shutting down, or the task
/// panicked) stays retryable: nothing of it was applied.
impl From<workers::Error> for MergeError {
    fn from(error: workers::Error) -> Self {
        MergeError::Storage(format!("the merge did not run: {error}"))
    }
}
