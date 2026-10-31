//! An embedder's check on replicated documents. Before a replicated commit
//! stores anything, the filter is given the collection, the DocID and the
//! values of the fields the commit writes; a refusal drops the document for
//! good.

use defra_core::block::CrdtDelta;
use defra_core::Block;
use rapidhash::RapidHashMap;

/// The fields a commit writes, by name, each its CBOR-decoded value. A
/// non-negative integer decodes as an unsigned integer and a counter holds
/// its increment, as Go's decoder gives them.
pub type FieldValues = RapidHashMap<String, ciborium::Value>;

/// Decides whether a replicated document is kept. Called on the merge path
/// for every replicated commit, so it must not block.
pub trait ReplicationFilter: Send + Sync {
    /// Whether to keep the document `doc_id` of the collection with
    /// `collection_id`, given the values its commit writes. A field that is
    /// encrypted, does not decode or has not arrived is absent, so a filter
    /// keeps a document whose deciding field is missing.
    fn allow_replication(&self, collection_id: &str, doc_id: &str, fields: &FieldValues) -> bool;
}

/// Add the values `block` holds under their field names: one for a field
/// block, each field of an immutable block, none for a composite. An
/// encrypted block holds ciphertext, and a value that does not decode is
/// left out.
pub fn add_field_values(fields: &mut FieldValues, block: &Block) {
    if block.encryption.is_some() {
        return;
    }
    if let CrdtDelta::Immutable(payload) = &block.delta {
        for delta in payload.lww_deltas() {
            add(fields, &delta.field_name, &delta.data);
        }
        return;
    }
    if let Some(delta) = block.delta.field() {
        add(fields, delta.field_name, delta.data);
    }
}

fn add(fields: &mut FieldValues, name: &str, data: &[u8]) {
    if let Ok(value) = ciborium::from_reader::<ciborium::Value, _>(data) {
        fields.insert(name.to_owned(), value);
    }
}
