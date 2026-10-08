//! Document storage keys shared across the write, merge, and index layers.
//!
//! The document body and deletion-marker key layouts were previously private to
//! `crates/db` (with `db-merge` hand-rolling the `/del/` prefix), which made it
//! impossible for the index layer to ask the one question unique enforcement
//! needs: *is the document an index entry points at still alive?* (#1111/#700:
//! a stale unique entry pointing at a deleted or missing document permanently
//! blocked re-creating that value, with no repair affordance.)
//!
//! Documents are keyed by their node-local u64 doc short ID (#4838), so these
//! helpers take a short ID and encode it with the same order-preserving uvarint
//! used everywhere else in the datastore.

use super::doc_id_index::encode_doc_short_id;

/// Prefix for document body keys: `/d/{collection_id}/{doc_short_id}`.
pub const DOC_KEY_PREFIX: &[u8] = b"/d/";

/// Prefix for logical-deletion markers: `/del/{collection_id}/{doc_short_id}`.
pub const DELETED_KEY_PREFIX: &[u8] = b"/del/";

// DEFRALEVEL(S5): Resolve per-doc shared write via CRDT merge/coalesce (open design)
/// Storage key for a document body.
pub fn doc_key(collection_id: &str, doc_short_id: u64) -> Vec<u8> {
    build_key(DOC_KEY_PREFIX, collection_id, doc_short_id)
}

// DEFRALEVEL(S1): Keep as Ordinary validated key; identical writes elide
/// Storage key for a document's logical-deletion marker.
pub fn deleted_doc_key(collection_id: &str, doc_short_id: u64) -> Vec<u8> {
    build_key(DELETED_KEY_PREFIX, collection_id, doc_short_id)
}

// DEFRALEVEL(S10): Expose /d/ + /del/ layout as a defradb key-codec impl of a regolith trait; regolith composes write keys
fn build_key(prefix: &[u8], collection_id: &str, doc_short_id: u64) -> Vec<u8> {
    let encoded = encode_doc_short_id(doc_short_id);
    let mut key = Vec::with_capacity(prefix.len() + collection_id.len() + 1 + encoded.len());
    key.extend_from_slice(prefix);
    key.extend_from_slice(collection_id.as_bytes());
    key.push(b'/');
    key.extend_from_slice(&encoded);
    key
}
