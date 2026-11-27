//! Core filter-to-index-scan conversion.

use document::{JsonPath, NormalValue};
use schema::{FieldKind, IndexDescription, ScalarKind};
use storage::index::Bound;

use crate::mapper::{Filter, FilterOp, OrderBy};

use super::conditions::{
    accepts_missing, can_be_ordered_by_filtered_index, can_use_index, extract_field_conditions,
    filtered_index_order_prefix, is_full_scan, should_fallback_to_full_scan,
};
use super::empty_array::with_empty_arrays;
use super::json_empty_array::with_json_empty_arrays;
use super::path_bounds::{composite_range, order_in_values, path_high_end, path_low_end};
use super::types::{
    ConditionValue, FieldCondition, IndexScanParams, IndexScanType, ScanValueFilter,
};
use super::values::{
    normalize_for_index_field, wrap_value_for_json_path, wrap_values_for_json_path,
};

/// Convert a filter to index scan parameters.
///
/// Returns None if the filter cannot use the index efficiently.
///
/// # Ordering Support
///
/// If `order_by` is provided, the scan direction will be set based on whether
/// the index can satisfy the ordering:
/// - If ordering matches index direction: forward scan
/// - If ordering is opposite of index direction: reverse scan
///
/// # Limit/Offset Support
///
/// If `limit` and `offset` are provided and the index provides ordering,
/// these are passed through to enable early termination during index scan.
/// This optimization allows ORDER BY + LIMIT queries to stop scanning early.
///
/// # Array Field Support
///
/// For array fields with `_any`, `_all` operators, the scan type is determined
/// by the inner operator. For example:
/// - `{numbers: {_any: {_eq: 30}}}` → `ExactMatch{values: [30]}`
/// - `{tags: {_any: {_in: ["red", "blue"]}}}` → `InScan{values: ["red", "blue"]}`
pub fn filter_to_index_scan(
    filter: &Filter,
    index: &IndexDescription,
    order_by: Option<&OrderBy>,
    collection_fields: &[schema::FieldDescription],
    limit: Option<u64>,
    offset: u64,
) -> Option<IndexScanParams> {
    if !can_use_index(filter, index) {
        return None;
    }

    let conditions: Vec<FieldCondition> = extract_field_conditions(filter)
        .into_iter()
        .map(|c| {
            let is_json = is_json_field(field_kind(collection_fields, &c.field_name));
            at_entry_path(c, is_json)
        })
        .collect();
    let first_field = &index.fields[0].name;
    let first_field_kind = field_kind(collection_fields, first_field);
    let first_field_is_json = is_json_field(first_field_kind);

    let first_field_conditions: Vec<_> = conditions
        .iter()
        .filter(|c| &c.field_name == first_field)
        .cloned()
        .collect();

    if first_field_conditions.is_empty() {
        return None;
    }

    // Check if any condition requires falling back to full scan (matches Go's shouldFallbackToFullScan).
    // Returns None to skip index when the index cannot produce correct results.
    for cond in &first_field_conditions {
        if should_fallback_to_full_scan(cond, first_field_is_json, first_field_kind) {
            return None;
        }
    }

    // Analyze conditions to determine scan type
    // For array operators, we look at the inner operator
    // Track JSON path for wrapping values when needed
    let mut has_eq = false;
    let mut eq_value = None;
    let mut eq_cond: Option<&FieldCondition> = None;
    let mut eq_json_path: Option<JsonPath> = None;
    let mut has_in = false;
    let mut in_values = None;
    let mut in_cond: Option<&FieldCondition> = None;
    let mut in_json_path: Option<JsonPath> = None;
    let mut has_scan_all = false;
    let mut scan_value_filter: Option<ScanValueFilter> = None;
    let mut value_filter_cond: Option<&FieldCondition> = None;
    let mut lower_bound = Bound::Unbounded;
    let mut upper_bound = Bound::Unbounded;
    let mut range_json_path: Option<JsonPath> = None;
    let mut scan_json_path: Option<JsonPath> = None;
    // One scan holds one path and one `_any` clause; the filter checks the rest.
    let first_range = first_field_conditions.iter().find(|cond| {
        cond.array_op != Some(FilterOp::None)
            && matches!(
                cond.op,
                FilterOp::Gt | FilterOp::Gte | FilterOp::Lt | FilterOp::Lte
            )
            && matches!(cond.value, ConditionValue::Single(_))
    });
    let in_range = |cond: &FieldCondition| {
        first_range.is_some_and(|first| {
            first.json_path == cond.json_path
                && (cond.array_op != Some(FilterOp::Any) || first.clause == cond.clause)
        })
    };
    // A full scan reads one path. A bare `_nin` passes a document missing its
    // path, which has no entry there, so it chooses only when nothing else can.
    let full_scan_at = first_field_conditions
        .iter()
        .position(|cond| is_full_scan(cond) && !accepts_missing(cond))
        .or_else(|| first_field_conditions.iter().position(is_full_scan));

    for (at, cond) in first_field_conditions.iter().enumerate() {
        // Skip _none operators (they don't use index)
        if cond.array_op == Some(FilterOp::None) {
            continue;
        }

        match cond.op {
            FilterOp::Eq if !has_eq => {
                if let ConditionValue::Single(v) = &cond.value {
                    has_eq = true;
                    eq_value = Some(v.clone());
                    eq_cond = Some(cond);
                    eq_json_path = cond.json_path.clone();
                }
            }
            FilterOp::In if !has_in => {
                if let ConditionValue::Multiple(vs) = &cond.value {
                    has_in = true;
                    in_values = Some(vs.clone());
                    in_cond = Some(cond);
                    in_json_path = cond.json_path.clone();
                }
            }
            FilterOp::Gt if in_range(cond) => {
                if let ConditionValue::Single(v) = &cond.value {
                    lower_bound = Bound::Exclusive(v.clone());
                    range_json_path = cond.json_path.clone();
                }
            }
            FilterOp::Gte if in_range(cond) => {
                if let ConditionValue::Single(v) = &cond.value {
                    lower_bound = Bound::Inclusive(v.clone());
                    range_json_path = cond.json_path.clone();
                }
            }
            FilterOp::Lt if in_range(cond) => {
                if let ConditionValue::Single(v) = &cond.value {
                    upper_bound = Bound::Exclusive(v.clone());
                    range_json_path = cond.json_path.clone();
                }
            }
            FilterOp::Lte if in_range(cond) => {
                if let ConditionValue::Single(v) = &cond.value {
                    upper_bound = Bound::Inclusive(v.clone());
                    range_json_path = cond.json_path.clone();
                }
            }
            // One condition pins the scan path and its value filter; the
            // filter checks the others.
            FilterOp::Ne
            | FilterOp::Nin
            | FilterOp::Like
            | FilterOp::Nlike
            | FilterOp::Ilike
            | FilterOp::Nilike
                if full_scan_at != Some(at) => {}
            // _ne/_nin/_like/_nlike use full index scan with post-filtering (matches Go behavior)
            // For JSON fields, we still need to track the path to constrain the scan
            FilterOp::Ne | FilterOp::Nin => {
                has_scan_all = true;
                scan_json_path = cond.json_path.clone();
            }
            FilterOp::Like | FilterOp::Nlike | FilterOp::Ilike | FilterOp::Nilike => {
                has_scan_all = true;
                if let ConditionValue::Pattern(pattern) = &cond.value {
                    scan_value_filter = Some(match cond.op {
                        FilterOp::Like => ScanValueFilter::Like(pattern.clone()),
                        FilterOp::Nlike => ScanValueFilter::Nlike(pattern.clone()),
                        FilterOp::Ilike => ScanValueFilter::Ilike(pattern.clone()),
                        FilterOp::Nilike => ScanValueFilter::Nilike(pattern.clone()),
                        _ => unreachable!(),
                    });
                    value_filter_cond = Some(cond);
                }
                scan_json_path = cond.json_path.clone();
            }
            _ => {}
        }
    }

    // Determine if we need to reverse the scan based on ordering
    let reverse = order_by
        .map(|o| filtered_index_order_prefix(o, index, filter))
        .map(|(prefix, needs_reverse)| prefix > 0 && needs_reverse)
        .unwrap_or(false);

    // For descending indexes, range bounds must be swapped because the encoding
    // reverses the byte order: higher values have lower encoded bytes.
    // e.g., _gt:30 on DESC index → upper_bound (not lower) in byte order.
    let first_field_descending = index.fields.first().map(|f| f.descending).unwrap_or(false);
    if first_field_descending {
        std::mem::swap(&mut lower_bound, &mut upper_bound);
    }

    // For composite indexes, check if subsequent fields also have eq conditions.
    // If all fields are matched exactly, use ExactMatch; otherwise use PrefixScan.
    //
    // Note: We exclude _none and _all array operators here because they should be
    // handled as residual filters, not used to narrow the index scan.
    // - _none: cannot narrow scan (requires checking ALL elements don't match)
    // - _all: index provides candidates but residual filter verifies ALL match
    let is_composite = index.fields.len() > 1;
    let subsequent_eq_values = if is_composite && (has_eq || has_in) {
        suffix_eq_values(index, &conditions, collection_fields)?
    } else {
        Vec::new()
    };
    let all_fields_matched =
        is_composite && has_eq && subsequent_eq_values.len() == index.fields.len() - 1;

    // Normalize filter values to match schema field encoding types.
    // e.g., a Float32 field stores with encode_float32, so lookup values must be Float32.
    if let Some(ref mut v) = eq_value {
        *v = normalize_for_index_field(v.clone(), first_field, collection_fields);
    }
    if let Some(ref mut vs) = in_values {
        *vs = vs
            .iter()
            .map(|v| normalize_for_index_field(v.clone(), first_field, collection_fields))
            .collect();
    }
    lower_bound = match lower_bound {
        Bound::Inclusive(v) => {
            Bound::Inclusive(normalize_for_index_field(v, first_field, collection_fields))
        }
        Bound::Exclusive(v) => {
            Bound::Exclusive(normalize_for_index_field(v, first_field, collection_fields))
        }
        Bound::Unbounded => Bound::Unbounded,
        _ => Bound::Unbounded,
    };
    upper_bound = match upper_bound {
        Bound::Inclusive(v) => {
            Bound::Inclusive(normalize_for_index_field(v, first_field, collection_fields))
        }
        Bound::Exclusive(v) => {
            Bound::Exclusive(normalize_for_index_field(v, first_field, collection_fields))
        }
        Bound::Unbounded => Bound::Unbounded,
        _ => Bound::Unbounded,
    };

    // Determine scan type (narrowing scans take priority over full scans)
    // Wrap values in JsonLeafValue when JSON path is present
    //
    // For composite indexes where we only match the first field, we use PrefixScan
    // instead of ExactMatch because ExactMatchIterator expects the doc_id right
    // after the encoded values, but composite index keys have additional fields.
    let ranged = !lower_bound.is_unbounded() || !upper_bound.is_unbounded();
    // Only an `_all` scan wants the empty-array markers at the top of its path.
    let wants_markers = |cond: Option<&FieldCondition>| {
        cond.is_some_and(|cond| cond.array_op == Some(FilterOp::All))
    };
    let range_markers = wants_markers(first_range);
    let scan_all_markers =
        wants_markers(full_scan_at.and_then(|at| first_field_conditions.get(at)));
    let scan_type = if has_eq {
        let wrapped_value = wrap_value_for_json_path(eq_value.unwrap(), eq_json_path.as_ref());
        if all_fields_matched {
            // All fields of composite index have eq conditions - use ExactMatch
            let mut values = vec![wrapped_value];
            values.extend(subsequent_eq_values);
            IndexScanType::ExactMatch { values }
        } else if let Some((lower, upper)) = index
            .fields
            .get(1 + subsequent_eq_values.len())
            .and_then(|field| composite_range(&conditions, field, collection_fields))
        {
            let mut prefix_values = vec![wrapped_value];
            prefix_values.extend(subsequent_eq_values);
            IndexScanType::RangeScan {
                prefix_values,
                lower,
                upper,
                reverse,
            }
        } else if is_composite && !subsequent_eq_values.is_empty() {
            // Multiple consecutive fields matched but not all - use PrefixScan with all eq values
            let mut values = vec![wrapped_value];
            values.extend(subsequent_eq_values);
            IndexScanType::PrefixScan {
                prefix_values: values,
                reverse,
            }
        } else if is_composite {
            // Only first field matched - use PrefixScan with just first value
            IndexScanType::PrefixScan {
                prefix_values: vec![wrapped_value],
                reverse,
            }
        } else {
            IndexScanType::ExactMatch {
                values: vec![wrapped_value],
            }
        }
    } else if has_in {
        let mut wrapped_values =
            wrap_values_for_json_path(in_values.unwrap(), in_json_path.as_ref());
        // Without `order` the lookups run as the filter lists them, as Go's
        // do. With one the planner relies on the index for it, so they run in
        // the index's key order, backward when the scan is: `_in: [55, 25]`
        // must not come back as written. The keys are encoded after the JSON
        // wrap, so they are the ones the index stores.
        order_in_values(
            &mut wrapped_values,
            first_field_descending,
            order_by.map(|_| reverse),
        );
        // Eq values on the later fields make each lookup exact.
        IndexScanType::InScan {
            values: wrapped_values,
            suffix_values: subsequent_eq_values,
        }
    } else if !lower_bound.is_unbounded() || !upper_bound.is_unbounded() {
        // Wrap range bounds for JSON paths
        // When we have a JSON path and one bound is unbounded, we need to constrain
        // the scan to entries with the same path using PathMin/PathMax sentinels
        let lower = match lower_bound {
            Bound::Inclusive(v) => {
                Bound::Inclusive(wrap_value_for_json_path(v, range_json_path.as_ref()))
            }
            Bound::Exclusive(v) => {
                Bound::Exclusive(wrap_value_for_json_path(v, range_json_path.as_ref()))
            }
            Bound::Unbounded => {
                // For empty paths (top-level JSON), leave unbounded to match Go behavior:
                // Go scans from value to end of entire index, counting all keys.
                match &range_json_path {
                    Some(path) if !path.is_empty() => {
                        path_low_end(path, first_field_descending, range_markers)
                    }
                    _ => Bound::Unbounded,
                }
            }
            _ => Bound::Unbounded,
        };
        let upper = match upper_bound {
            Bound::Inclusive(v) => {
                Bound::Inclusive(wrap_value_for_json_path(v, range_json_path.as_ref()))
            }
            Bound::Exclusive(v) => {
                Bound::Exclusive(wrap_value_for_json_path(v, range_json_path.as_ref()))
            }
            Bound::Unbounded => match &range_json_path {
                Some(path) if !path.is_empty() => {
                    path_high_end(path, first_field_descending, range_markers)
                }
                _ => Bound::Unbounded,
            },
            _ => Bound::Unbounded,
        };
        IndexScanType::RangeScan {
            prefix_values: vec![],
            lower,
            upper,
            reverse,
        }
    } else if has_scan_all {
        // Full index scan with residual filter (for _ne, _like, etc.)
        // For JSON fields with non-empty path, constrain scan to that path.
        // For empty path (top-level JSON), use full prefix scan to match Go.
        match &scan_json_path {
            Some(path) if !path.is_empty() => IndexScanType::RangeScan {
                prefix_values: vec![],
                lower: path_low_end(path, first_field_descending, scan_all_markers),
                upper: path_high_end(path, first_field_descending, scan_all_markers),
                reverse,
            },
            _ => IndexScanType::PrefixScan {
                prefix_values: vec![],
                reverse,
            },
        }
    } else {
        return None;
    };
    let scan_type = with_empty_arrays(
        scan_type,
        &first_field_conditions,
        first_field,
        first_field_kind,
        first_field_descending,
        is_composite,
    );

    // The executor tests the pattern on the scanned entry, so it must name the
    // element the scan matched; the filter checks any other pattern.
    let scan_cond = if has_eq {
        eq_cond
    } else if has_in {
        in_cond
    } else if ranged {
        first_range
    } else {
        value_filter_cond
    };
    let same_element = scan_cond
        .zip(value_filter_cond)
        .is_some_and(|(scan, like)| {
            scan.json_path == like.json_path
                && (like.array_op != Some(FilterOp::Any) || scan.clause == like.clause)
        });
    if !same_element {
        scan_value_filter = None;
    }

    let scan_type = if first_field_is_json {
        with_json_empty_arrays(
            scan_type,
            &first_field_conditions,
            first_field,
            is_composite,
        )
    } else {
        scan_type
    };

    // Only pass limit/offset if the index provides ordering
    // (otherwise the limit needs to be applied after sorting)
    // A merge of scans is in no index order.
    let index_provides_ordering = !matches!(scan_type, IndexScanType::OrScan { .. })
        && order_by
            .map(|o| can_be_ordered_by_filtered_index(o, index, filter).0)
            .unwrap_or(false);

    Some(IndexScanParams {
        index_name: index.name.clone(),
        scan_type,
        limit: if index_provides_ordering { limit } else { None },
        offset: if index_provides_ordering { offset } else { 0 },
        value_filter: scan_value_filter,
        cursor_seek: None,
    })
}

fn field_kind<'a>(
    collection_fields: &'a [schema::FieldDescription],
    name: &str,
) -> Option<&'a FieldKind> {
    collection_fields
        .iter()
        .find(|f| f.name == name)
        .map(|f| &f.kind)
}

fn is_json_field(kind: Option<&FieldKind>) -> bool {
    kind.and_then(FieldKind::as_scalar)
        .is_some_and(|kind| kind.base_kind() == ScalarKind::NillableJson)
}

/// `cond` at the path its JSON index entries are stored under.
fn at_entry_path(mut cond: FieldCondition, is_json: bool) -> FieldCondition {
    if is_json && cond.json_path.is_none() {
        // Under `_any`/`_all` only array elements match, as Go's index marker.
        cond.json_path = Some(match cond.array_op {
            Some(_) => JsonPath::new().append_index(),
            None => JsonPath::new(),
        });
    }
    cond
}

/// The `_eq` values of the later index fields, or `None` when one of them
/// needs a full scan.
fn suffix_eq_values(
    index: &IndexDescription,
    conditions: &[FieldCondition],
    collection_fields: &[schema::FieldDescription],
) -> Option<Vec<NormalValue>> {
    let mut values = Vec::new();
    for field_desc in index.fields.iter().skip(1) {
        let Some(cond) = conditions.iter().find(|c| {
            c.field_name == field_desc.name
                && c.op == FilterOp::Eq
                && c.array_op != Some(FilterOp::None)
                && c.array_op != Some(FilterOp::All)
        }) else {
            break;
        };
        let ConditionValue::Single(v) = &cond.value else {
            break;
        };
        let kind = field_kind(collection_fields, &field_desc.name);
        if should_fallback_to_full_scan(cond, is_json_field(kind), kind) {
            return None;
        }
        let normalized = normalize_for_index_field(v.clone(), &field_desc.name, collection_fields);
        values.push(wrap_value_for_json_path(
            normalized,
            cond.json_path.as_ref(),
        ));
    }
    Some(values)
}

/// Check if a filter with a top-level `_or` can use any of the given indexes.
///
/// Returns true if the filter has an `_or` whose branches ALL use the same index.
pub fn can_or_filter_use_index(filter: &Filter, indexes: &[IndexDescription]) -> bool {
    let branches = match extract_or_branches(filter) {
        Some(b) => b,
        None => return false,
    };
    indexes
        .iter()
        .any(|index| branches.iter().all(|branch| can_use_index(branch, index)))
}

/// Extract OR branches from a filter's top-level `_or` condition.
///
/// Returns `Some(branches)` if the filter has a single top-level `_or` with
/// valid sub-filters, `None` otherwise.
fn extract_or_branches(filter: &Filter) -> Option<Vec<Filter>> {
    let conditions = filter.conditions();
    let or_value = conditions.get("_or")?;
    let arr = or_value.as_array()?;
    let branches: Vec<Filter> = arr
        .iter()
        .filter_map(|item| {
            item.as_object()
                .map(|obj| Filter::from_conditions(obj.clone()))
        })
        .collect();
    if branches.len() == arr.len() && !branches.is_empty() {
        Some(branches)
    } else {
        None
    }
}

/// Try to convert an OR filter to index scan parameters.
///
/// Detects top-level `_or` in the filter, extracts each branch, and checks
/// if ALL branches can use the same index. If so, returns `OrScan` with
/// one sub-scan per branch.
///
/// Matches Go's `newMultiIndexIteratorForOrOp` / `extractOrBranches` pattern.
pub fn or_filter_to_index_scan(
    filter: &Filter,
    indexes: &[IndexDescription],
    collection_fields: &[schema::FieldDescription],
) -> Option<IndexScanParams> {
    let branches = extract_or_branches(filter)?;

    // Try each index to see if all branches can use it
    for index in indexes {
        let mut branch_scans = Vec::new();
        let mut all_work = true;

        for branch in &branches {
            if let Some(params) =
                filter_to_index_scan(branch, index, None, collection_fields, None, 0)
            {
                branch_scans.push(params.scan_type);
            } else {
                all_work = false;
                break;
            }
        }

        if all_work && !branch_scans.is_empty() {
            return Some(IndexScanParams {
                index_name: index.name.clone(),
                scan_type: IndexScanType::OrScan {
                    branches: branch_scans,
                },
                limit: None,
                offset: 0,
                value_filter: None,
                cursor_seek: None,
            });
        }
    }

    None
}
