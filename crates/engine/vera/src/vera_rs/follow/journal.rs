//! The submissions this node saw confirmed, kept so a later term can be
//! checked against them.
//!
//! A takeover voids the blocks the replaced device finalized above its cut,
//! and with them every submission they held. The node finds which of its own
//! it lost by asking the deployment for each one's receipt, newest first:
//! blocks are lost from one height up, so the lost submissions are always the
//! newest ones (`proofs/tla/FollowDeployment.tla`). It sends each lost one
//! again as it was signed, which also brings the deployment's count of the
//! worker's requests back to the worker's own.
//!
//! The journal keeps the newest [`MAX_CONFIRMATIONS`] submissions and at
//! most [`MAX_CONFIRMATION_BYTES`] of their signed requests. Pruning an entry
//! records it as the floor. A term that voided the floor may have voided
//! older submissions the journal no longer has: [`Journal::covers`] is then
//! false and the node refuses to go on (it fails closed) instead of taking
//! them as kept. A journal started for a worker that had already signed
//! requests begins with a floor nothing can check, for the same reason.
//!
//! On disk it is a directory: one file per entry, named by its sequence, and
//! `meta.json` with the floor, the term last checked, the effects owed again
//! and not yet handed over, and why the node failed closed, if it did. Each
//! file is written whole and synced before it replaces the old one. Opening
//! and every write do blocking I/O.

use std::collections::VecDeque;
use std::fs::File;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use alloy_primitives::B256;
use serde::{Deserialize, Serialize};

use super::effect::Lost;

/// The most confirmations the journal keeps.
pub(crate) const MAX_CONFIRMATIONS: usize = 4096;

/// The most bytes of signed requests the journal keeps. The newest entry is
/// kept whatever its size.
pub(crate) const MAX_CONFIRMATION_BYTES: usize = 64 << 20;

const META: &str = "meta.json";
const ENTRY_SUFFIX: &str = ".entry";

/// Why the journal cannot be used.
#[derive(Debug, thiserror::Error)]
pub enum JournalError {
    #[error("confirmation journal {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("confirmation journal {path}: {source}")]
    Format {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("confirmation journal {0}: entries are not one run of sequences")]
    Broken(PathBuf),
}

/// One confirmed submission.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Confirmation {
    /// The worker's sequence the request was signed with.
    pub sequence: u64,
    /// The submission's identifier.
    pub submission: B256,
    /// The height it was last confirmed at.
    pub height: u64,
    /// The request exactly as signed.
    #[serde(with = "hex_bytes")]
    pub wire: Vec<u8>,
}

/// The oldest point the journal can vouch for.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum Floor {
    /// The newest entry pruned: every older submission is kept wherever this
    /// one is.
    Pruned { sequence: u64, submission: B256 },
    /// The worker had signed requests before the journal began.
    Unjournaled { before: u64 },
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Meta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    floor: Option<Floor>,
    #[serde(default)]
    handled_term: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    undelivered: Vec<Lost>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    closed: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    replaying: Option<Replaying>,
}

/// A replay begun and not finished: the term it checks, and the first
/// sequence it sends again. Every entry from there on is sent again on the
/// next check, whatever the deployment holds by then: a refused replay is
/// held, and only this mark still says its effect is lost.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Replaying {
    pub term: u64,
    pub from: u64,
}

/// What the journal knows of one entry without reading it.
#[derive(Clone, Copy, Debug)]
struct Indexed {
    sequence: u64,
    bytes: usize,
}

pub(crate) struct Journal {
    directory: PathBuf,
    meta: Meta,
    entries: VecDeque<Indexed>,
    bytes: usize,
    /// The most entries kept: [`MAX_CONFIRMATIONS`], less in tests.
    keep: usize,
}

impl Journal {
    /// Opens the journal in `directory`, creating it for a worker whose next
    /// sequence is `next_sequence`.
    pub(crate) fn open(directory: &Path, next_sequence: u64) -> Result<Self, JournalError> {
        let io = |source| JournalError::Io {
            path: directory.to_path_buf(),
            source,
        };
        let created = !directory.join(META).exists();
        std::fs::create_dir_all(directory).map_err(io)?;
        let mut entries = Vec::new();
        for item in std::fs::read_dir(directory).map_err(io)? {
            let item = item.map_err(io)?;
            let name = item.file_name();
            let Some(sequence) = name
                .to_str()
                .and_then(|name| name.strip_suffix(ENTRY_SUFFIX))
                .and_then(|digits| digits.parse::<u64>().ok())
            else {
                continue;
            };
            let bytes = item.metadata().map_err(io)?.len() as usize;
            entries.push(Indexed { sequence, bytes });
        }
        entries.sort_by_key(|entry| entry.sequence);
        if entries
            .windows(2)
            .any(|pair| pair[1].sequence != pair[0].sequence + 1)
        {
            return Err(JournalError::Broken(directory.to_path_buf()));
        }
        let mut journal = Self {
            directory: directory.to_path_buf(),
            meta: Meta::default(),
            bytes: entries.iter().map(|entry| entry.bytes).sum(),
            entries: entries.into(),
            keep: MAX_CONFIRMATIONS,
        };
        if created {
            journal.meta.floor = (next_sequence > 0).then_some(Floor::Unjournaled {
                before: next_sequence,
            });
            journal.write_meta()?;
        } else {
            journal.meta = journal.read(&directory.join(META))?;
        }
        Ok(journal)
    }

    /// Keeps `confirmation`, replacing an entry of the same sequence, and
    /// prunes the oldest entries past the journal's bounds.
    pub(crate) fn record(&mut self, confirmation: &Confirmation) -> Result<(), JournalError> {
        if let Some(newest) = self.entries.back() {
            if confirmation.sequence > newest.sequence + 1
                || confirmation.sequence < self.entries[0].sequence
            {
                return Err(JournalError::Broken(self.directory.clone()));
            }
        }
        let bytes = self.write(&self.entry_path(confirmation.sequence), confirmation)?;
        match self
            .entries
            .iter_mut()
            .find(|entry| entry.sequence == confirmation.sequence)
        {
            Some(entry) => {
                self.bytes = self.bytes - entry.bytes + bytes;
                entry.bytes = bytes;
            }
            None => {
                self.entries.push_back(Indexed {
                    sequence: confirmation.sequence,
                    bytes,
                });
                self.bytes += bytes;
            }
        }
        self.prune()
    }

    fn prune(&mut self) -> Result<(), JournalError> {
        while self.entries.len() > 1
            && (self.entries.len() > self.keep || self.bytes > MAX_CONFIRMATION_BYTES)
        {
            let oldest = self.entries[0];
            let pruned = self.entry(oldest.sequence)?;
            // The floor is stored before the entry goes, so a crash between
            // the two leaves an entry the floor already vouches for.
            self.commit(Meta {
                floor: Some(Floor::Pruned {
                    sequence: pruned.sequence,
                    submission: pruned.submission,
                }),
                ..self.meta.clone()
            })?;
            remove(&self.entry_path(oldest.sequence))?;
            self.entries.pop_front();
            self.bytes -= oldest.bytes;
        }
        Ok(())
    }

    /// This journal keeping at most `keep` entries.
    #[cfg(test)]
    pub(crate) fn keeping(mut self, keep: usize) -> Self {
        self.keep = keep;
        self
    }

    /// The sequences of the entries, oldest first.
    pub(crate) fn sequences(&self) -> impl DoubleEndedIterator<Item = u64> + '_ {
        self.entries.iter().map(|entry| entry.sequence)
    }

    /// The entry of `sequence`.
    pub(crate) fn entry(&self, sequence: u64) -> Result<Confirmation, JournalError> {
        self.read(&self.entry_path(sequence))
    }

    pub(crate) fn floor(&self) -> Option<&Floor> {
        self.meta.floor.as_ref()
    }

    pub(crate) fn handled_term(&self) -> u64 {
        self.meta.handled_term
    }

    pub(crate) fn undelivered(&self) -> &[Lost] {
        &self.meta.undelivered
    }

    pub(crate) fn closed(&self) -> Option<&str> {
        self.meta.closed.as_deref()
    }

    /// The replay begun and not finished, if any.
    pub(crate) fn replaying(&self) -> Option<Replaying> {
        self.meta.replaying
    }

    /// Records, before anything is sent again, that the check of `term`
    /// sends again every entry from sequence `from` on.
    pub(crate) fn begin_replay(&mut self, term: u64, from: u64) -> Result<(), JournalError> {
        self.commit(Meta {
            replaying: Some(Replaying { term, from }),
            ..self.meta.clone()
        })
    }

    /// Records that `term` was checked, its replay finished, and what it
    /// voided that is still to be handed over.
    pub(crate) fn handled(
        &mut self,
        term: u64,
        undelivered: Vec<Lost>,
    ) -> Result<(), JournalError> {
        self.commit(Meta {
            handled_term: self.meta.handled_term.max(term),
            undelivered,
            replaying: None,
            ..self.meta.clone()
        })
    }

    /// Records that what was owed again was handed over.
    pub(crate) fn delivered(&mut self) -> Result<(), JournalError> {
        self.commit(Meta {
            undelivered: Vec::new(),
            ..self.meta.clone()
        })
    }

    /// Records that the node cannot tell what a term voided, and why.
    pub(crate) fn close(&mut self, reason: String) -> Result<(), JournalError> {
        self.commit(Meta {
            closed: Some(reason),
            ..self.meta.clone()
        })
    }

    /// Writes `next` and only then takes it as the journal's state, so what
    /// the journal reports is never ahead of its file. A failed write keeps
    /// the state it had; the file then holds that state or `next` (the
    /// rename is atomic), and the next write that succeeds settles it.
    fn commit(&mut self, next: Meta) -> Result<(), JournalError> {
        self.write(&self.directory.join(META), &next)?;
        self.meta = next;
        Ok(())
    }

    fn entry_path(&self, sequence: u64) -> PathBuf {
        self.directory.join(format!("{sequence:020}{ENTRY_SUFFIX}"))
    }

    fn write_meta(&self) -> Result<(), JournalError> {
        self.write(&self.directory.join(META), &self.meta).map(drop)
    }

    fn write<T>(&self, path: &Path, value: &T) -> Result<usize, JournalError>
    where
        T: Serialize,
    {
        let format = |source| JournalError::Format {
            path: path.to_path_buf(),
            source,
        };
        let io = |source| JournalError::Io {
            path: path.to_path_buf(),
            source,
        };
        let bytes = serde_json::to_vec(value).map_err(format)?;
        let mut file = tempfile::NamedTempFile::new_in(&self.directory).map_err(io)?;
        file.write_all(&bytes).map_err(io)?;
        file.as_file().sync_all().map_err(io)?;
        file.persist(path).map_err(|error| io(error.error))?;
        #[cfg(unix)]
        File::open(&self.directory)
            .and_then(|directory| directory.sync_all())
            .map_err(io)?;
        Ok(bytes.len())
    }

    fn read<T>(&self, path: &Path) -> Result<T, JournalError>
    where
        T: for<'de> Deserialize<'de>,
    {
        let bytes = std::fs::read(path).map_err(|source| JournalError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        serde_json::from_slice(&bytes).map_err(|source| JournalError::Format {
            path: path.to_path_buf(),
            source,
        })
    }
}

fn remove(path: &Path) -> Result<(), JournalError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(JournalError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

mod hex_bytes {
    use serde::{Deserialize, Deserializer, Serializer};

    pub(super) fn serialize<S>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&hex::encode(bytes))
    }

    pub(super) fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let text = String::deserialize(deserializer)?;
        hex::decode(text).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
#[path = "journal_tests.rs"]
mod tests;
