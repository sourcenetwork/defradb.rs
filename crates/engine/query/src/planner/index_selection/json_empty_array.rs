//! `_all` over an empty array is true, so an `_all` scan on a JSON path must
//! also find the documents whose array there is empty.
//!
//! A JSON index writes one empty-array marker under each empty array, at the
//! path its elements would have, so the scan adds that marker as a branch of
//! its own. The residual filter, the whole filter, checks every candidate.

use document::{JsonLeafValue, JsonPath, JsonScalarValue, NormalValue};
use storage::index::Bound;

use crate::mapper::FilterOp;

use super::types::{FieldCondition, IndexScanType};

/// `scan` widened to the empty-array markers its `_all` conditions on the
/// JSON index's first field need, unless it reads them already.
pub(super) fn with_json_empty_arrays(
    scan: IndexScanType,
    conditions: &[FieldCondition],
    first_field: &str,
    is_composite: bool,
) -> IndexScanType {
    let Some(path) = conditions
        .iter()
        .filter(|c| c.field_name == first_field && c.array_op == Some(FilterOp::All))
        .find_map(|c| c.json_path.as_ref().filter(|path| !path.is_empty()))
    else {
        return scan;
    };
    if reads_path_top(&scan, path) {
        return scan;
    }
    let marker = NormalValue::JsonLeaf(JsonLeafValue::new(
        path.clone(),
        JsonScalarValue::EmptyArray,
    ));
    let empty = if is_composite {
        IndexScanType::PrefixScan {
            prefix_values: vec![marker],
            reverse: false,
        }
    } else {
        IndexScanType::ExactMatch {
            values: vec![marker],
        }
    };
    IndexScanType::OrScan {
        branches: vec![scan, empty],
    }
}

/// Whether `scan` reaches the top of `path`, where the marker sorts, so
/// adding the marker would read it twice.
fn reads_path_top(scan: &IndexScanType, path: &JsonPath) -> bool {
    match scan {
        IndexScanType::RangeScan { lower, upper, .. } => {
            is_path_max(lower, path) || is_path_max(upper, path)
        }
        IndexScanType::PrefixScan { prefix_values, .. } => prefix_values.is_empty(),
        _ => false,
    }
}

fn is_path_max(bound: &Bound, path: &JsonPath) -> bool {
    match bound {
        Bound::Inclusive(NormalValue::JsonLeaf(leaf))
        | Bound::Exclusive(NormalValue::JsonLeaf(leaf)) => {
            leaf.value == JsonScalarValue::PathMax && &leaf.path == path
        }
        _ => false,
    }
}
