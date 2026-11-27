//! `_all` over an empty array is true, so its index scan must also find the
//! documents whose array is empty.
//!
//! An array index stores an empty array under one null entry, the entry a null
//! array and a null element share, so the scan adds that entry as a branch of
//! its own. The residual filter, the whole filter, drops the null arrays and
//! the null elements again.

use document::{JsonLeafValue, JsonPath, JsonPathPart, JsonScalarValue, NormalValue};
use schema::FieldKind;
use storage::index::Bound;

use crate::mapper::FilterOp;

use super::types::{FieldCondition, IndexScanType};

/// `scan` widened to the null entry when `conditions` hold an `_all` on the
/// index's first field and that field is a scalar array.
///
/// An `_any` on the same field needs an element, so no empty array passes the
/// filter and the widening would only add reads.
pub(super) fn with_empty_arrays(
    scan: IndexScanType,
    conditions: &[FieldCondition],
    first_field: &str,
    first_field_kind: Option<&FieldKind>,
    first_field_descending: bool,
    is_composite: bool,
) -> IndexScanType {
    let is_scalar_array = first_field_kind.is_some_and(|kind| kind.as_scalar_array().is_some());
    let on_first_field = |op| {
        conditions
            .iter()
            .any(|c| c.field_name == first_field && c.array_op == Some(op))
    };
    if !is_scalar_array
        || !on_first_field(FilterOp::All)
        || on_first_field(FilterOp::Any)
        || visits_null(&scan, first_field_descending)
    {
        return scan;
    }
    with_entry(scan, NormalValue::Null, is_composite)
}

/// `scan` widened to the empty-array marker at `path` of a JSON field, unless
/// an `_any` on that same array makes every empty array fail.
pub(super) fn with_json_empty_arrays(
    scan: IndexScanType,
    path: JsonPath,
    conditions: &[FieldCondition],
    is_composite: bool,
) -> IndexScanType {
    let any_on_path = conditions.iter().any(|c| {
        c.array_op == Some(FilterOp::Any)
            && c.json_path.as_ref().map(array_path) == Some(path.clone())
    });
    if any_on_path {
        return scan;
    }
    let marker = NormalValue::JsonLeaf(JsonLeafValue::new(path, JsonScalarValue::EmptyArray));
    with_entry(scan, marker, is_composite)
}

/// The JSON array path of an `_all` condition: `None` for any other condition.
pub(super) fn all_path(cond: &FieldCondition, is_json: bool) -> Option<JsonPath> {
    if !is_json || cond.array_op != Some(FilterOp::All) {
        return None;
    }
    Some(array_path(&cond.json_path.clone().unwrap_or_default()))
}

/// The path of the array an element path reads: everything before its last
/// `Index`, where the marker of an empty array is stored.
fn array_path(path: &JsonPath) -> JsonPath {
    let end = path
        .0
        .iter()
        .rposition(|part| matches!(part, JsonPathPart::Index))
        .unwrap_or(path.len());
    JsonPath(path.0[..end].to_vec())
}

fn with_entry(scan: IndexScanType, entry: NormalValue, is_composite: bool) -> IndexScanType {
    let lookup = if is_composite {
        IndexScanType::PrefixScan {
            prefix_values: vec![entry],
            reverse: false,
        }
    } else {
        IndexScanType::ExactMatch {
            values: vec![entry],
        }
    };
    IndexScanType::OrScan {
        branches: vec![scan, lookup],
    }
}

/// Whether `scan` already visits the null entry, so adding it would count its
/// documents twice.
///
/// Null sorts first in an ascending index and last in a descending one, and
/// the range bounds are in key order, so a range with no prefix and no bound
/// on the null side reads it.
fn visits_null(scan: &IndexScanType, descending: bool) -> bool {
    match scan {
        IndexScanType::PrefixScan { prefix_values, .. } => prefix_values
            .first()
            .is_none_or(|value| matches!(value, NormalValue::Null)),
        IndexScanType::ExactMatch { values } => {
            matches!(values.first(), Some(NormalValue::Null))
        }
        IndexScanType::InScan { values, .. } => values
            .iter()
            .any(|value| matches!(value, NormalValue::Null)),
        IndexScanType::RangeScan {
            prefix_values,
            lower,
            upper,
            ..
        } => {
            let null_side = if descending { upper } else { lower };
            prefix_values.is_empty() && matches!(null_side, Bound::Unbounded)
        }
        _ => false,
    }
}
