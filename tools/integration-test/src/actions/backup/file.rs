//! The file a backup is written to or read from, as Go's harness gives one
//! to each export and import: in a fresh directory, or under a directory that
//! does not exist when the action wants the file missing.

use std::path::PathBuf;

use tempfile::TempDir;

use crate::client_type::ClientType;
use crate::scenario::Failure;
use crate::test_case::State;

pub(super) struct BackupFile {
    _dir: TempDir,
    pub path: PathBuf,
}

impl BackupFile {
    pub fn new(missing: bool) -> Result<Self, Failure> {
        let dir = tempfile::tempdir()
            .map_err(|e| Failure::operation(format!("could not make a backup directory: {e}")))?;
        let path = match missing {
            true => dir.path().join("missing").join("backup.json"),
            false => dir.path().join("backup.json"),
        };
        Ok(Self { _dir: dir, path })
    }

    /// Write `content` where the node would have, failing as Go does when the
    /// directory is missing.
    pub fn write(&self, content: &str) -> Result<(), Failure> {
        std::fs::write(&self.path, content)
            .map_err(|e| Failure::operation(format!("failed to create file: {e}")))
    }

    pub fn read(&self) -> Result<String, Failure> {
        std::fs::read_to_string(&self.path)
            .map_err(|e| Failure::operation(format!("failed to open file: {e}")))
    }

    pub fn as_str(&self) -> &str {
        self.path.to_str().unwrap_or_default()
    }
}

/// A Go node reads and writes a backup on its own host from a path in the
/// request, which the HTTP client does not send.
pub(super) fn refuse_go(state: &State, op: &str) -> Result<(), Failure> {
    match state.client_type() {
        ClientType::GoHttp => Err(Failure::unsupported(format!(
            "{op} on a Go node: it takes a file path the HTTP client does not send"
        ))),
        _ => Ok(()),
    }
}
