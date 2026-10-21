//! Applying an update's explicit CRDT ops to the document it writes.

use chrono::{DateTime, FixedOffset};
use document::{Document, NormalValue};
use schema::{CType, FieldDescription};

use super::create_conversions::json_to_normal_value_with_kind_and_time;
use super::update::{increment_value, validate_pcounter_increment};
use crate::error::{QueryError, Result};
use crate::mapper::FieldOp;

/// Apply `op` to `field` of `doc`, whose current values it holds.
pub(super) fn apply_op(
    doc: &mut Document,
    field: &str,
    field_def: Option<&FieldDescription>,
    op: &FieldOp,
    request_time: Option<DateTime<FixedOffset>>,
) -> Result<()> {
    let crdt = field_def.map(|f| f.crdt_type);
    let kind = field_def.map(|f| &f.kind);
    match op {
        FieldOp::Set(value) => {
            // Two peers setting a counter at once would each add their own gap.
            if crdt.is_some_and(|c| c.is_counter()) {
                return Err(QueryError::execution(format!(
                    "`{field}` is a counter, which merges increments; use `increment`"
                )));
            }
            let value = json_to_normal_value_with_kind_and_time(value, kind, request_time)?;
            doc.set(field.to_string(), value);
            Ok(())
        }
        FieldOp::Increment(amount) => {
            if !crdt.is_some_and(|c| c.is_counter()) {
                return Err(QueryError::execution(format!(
                    "`increment` applies to counter fields, and `{field}` is not one; use `set`"
                )));
            }
            let amount = json_to_normal_value_with_kind_and_time(amount, kind, request_time)?;
            add_to_counter(doc, field, crdt, amount)
        }
        FieldOp::Text(edits) => {
            if !crdt.is_some_and(|c| c.is_text()) {
                return Err(QueryError::execution(format!(
                    "text edits apply to `@crdt(type: text)` fields, and `{field}` is not one; \
                     use `set`"
                )));
            }
            for edit in edits {
                doc.edit_text(field.to_string(), edit.clone());
            }
            Ok(())
        }
    }
}

fn add_to_counter(
    doc: &mut Document,
    field: &str,
    crdt: Option<CType>,
    amount: NormalValue,
) -> Result<()> {
    if crdt == Some(CType::PCounter) {
        validate_pcounter_increment(&amount).map_err(|_| {
            QueryError::execution(format!(
                "`{field}` is a P-counter, which only grows; it cannot move below its current value"
            ))
        })?;
    }
    let total = increment_value(doc.get(field), &amount)?;
    doc.set_counter_delta(field.to_string(), amount);
    doc.set(field.to_string(), total);
    Ok(())
}
