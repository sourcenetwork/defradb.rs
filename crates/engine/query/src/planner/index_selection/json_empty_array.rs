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
///
/// An `_any` on the same array needs an element, so no empty array there
/// passes the filter and its marker would only add reads.
pub(super) fn with_json_empty_arrays(
    scan: IndexScanType,
    conditions: &[FieldCondition],
    first_field: &str,
    is_composite: bool,
) -> IndexScanType {
    let on_first_field = |op| {
        conditions
            .iter()
            .filter(move |c| c.field_name == first_field && c.array_op == Some(op))
            .filter_map(|c| c.json_path.as_ref().filter(|path| !path.is_empty()))
    };
    let mut paths: Vec<&JsonPath> = Vec::new();
    for path in on_first_field(FilterOp::All) {
        if !paths.contains(&path)
            && !reads_path_top(&scan, path)
            && !on_first_field(FilterOp::Any).any(|any| any == path)
        {
            paths.push(path);
        }
    }
    if paths.is_empty() {
        return scan;
    }

    let mut branches = Vec::with_capacity(paths.len() + 1);
    branches.push(scan);
    for path in paths {
        let marker = NormalValue::JsonLeaf(JsonLeafValue::new(
            path.clone(),
            JsonScalarValue::EmptyArray,
        ));
        branches.push(if is_composite {
            IndexScanType::PrefixScan {
                prefix_values: vec![marker],
                reverse: false,
            }
        } else {
            IndexScanType::ExactMatch {
                values: vec![marker],
            }
        });
    }
    IndexScanType::OrScan { branches }
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
