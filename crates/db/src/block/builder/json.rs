//! A Json field's block carries edits at paths, not a value.

use defra_core::block::{CrdtDelta, JsonDeltaPayload};
use defra_core::json_crdt;
use document::Document;
use schema::CollectionVersion;

/// Whether `field` is a Json field of `schema`.
pub(crate) fn is_json(schema: &CollectionVersion, field: &str) -> bool {
    schema
        .fields
        .iter()
        .any(|f| f.name == field && f.crdt_type.is_json())
}

/// The encoded edits of a Json field's block: the ops the write recorded, or,
/// without them, the whole value written over an absent field.
pub(crate) fn ops_bytes(doc: &Document, field: &str) -> Result<Vec<u8>, String> {
    json_crdt::encode_ops(&doc.json_ops_or_value(field)).map_err(|error| error.to_string())
}

pub(crate) fn delta(
    field_name: &str,
    priority: u64,
    schema_version_id: &str,
    data: Vec<u8>,
) -> CrdtDelta {
    CrdtDelta::Json(JsonDeltaPayload {
        field_name: field_name.to_owned(),
        priority,
        schema_version_id: schema_version_id.to_owned(),
        data,
    })
}
