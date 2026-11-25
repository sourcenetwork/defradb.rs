//! Checks on chunk writes that arrive from peers.

use document::{Document, NormalValue};
use schema::{CHUNK_HASH_FIELD, CHUNK_SOURCE_ID_FIELD, CHUNK_TEXT_FIELD};

use super::hash::text_hash;

/// Why a merged chunk is refused.
#[derive(Debug, thiserror::Error)]
pub enum ChunkMergeError {
    #[error("a chunk cannot change after it is created. Field: {0}")]
    Changed(String),
    #[error("a chunk's hash does not match its text")]
    HashMismatch,
    #[error("a chunk's text and hash must both be strings")]
    NotText,
    #[error("a chunk must arrive with its vector. Field: {0}")]
    NoVector(String),
}

/// Refuses a change to any stored field of a chunk, its vector included, a
/// chunk whose text and hash aren't a matching pair, or one without its vector.
/// Only an `encrypted` merge may leave them out, as fields this node can't read.
pub fn check_chunk_merge(
    old: Option<&Document>,
    merged: &Document,
    encrypted: bool,
) -> Result<(), ChunkMergeError> {
    let vectors = merged
        .collection()
        .vector_embeddings
        .iter()
        .map(|embedding| embedding.field_name.as_str());
    if let Some(old) = old {
        // The fixed fields count even when unset, so a merge can't fill one in.
        // Other unset fields don't: a field outside the schema is dropped later.
        let fixed = [CHUNK_SOURCE_ID_FIELD, CHUNK_TEXT_FIELD, CHUNK_HASH_FIELD];
        if let Some(field) = fixed
            .into_iter()
            .chain(vectors.clone())
            .chain(old.field_names())
            .find(|field| old.get(field) != merged.get(field))
        {
            return Err(ChunkMergeError::Changed(field.to_string()));
        }
    }
    match (merged.get(CHUNK_TEXT_FIELD), merged.get(CHUNK_HASH_FIELD)) {
        (Some(NormalValue::String(text)), Some(NormalValue::String(hash))) => {
            if text_hash(text) != *hash {
                return Err(ChunkMergeError::HashMismatch);
            }
            // A later write of the same text reuses this chunk, so its text would have no vector.
            let mut missing = vectors.filter(|field| {
                merged
                    .get(field)
                    .is_none_or(|value| matches!(value, NormalValue::Null))
            });
            match missing.next() {
                Some(field) if !encrypted => Err(ChunkMergeError::NoVector(field.to_string())),
                _ => Ok(()),
            }
        }
        // The read path checks the hash once this node can read the text.
        (None, None) if encrypted => Ok(()),
        _ => Err(ChunkMergeError::NotText),
    }
}
