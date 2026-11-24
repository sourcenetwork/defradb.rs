//! The note of requests resolved by someone other than the caller that
//! journaled them.
//!
//! The drainer stores a request's digest with its changes before the worker
//! journals it, and the request's transaction hash after. A crash between
//! the two leaves a change that names its request by digest alone. Whoever
//! then finds the request in the journal (the provider at start, another
//! command before its own) sends it, waits for its verified receipt, and
//! writes the outcome here under the digest before the journal lets the
//! request go. The drainer reads it back by the digest (`AcpOutbox.tla`,
//! JRecover and JDecide with `Note = TRUE`).
//!
//! The note keeps the latest [`MAX_NOTED`] outcomes. One pushed out is a
//! note that can no longer be read: the drainer then sends its change again.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::provider::ProviderError;

/// How many outcomes the note keeps. Only a request journaled and not
/// resolved by its own caller is noted, which takes a crash or a dropped
/// wait each.
pub const MAX_NOTED: usize = 256;

const NOTE_VERSION: u8 = 1;
const MAX_NOTE_BYTES: usize = 128 * 1024;

/// What a resolved request came to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Noted {
    pub digest: String,
    pub tx: String,
    /// The worker sequence the request was signed with.
    pub sequence: u64,
    pub success: bool,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stored {
    version: u8,
    outcomes: Vec<Noted>,
}

/// The outcomes noted in one worker's directory.
pub struct OutcomeNote {
    path: PathBuf,
    outcomes: Vec<Noted>,
}

fn failure(cause: impl std::fmt::Display) -> ProviderError {
    ProviderError::Unavailable(format!("owed request note: {cause}"))
}

impl OutcomeNote {
    /// The note kept in `directory`, empty when there is none yet.
    pub fn open(directory: &Path) -> Result<Self, ProviderError> {
        let path = directory.join("owed-outcomes.json");
        let outcomes = match File::open(&path) {
            Ok(file) => {
                let mut bytes = Vec::new();
                file.take(MAX_NOTE_BYTES as u64 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(failure)?;
                if bytes.len() > MAX_NOTE_BYTES {
                    return Err(failure("the note exceeds its byte limit"));
                }
                let stored: Stored = serde_json::from_slice(&bytes).map_err(failure)?;
                if stored.version != NOTE_VERSION {
                    return Err(failure(format!("unknown version {}", stored.version)));
                }
                stored.outcomes
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(failure(error)),
        };
        Ok(Self { path, outcomes })
    }

    /// The outcome noted for the request whose digest is `digest`, among
    /// those signed with a sequence from `floor` on. A request with the same
    /// bytes sent before (a delegation reused for the same command) was
    /// signed with a lower one, and its outcome is not the caller's.
    ///
    /// A caller that recorded the request's transaction hash `tx` gets the
    /// outcome of that transaction only: another request with the same bytes
    /// is another transaction.
    pub fn get(&self, digest: &str, tx: Option<&str>, floor: Option<u64>) -> Option<&Noted> {
        self.outcomes.iter().rev().find(|noted| {
            noted.digest == digest
                && match tx {
                    Some(tx) => noted.tx == tx,
                    None => floor.is_none_or(|floor| noted.sequence >= floor),
                }
        })
    }

    /// Note `outcome` durably, in place of an earlier outcome of the same
    /// transaction, pushing out the oldest past [`MAX_NOTED`]. An outcome of
    /// another transaction with the same bytes stays: its caller may name it
    /// by hash. Blocking IO: the write is synced before this returns.
    pub fn record(&mut self, outcome: Noted) -> Result<(), ProviderError> {
        let mut outcomes = self.outcomes.clone();
        outcomes.retain(|noted| noted.tx != outcome.tx);
        outcomes.push(outcome);
        let excess = outcomes.len().saturating_sub(MAX_NOTED);
        outcomes.drain(..excess);
        let stored = Stored {
            version: NOTE_VERSION,
            outcomes,
        };
        let bytes = serde_json::to_vec(&stored).map_err(failure)?;
        persist(&self.path, &bytes)?;
        self.outcomes = stored.outcomes;
        Ok(())
    }
}

impl OutcomeNote {
    /// Note that the transaction `tx`, if it is noted, came to `success` on
    /// the deployment's history now: a switch of its active device voided
    /// the block that held it, and it was sent again
    /// (`follow::reconcile`). A note says what the deployment holds, not
    /// what a void block held. Blocking IO.
    pub fn revise(&mut self, tx: &str, success: bool) -> Result<(), ProviderError> {
        let Some(noted) = self.outcomes.iter().find(|noted| noted.tx == tx).cloned() else {
            return Ok(());
        };
        if noted.success == success {
            return Ok(());
        }
        self.record(Noted { success, ..noted })
    }
}

/// Replace `path` with `bytes` so a crash leaves the old file or the new
/// one, never part of either.
fn persist(path: &Path, bytes: &[u8]) -> Result<(), ProviderError> {
    let directory = path
        .parent()
        .ok_or_else(|| failure("the note has no directory"))?;
    let staging = path.with_extension("json.new");
    let mut file = File::create(&staging).map_err(failure)?;
    file.write_all(bytes).map_err(failure)?;
    file.sync_all().map_err(failure)?;
    drop(file);
    fs::rename(&staging, path).map_err(failure)?;
    #[cfg(unix)]
    File::open(directory)
        .and_then(|directory| directory.sync_all())
        .map_err(failure)?;
    #[cfg(not(unix))]
    let _ = directory;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noted(index: usize) -> Noted {
        Noted {
            digest: format!("0xd{index}"),
            tx: format!("0xt{index}"),
            sequence: index as u64,
            success: index.is_multiple_of(2),
        }
    }

    #[test]
    fn a_noted_outcome_is_revised_to_what_a_replay_came_to() {
        let directory = tempfile::tempdir().unwrap();
        let mut note = OutcomeNote::open(directory.path()).unwrap();
        note.record(noted(0)).unwrap();
        assert!(note.get("0xd0", Some("0xt0"), None).unwrap().success);
        note.revise("0xt0", false).unwrap();
        note.revise("0xunknown", false).unwrap();
        let note = OutcomeNote::open(directory.path()).unwrap();
        assert!(!note.get("0xd0", Some("0xt0"), None).unwrap().success);
        assert_eq!(note.get("0xd0", None, None).unwrap().sequence, 0);
    }

    #[test]
    fn an_outcome_survives_a_reopen_and_the_oldest_is_pushed_out() {
        let directory = tempfile::tempdir().unwrap();
        let mut note = OutcomeNote::open(directory.path()).unwrap();
        assert!(note.get("0xd0", None, None).is_none());
        for index in 0..=MAX_NOTED {
            note.record(noted(index)).unwrap();
        }
        let reopened = OutcomeNote::open(directory.path()).unwrap();
        assert!(
            reopened.get("0xd0", None, None).is_none(),
            "the oldest was pushed out"
        );
        assert_eq!(reopened.get("0xd1", None, None), Some(&noted(1)));
        assert_eq!(
            reopened.get(&format!("0xd{MAX_NOTED}"), None, None),
            Some(&noted(MAX_NOTED))
        );
    }

    /// A request with the same bytes as one sent earlier (a delegation
    /// reused for the same command) is not decided by the earlier one's
    /// outcome: only an outcome signed from the caller's floor on counts.
    #[test]
    fn an_outcome_of_an_earlier_request_with_the_same_bytes_is_not_the_callers() {
        let directory = tempfile::tempdir().unwrap();
        let mut note = OutcomeNote::open(directory.path()).unwrap();
        let earlier = Noted {
            sequence: 3,
            success: true,
            ..noted(1)
        };
        note.record(earlier.clone()).unwrap();
        assert_eq!(note.get("0xd1", None, Some(3)), Some(&earlier));
        assert_eq!(note.get("0xd1", None, Some(4)), None);

        let later = Noted {
            tx: "0xt9".into(),
            sequence: 9,
            success: false,
            ..noted(1)
        };
        note.record(later.clone()).unwrap();
        assert_eq!(note.get("0xd1", None, Some(4)), Some(&later));
    }

    /// A caller that recorded its request's hash gets that transaction's
    /// outcome, whether a later request with the same bytes was noted after
    /// it or not yet at all.
    #[test]
    fn a_recorded_hash_reads_its_own_outcome_and_no_other() {
        let directory = tempfile::tempdir().unwrap();
        let mut note = OutcomeNote::open(directory.path()).unwrap();
        let earlier = Noted {
            sequence: 3,
            success: true,
            ..noted(1)
        };
        note.record(earlier.clone()).unwrap();
        assert_eq!(note.get("0xd1", Some("0xt9"), None), None);

        let later = Noted {
            tx: "0xt9".into(),
            sequence: 9,
            success: false,
            ..noted(1)
        };
        note.record(later.clone()).unwrap();
        let reopened = OutcomeNote::open(directory.path()).unwrap();
        assert_eq!(reopened.get("0xd1", Some("0xt1"), None), Some(&earlier));
        assert_eq!(reopened.get("0xd1", Some("0xt9"), None), Some(&later));
        assert_eq!(reopened.get("0xd1", Some("0xt5"), None), None);
    }

    #[test]
    fn a_request_noted_again_keeps_its_latest_outcome_once() {
        let directory = tempfile::tempdir().unwrap();
        let mut note = OutcomeNote::open(directory.path()).unwrap();
        note.record(noted(1)).unwrap();
        let again = Noted {
            success: true,
            ..noted(1)
        };
        note.record(again.clone()).unwrap();
        let reopened = OutcomeNote::open(directory.path()).unwrap();
        assert_eq!(reopened.get("0xd1", None, None), Some(&again));
        assert_eq!(reopened.outcomes.len(), 1);
    }

    #[test]
    fn a_note_of_another_version_is_refused() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(
            directory.path().join("owed-outcomes.json"),
            br#"{"version":2,"outcomes":[]}"#,
        )
        .unwrap();
        assert!(OutcomeNote::open(directory.path()).is_err());
    }
}
