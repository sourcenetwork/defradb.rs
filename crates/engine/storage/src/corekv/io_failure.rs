use std::io;

/// Cloneable I/O facts retained across storage and GraphQL boundaries.
/// A failure does not establish whether the affected commit reached disk.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct IoFailure {
    kind: io::ErrorKind,
    raw_os_error: Option<i32>,
    message: String,
    /// What the commit this failure ended did, when it ended one: it may
    /// have landed when the engine raised it while it made the commit
    /// durable (`defra.txn.commit_outcome.indeterminate`), and nothing
    /// landed when it came before the storage commit
    /// (`defra.txn.commit_outcome.not_committed`).
    commit: Option<defra_core::CommitOutcome>,
}

impl IoFailure {
    /// What the commit this failure ended did; `None` outside a commit.
    pub fn commit_outcome(&self) -> Option<defra_core::CommitOutcome> {
        self.commit
    }

    /// This failure, raised while the engine made a commit durable.
    pub fn at_commit(mut self) -> Self {
        self.commit = Some(defra_core::CommitOutcome::Indeterminate);
        self
    }

    /// This failure, ending a commit before the engine made any of it
    /// durable. A failure already raised at the commit keeps its class.
    pub fn before_commit(mut self) -> Self {
        self.commit
            .get_or_insert(defra_core::CommitOutcome::NotCommitted);
        self
    }

    pub fn kind(&self) -> io::ErrorKind {
        self.kind
    }

    pub fn raw_os_error(&self) -> Option<i32> {
        self.raw_os_error
    }

    pub fn with_context(mut self, context: impl std::fmt::Display) -> Self {
        self.message = format!("{context}: {}", self.message);
        self
    }
}

impl From<io::Error> for IoFailure {
    fn from(error: io::Error) -> Self {
        Self {
            kind: error.kind(),
            raw_os_error: error.raw_os_error(),
            message: error.to_string(),
            commit: None,
        }
    }
}
