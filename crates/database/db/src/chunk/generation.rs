//! A chunk made again after its delete takes the next generation, so a new ID.
//!
//! `proofs/tla/ChunkCleanup.tla` checks that this never resurrects a deleted
//! chunk and that peers that make it again pick the same generation.

use datastore::NamespaceView;
use document::{Document, NormalValue};
use query::error::{QueryError, Result};
use schema::CHUNK_GENERATION_FIELD;

/// Whether this node already used `doc_id`, for a live chunk or a deleted one.
pub(crate) async fn is_taken(systemstore: &NamespaceView, doc_id: &str) -> Result<bool> {
    Ok(crate::docid::map::get_doc_ref(systemstore, doc_id)
        .await
        .map_err(|e| QueryError::execution(e.to_string()))?
        .is_some())
}

/// Moves `doc` to the generation after the one whose ID was taken.
pub(crate) fn next_generation(doc: &mut Document) -> Result<()> {
    let next = generation_of(doc) + 1;
    doc.set(CHUNK_GENERATION_FIELD, NormalValue::Int(next))
        .map_err(|e| QueryError::execution(format!("chunk generation: {e}")))
}

/// A chunk's generation. The first has none set, so its ID stays as it was.
pub(crate) fn generation_of(doc: &Document) -> i64 {
    doc.get(CHUNK_GENERATION_FIELD)
        .and_then(NormalValue::as_int)
        .unwrap_or(0)
}
