//! Copying a wrapper-encrypted store into a new one, and checking the copy.
//!
//! Memory is bounded by bytes whatever the store's size: the source is read
//! one entry at a time, and the copy is written through a streaming writer
//! that holds at most [`BATCH_BYTES`] plus one entry and syncs each batch.
//! The check reads the copy back the same way.

use std::path::Path;

use regolith::{Db, DurabilityMode, Options, StreamOptions, WriteOptions};
use sha2::{Digest, Sha256};

use super::wrapped::WrapperKey;
use super::Step;
use crate::corekv::{Error, Result};

/// Bytes the copy buffers before writing them as one synced batch.
const BATCH_BYTES: usize = 4 << 20;

/// What a pass over a store saw: how many entries, and a SHA-256 over each
/// key and value with their lengths, in key order.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Tally {
    pub(super) entries: u64,
    digest: [u8; 32],
}

#[derive(Default)]
struct Hasher {
    entries: u64,
    sha: Sha256,
}

impl Hasher {
    fn add(&mut self, key: &[u8], value: &[u8]) {
        for part in [key, value] {
            self.sha.update((part.len() as u64).to_be_bytes());
            self.sha.update(part);
        }
        self.entries += 1;
    }

    fn finish(self) -> Tally {
        Tally {
            entries: self.entries,
            digest: self.sha.finalize().into(),
        }
    }
}

fn engine(error: regolith::Error, action: &str) -> Error {
    Error::Backend(format!("store rewrite could not {action}: {error}"))
}

fn open(path: &Path, options: &Options) -> Result<Db> {
    Db::open(path, options.clone()).map_err(|error| engine(error, "open the copy"))
}

fn close(db: Db) -> Result<()> {
    db.close().map_err(|error| engine(error, "close the copy"))
}

/// Writes every entry of `source`, decrypted under `key`, into a new store at
/// `target` sealed under the current at-rest key, and returns the tally of
/// what it wrote.
pub(super) fn copy(
    source: &Db,
    key: &WrapperKey<'_>,
    target: &Path,
    options: &Options,
    step: &mut dyn FnMut(Step),
) -> Result<Tally> {
    let db = open(target, options)?;
    let tally = {
        let mut writer = db.streaming_writer(StreamOptions {
            max_buffered_bytes: BATCH_BYTES,
            durability: Some(DurabilityMode::Immediate),
        });
        let mut hasher = Hasher::default();
        for entry in source.snapshot().into_owned_iter() {
            let (stored_key, sealed) = entry.map_err(|error| engine(error, "read the store"))?;
            let value = key.open(&stored_key, &sealed).ok_or_else(|| {
                Error::Backend(format!(
                    "entry {} of this store does not decrypt under the at-rest key its other entries use; the store was left unchanged",
                    hasher.entries + 1
                ))
            })?;
            hasher.add(&stored_key, &value);
            writer
                .put_owned(&stored_key, value)
                .map_err(|error| engine(error, "write the copy"))?;
            step(Step::Copying);
        }
        writer
            .finish()
            .map_err(|error| engine(error, "write the copy"))?;
        hasher.finish()
    };
    close(db)?;
    Ok(tally)
}

/// Reads the copy back from disk, compares it with what was copied, and only
/// then writes the rewrite marker, synced, into it.
pub(super) fn verify_and_mark(
    target: &Path,
    options: &Options,
    copied: &Tally,
    marker_key: &[u8],
) -> Result<()> {
    let db = open(target, options)?;
    let mut hasher = Hasher::default();
    for entry in db.snapshot().into_owned_iter() {
        let (key, value) = entry.map_err(|error| engine(error, "read the copy"))?;
        hasher.add(&key, &value);
    }
    if hasher.finish() != *copied {
        return Err(Error::Backend(
            "the rewritten copy does not match the store it was copied from; the store was left unchanged".into(),
        ));
    }
    let synced = WriteOptions {
        sync: true,
        ..WriteOptions::default()
    };
    db.put_opt(&synced, marker_key, &copied.entries.to_be_bytes())
        .map_err(|error| engine(error, "mark the copy"))?;
    close(db)
}
