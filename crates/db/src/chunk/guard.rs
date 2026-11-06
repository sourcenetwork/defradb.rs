//! Checks on chunk writes that arrive from peers.

use document::{Document, NormalValue};
use schema::{CHUNK_HASH_FIELD, CHUNK_TEXT_FIELD};

use super::list::text_hash;

/// Why a merged chunk is refused.
#[derive(Debug, thiserror::Error)]
pub enum ChunkMergeError {
    #[error("a chunk cannot change after it is created")]
    Changed,
    #[error("a chunk's hash does not match its text")]
    HashMismatch,
}

/// Refuses a chunk whose hash doesn't match its text, and any change to an
/// existing chunk, its vectors included. Otherwise a peer could plant a fake
/// chunk or vector that the parent's next write would never replace.
pub fn check_chunk_merge(old: Option<&Document>, merged: &Document) -> Result<(), ChunkMergeError> {
    if let Some(old) = old {
        let changed = old
            .field_names()
            .chain(merged.field_names())
            .any(|field| old.get(field) != merged.get(field));
        if changed {
            return Err(ChunkMergeError::Changed);
        }
    }
    // An encrypted chunk can't be checked here, so the read path checks its hash.
    if let (Some(NormalValue::String(text)), Some(NormalValue::String(hash))) =
        (merged.get(CHUNK_TEXT_FIELD), merged.get(CHUNK_HASH_FIELD))
    {
        if text_hash(text) != *hash {
            return Err(ChunkMergeError::HashMismatch);
        }
    }
    Ok(())
}
