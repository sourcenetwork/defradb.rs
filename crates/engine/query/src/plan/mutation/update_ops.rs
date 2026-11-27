//! Writing an update's ops to the document it updates, through each field's
//! CRDT type.

use chrono::{DateTime, FixedOffset};
use crdt::Crdts;
use document::{Document, FieldOp, NormalValue};
use schema::FieldDescription;

use super::create_conversions::json_to_normal_value_with_kind_and_time;
use crate::error::{QueryError, Result};
use crate::mapper::InputOp;
use defra_core::json_crdt::{JsonEdit, JsonOp};

/// The op an implicit `field: value` input makes: the one the field's type
/// reads a written value as.
pub(super) fn implicit_op(
    crdts: &Crdts,
    field_def: Option<&FieldDescription>,
    value: NormalValue,
) -> Result<FieldOp> {
    match field_def.and_then(|field| crdts.field(field.crdt_type)) {
        Some(crdt) => crdt
            .op(value, false)
            .map_err(|error| QueryError::execution(error.to_string())),
        None => Ok(FieldOp::Set(value)),
    }
}

/// The op an explicit input op names, refused when the field's type cannot
/// take it.
pub(super) fn named_op(
    crdts: &Crdts,
    field: &str,
    field_def: Option<&FieldDescription>,
    op: &InputOp,
    request_time: Option<DateTime<FixedOffset>>,
) -> Result<FieldOp> {
    let value = |json| {
        json_to_normal_value_with_kind_and_time(json, field_def.map(|f| &f.kind), request_time)
    };
    let op = match op {
        InputOp::Set(json) => FieldOp::Set(value(json)?),
        InputOp::Increment(json) => FieldOp::Increment(value(json)?),
        InputOp::Text(edits) => FieldOp::Edit(edits.clone()),
        InputOp::JsonDelete(paths) => FieldOp::Paths(
            paths
                .iter()
                .map(|path| JsonOp {
                    path: path.clone(),
                    edit: JsonEdit::Delete,
                })
                .collect(),
        ),
    };
    match field_def.and_then(|field| crdts.field(field.crdt_type)) {
        Some(crdt) => crdt
            .named_op(op)
            .map_err(|error| QueryError::execution(format!("`{field}`: {error}"))),
        None if matches!(op, FieldOp::Set(_)) => Ok(op),
        None => Err(QueryError::execution(format!(
            "`{field}` has no CRDT type, so it takes only `set`, not `{}`",
            op.name()
        ))),
    }
}

/// Apply `op` to `field` of `doc`, whose current values it holds, and keep it
/// as the field's pending op for the write's block.
pub(super) fn write_op(
    crdts: &Crdts,
    doc: &mut Document,
    field: &str,
    field_def: Option<&FieldDescription>,
    op: FieldOp,
) -> Result<()> {
    let execution = |error: &dyn std::fmt::Display| QueryError::execution(error.to_string());
    if let Some((field_def, crdt)) =
        field_def.and_then(|fd| crdts.field(fd.crdt_type).map(|crdt| (fd, crdt)))
    {
        let value = crdt
            .apply(field_def, doc.get(field), &op)
            .map_err(|e| execution(&e))?;
        doc.set(field.to_string(), value)
            .map_err(|e| execution(&e))?;
    }
    doc.edit(field.to_string(), op).map_err(|e| execution(&e))
}
