//! `_all` over an empty array is true, so its index scan must also find the
//! documents whose array is empty.
//!
//! An array index stores an empty array under one null entry, the entry a null
//! array and a null element share, so the scan adds that entry as a branch of
//! its own. The residual filter, the whole filter, drops the null arrays and
//! the null elements again.

use document::NormalValue;
use schema::FieldKind;

use crate::mapper::FilterOp;

use super::types::{FieldCondition, IndexScanType};

/// `scan` widened to the null entry when `conditions` hold an `_all` on the
/// index's first field and that field is a scalar array.
pub(super) fn with_empty_arrays(
    scan: IndexScanType,
    conditions: &[FieldCondition],
    first_field: &str,
    first_field_kind: Option<&FieldKind>,
    is_composite: bool,
) -> IndexScanType {
    let is_scalar_array = first_field_kind.is_some_and(|kind| kind.as_scalar_array().is_some());
    let has_all = conditions
        .iter()
        .any(|c| c.field_name == first_field && c.array_op == Some(FilterOp::All));
    if !is_scalar_array || !has_all || visits_null(&scan) {
        return scan;
    }
    let empty = if is_composite {
        IndexScanType::PrefixScan {
            prefix_values: vec![NormalValue::Null],
            reverse: false,
        }
    } else {
        IndexScanType::ExactMatch {
            values: vec![NormalValue::Null],
        }
    };
    IndexScanType::OrScan {
        branches: vec![scan, empty],
    }
}

/// Whether `scan` already visits the null entry, so adding it would count its
/// documents twice.
fn visits_null(scan: &IndexScanType) -> bool {
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
        _ => false,
    }
}
