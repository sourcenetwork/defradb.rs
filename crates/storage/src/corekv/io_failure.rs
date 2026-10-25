use std::io;

/// Cloneable I/O facts retained across storage and GraphQL boundaries.
/// A failure does not establish whether the affected commit reached disk.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct IoFailure {
    kind: io::ErrorKind,
    raw_os_error: Option<i32>,
    message: String,
}

impl IoFailure {
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
        }
    }
}
