//! A Text field's block carries edits, not a value.

use defra_core::block::{CrdtDelta, TextDeltaPayload};
use defra_core::TextOp;
use document::{Document, NormalValue};
use schema::CollectionVersion;

/// Whether `field` is a Text field of `schema`.
pub(crate) fn is_text(schema: &CollectionVersion, field: &str) -> bool {
    schema
        .fields
        .iter()
        .any(|f| f.name == field && f.crdt_type.is_text())
}

/// The encoded ops of a Text field's block: the edits the write recorded, or,
/// without them, an insert of the whole value on create and a replacement of
/// the whole text on update.
pub(crate) fn ops_bytes(
    doc: &Document,
    field: &str,
    value: &NormalValue,
    is_create: bool,
) -> Result<Vec<u8>, String> {
    let text = value.as_str().unwrap_or_default();
    let ops = match doc.text_ops(field) {
        Some(ops) => ops.to_vec(),
        None if is_create => TextOp::diff("", text),
        None => TextOp::replace_all(text),
    };
    TextOp::encode_all(&ops).map_err(|error| error.to_string())
}

pub(crate) fn delta(
    field_name: &str,
    priority: u64,
    schema_version_id: &str,
    data: Vec<u8>,
) -> CrdtDelta {
    CrdtDelta::Text(TextDeltaPayload {
        field_name: field_name.to_owned(),
        priority,
        schema_version_id: schema_version_id.to_owned(),
        data,
    })
}
