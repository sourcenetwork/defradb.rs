//! Checks on chunk writes that arrive from peers.

use document::{Document, NormalValue};
use schema::{CHUNK_HASH_FIELD, CHUNK_SOURCE_ID_FIELD, CHUNK_TEXT_FIELD};

use super::hash::text_hash;

/// Why a merged chunk is refused.
#[derive(Debug, thiserror::Error)]
pub enum ChunkMergeError {
    #[error("a chunk cannot change after it is created")]
    Changed,
    #[error("a chunk's hash does not match its text")]
    HashMismatch,
    #[error("a chunk's text and hash must both be strings")]
    NotText,
}

/// Refuses a changed chunk, or one whose text and hash aren't a matching pair.
/// Only an `encrypted` merge may leave both out, as fields this node can't read.
pub fn check_chunk_merge(
    old: Option<&Document>,
    merged: &Document,
    encrypted: bool,
) -> Result<(), ChunkMergeError> {
    if let Some(old) = old {
        let changed = [CHUNK_SOURCE_ID_FIELD, CHUNK_TEXT_FIELD, CHUNK_HASH_FIELD]
            .iter()
            .any(|field| old.get(field) != merged.get(field));
        if changed {
            return Err(ChunkMergeError::Changed);
        }
    }
    match (merged.get(CHUNK_TEXT_FIELD), merged.get(CHUNK_HASH_FIELD)) {
        (Some(NormalValue::String(text)), Some(NormalValue::String(hash))) => {
            if text_hash(text) != *hash {
                return Err(ChunkMergeError::HashMismatch);
            }
            Ok(())
        }
        // The read path checks the hash once this node can read the text.
        (None, None) if encrypted => Ok(()),
        _ => Err(ChunkMergeError::NotText),
    }
}
