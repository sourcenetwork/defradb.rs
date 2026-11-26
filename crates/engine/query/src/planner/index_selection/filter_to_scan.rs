//! Core filter-to-index-scan conversion.

use document::{JsonLeafValue, JsonPath, JsonScalarValue, NormalValue};
use rapidhash::RapidHashSet;
use schema::{FieldKind, IndexDescription, ScalarKind};
use storage::index::Bound;

use crate::mapper::{Filter, FilterOp, OrderBy};

use super::conditions::{
    can_be_ordered_by_filtered_index, can_use_index, extract_field_conditions,
    filtered_index_order_prefix, should_fallback_to_full_scan,
};
use super::empty_array::with_empty_arrays;
use super::types::{ConditionValue, IndexScanParams, IndexScanType, ScanValueFilter};
use super::values::{
    normalize_for_index_field, wrap_value_for_json_path, wrap_values_for_json_path,
};

fn composite_range(
    conditions: &[super::types::FieldCondition],
    field: &schema::IndexedFieldDescription,
    collection_fields: &[schema::FieldDescription],
) -> Option<(Bound, Bound)> {
    if collection_fields
        .iter()
        .find(|candidate| candidate.name == field.name)
        .is_some_and(|candidate| {
            candidate.kind.is_array()
                || candidate
                    .kind
                    .as_scalar()
                    .is_some_and(|kind| kind.base_kind() == ScalarKind::NillableJson)
        })
    {
        return None;
    }
    let mut lower = Bound::Unbounded;
    let mut upper = Bound::Unbounded;
    let mut found = false;
    for condition in conditions.iter().filter(|condition| {
        condition.field_name == field.name
            && condition.array_op.is_none()
            && condition.json_path.is_none()
    }) {
        let ConditionValue::Single(value) = &condition.value else {
            continue;
        };
        let value = normalize_for_index_field(value.clone(), &field.name, collection_fields);
        match condition.op {
            FilterOp::Gt => lower = Bound::Exclusive(value),
            FilterOp::Gte => lower = Bound::Inclusive(value),
            FilterOp::Lt => upper = Bound::Exclusive(value),
            FilterOp::Lte => upper = Bound::Inclusive(value),
            _ => continue,
        }
        found = true;
    }
    if field.descending {
        std::mem::swap(&mut lower, &mut upper);
    }
    found.then_some((lower, upper))
}

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

    let conditions = extract_field_conditions(filter);
    let first_field = &index.fields[0].name;

    // Check if the first index field is JSON-typed
    let first_field_kind = collection_fields
        .iter()
        .find(|f| &f.name == first_field)
        .map(|f| &f.kind);
    let first_field_is_json = first_field_kind
        .and_then(FieldKind::as_scalar)
        .is_some_and(|kind| kind.base_kind() == ScalarKind::NillableJson);

    // Find conditions on the first index field
    // For JSON fields, ensure top-level conditions get an empty json_path
    let first_field_conditions: Vec<_> = conditions
        .iter()
        .map(|c| {
            if &c.field_name == first_field && first_field_is_json && c.json_path.is_none() {
                // Top-level JSON filter (e.g., custom: {_gt: 20}) needs an empty path.
                // Under `_any`/`_all` only array elements match, as Go's index marker.
                let mut c = c.clone();
                c.json_path = Some(match c.array_op {
                    Some(_) => JsonPath::new().append_index(),
                    None => JsonPath::new(),
                });
                c
            } else {
                c.clone()
            }
        })
        .filter(|c| &c.field_name == first_field)
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
    let mut eq_json_path: Option<JsonPath> = None;
    let mut has_in = false;
    let mut in_values = None;
    let mut in_json_path: Option<JsonPath> = None;
    let mut has_scan_all = false;
    let mut scan_value_filter: Option<ScanValueFilter> = None;
    let mut value_filter_path: Option<JsonPath> = None;
    let mut lower_bound = Bound::Unbounded;
    let mut upper_bound = Bound::Unbounded;
    let mut range_json_path: Option<JsonPath> = None;
    let mut scan_json_path: Option<JsonPath> = None;
    // Bounds on two paths cannot share one scan, and neither can bounds from
    // two `_any` clauses, so only the first range bounds it and the filter
    // checks the rest.
    let first_range = first_field_conditions.iter().find(|cond| {
        cond.array_op != Some(FilterOp::None)
            && matches!(
                cond.op,
                FilterOp::Gt | FilterOp::Gte | FilterOp::Lt | FilterOp::Lte
            )
            && matches!(cond.value, ConditionValue::Single(_))
    });
    let in_range = |cond: &super::types::FieldCondition| {
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
    // The `_all` paths behind each kind of scan, whose empty arrays it must add.
    let mut eq_all = None;
    let mut in_all = None;
    let mut range_all = None;
    let mut scan_all_all = None;

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
                    eq_json_path = cond.json_path.clone();
                    eq_all = all_path(cond, first_field_is_json);
                }
            }
            FilterOp::In if !has_in => {
                if let ConditionValue::Multiple(vs) = &cond.value {
                    has_in = true;
                    in_values = Some(vs.clone());
                    in_json_path = cond.json_path.clone();
                    in_all = all_path(cond, first_field_is_json);
                }
            }
            FilterOp::Gt if in_range(cond) => {
                if let ConditionValue::Single(v) = &cond.value {
                    lower_bound = Bound::Exclusive(v.clone());
                    range_json_path = cond.json_path.clone();
                    range_all = range_all.or(all_path(cond, first_field_is_json));
                }
            }
            FilterOp::Gte if in_range(cond) => {
                if let ConditionValue::Single(v) = &cond.value {
                    lower_bound = Bound::Inclusive(v.clone());
                    range_json_path = cond.json_path.clone();
                    range_all = range_all.or(all_path(cond, first_field_is_json));
                }
            }
            FilterOp::Lt if in_range(cond) => {
                if let ConditionValue::Single(v) = &cond.value {
                    upper_bound = Bound::Exclusive(v.clone());
                    range_json_path = cond.json_path.clone();
                    range_all = range_all.or(all_path(cond, first_field_is_json));
                }
            }
            FilterOp::Lte if in_range(cond) => {
                if let ConditionValue::Single(v) = &cond.value {
                    upper_bound = Bound::Inclusive(v.clone());
                    range_json_path = cond.json_path.clone();
                    range_all = range_all.or(all_path(cond, first_field_is_json));
                }
            }
            // One condition pins the scan path, so its value filter and
            // empty-array lookup read the same path; the filter checks the others.
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
                scan_all_all = all_path(cond, first_field_is_json);
                if cond.json_path.is_some() || scan_all_all.is_some() {
                    scan_json_path = cond.json_path.clone();
                }
            }
            FilterOp::Like | FilterOp::Nlike | FilterOp::Ilike | FilterOp::Nilike => {
                has_scan_all = true;
                scan_all_all = all_path(cond, first_field_is_json);
                if let ConditionValue::Pattern(pattern) = &cond.value {
                    scan_value_filter = Some(match cond.op {
                        FilterOp::Like => ScanValueFilter::Like(pattern.clone()),
                        FilterOp::Nlike => ScanValueFilter::Nlike(pattern.clone()),
                        FilterOp::Ilike => ScanValueFilter::Ilike(pattern.clone()),
                        FilterOp::Nilike => ScanValueFilter::Nilike(pattern.clone()),
                        _ => unreachable!(),
                    });
                    value_filter_path = cond.json_path.clone();
                }
                if cond.json_path.is_some() || scan_all_all.is_some() {
                    scan_json_path = cond.json_path.clone();
                }
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
    let mut subsequent_eq_values: Vec<NormalValue> = Vec::new();
    if is_composite && has_eq {
        for field_desc in index.fields.iter().skip(1) {
            let field_cond = conditions.iter().find(|c| {
                c.field_name == field_desc.name
                    && c.op == FilterOp::Eq
                    && c.array_op != Some(FilterOp::None)
                    && c.array_op != Some(FilterOp::All)
            });
            if let Some(cond) = field_cond {
                if let ConditionValue::Single(v) = &cond.value {
                    subsequent_eq_values
                        .push(wrap_value_for_json_path(v.clone(), cond.json_path.as_ref()));
                } else {
                    break;
                }
            } else {
                break;
            }
        }
    }
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
    for (i, v) in subsequent_eq_values.iter_mut().enumerate() {
        if let Some(field_desc) = index.fields.get(i + 1) {
            *v = normalize_for_index_field(v.clone(), &field_desc.name, collection_fields);
        }
    }

    // Determine scan type (narrowing scans take priority over full scans)
    // Wrap values in JsonLeafValue when JSON path is present
    //
    // For composite indexes where we only match the first field, we use PrefixScan
    // instead of ExactMatch because ExactMatchIterator expects the doc_id right
    // after the encoded values, but composite index keys have additional fields.
    let ranged = !lower_bound.is_unbounded() || !upper_bound.is_unbounded();
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
        // For composite indexes, check if subsequent fields have Eq conditions.
        // If so, attach them as suffix_values to enable exact-match lookups
        // instead of prefix scans (e.g., _in on first field + _eq on second).
        let mut in_suffix_values: Vec<NormalValue> = Vec::new();
        if is_composite {
            for field_desc in index.fields.iter().skip(1) {
                let field_cond = conditions.iter().find(|c| {
                    c.field_name == field_desc.name
                        && c.op == FilterOp::Eq
                        && c.array_op != Some(FilterOp::None)
                        && c.array_op != Some(FilterOp::All)
                });
                if let Some(cond) = field_cond {
                    if let ConditionValue::Single(v) = &cond.value {
                        let normalized = normalize_for_index_field(
                            v.clone(),
                            &field_desc.name,
                            collection_fields,
                        );
                        in_suffix_values.push(wrap_value_for_json_path(
                            normalized,
                            cond.json_path.as_ref(),
                        ));
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
        }
        IndexScanType::InScan {
            values: wrapped_values,
            suffix_values: in_suffix_values,
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
                    Some(path) if !path.is_empty() => path_low_end(path, first_field_descending),
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
                Some(path) if !path.is_empty() => path_high_end(path, first_field_descending),
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
        if let Some(path) = &scan_json_path {
            if !path.is_empty() {
                IndexScanType::RangeScan {
                    prefix_values: vec![],
                    lower: path_low_end(path, first_field_descending),
                    upper: path_high_end(path, first_field_descending),
                    reverse,
                }
            } else {
                full_field_range(reverse, first_field_is_json)
            }
        } else {
            full_field_range(reverse, first_field_is_json)
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

    // The executor tests the pattern on the scanned value, so a pattern from
    // another path would drop matches; the filter checks it instead.
    let scan_path = if has_eq {
        &eq_json_path
    } else if has_in {
        &in_json_path
    } else if ranged {
        &range_json_path
    } else {
        &value_filter_path
    };
    if *scan_path != value_filter_path {
        scan_value_filter = None;
    }

    let all = if has_eq {
        eq_all
    } else if has_in {
        in_all
    } else if ranged {
        range_all
    } else {
        scan_all_all
    };
    if let Some(path) = all {
        return Some(IndexScanParams {
            index_name: index.name.clone(),
            scan_type: IndexScanType::OrScan {
                branches: vec![scan_type, empty_array_scan(path, is_composite)],
            },
            limit: None,
            offset: 0,
            value_filter: None,
            cursor_seek: None,
        });
    }

    // Only pass limit/offset if the index provides ordering
    // (otherwise the limit needs to be applied after sorting)
    let index_provides_ordering = order_by
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

/// A bound below every value at `path`. A descending index inverts the
/// bytes, so `PathMax` is its low end.
fn path_low_end(path: &JsonPath, descending: bool) -> Bound {
    let sentinel = if descending {
        JsonScalarValue::PathMax
    } else {
        JsonScalarValue::PathMin
    };
    Bound::Inclusive(NormalValue::JsonLeaf(JsonLeafValue::new(
        path.clone(),
        sentinel,
    )))
}

/// A bound above every value at `path`. On a descending index `PathMin`
/// sorts after the nulls, the last values of the path.
fn path_high_end(path: &JsonPath, descending: bool) -> Bound {
    if descending {
        Bound::Inclusive(NormalValue::JsonLeaf(JsonLeafValue::new(
            path.clone(),
            JsonScalarValue::PathMin,
        )))
    } else {
        Bound::Exclusive(NormalValue::JsonLeaf(JsonLeafValue::new(
            path.clone(),
            JsonScalarValue::PathMax,
        )))
    }
}

/// Every entry of the first field. On a JSON field a range, not a prefix, so
/// its empty array entries, which match no condition but `_all`, stay out.
fn full_field_range(reverse: bool, is_json: bool) -> IndexScanType {
    if !is_json {
        return IndexScanType::PrefixScan {
            prefix_values: vec![],
            reverse,
        };
    }
    IndexScanType::RangeScan {
        prefix_values: vec![],
        lower: Bound::Unbounded,
        upper: Bound::Unbounded,
        reverse,
    }
}

/// A condition served by a scan of every entry at its path.
fn is_full_scan(cond: &super::types::FieldCondition) -> bool {
    cond.array_op != Some(FilterOp::None)
        && matches!(
            cond.op,
            FilterOp::Ne
                | FilterOp::Nin
                | FilterOp::Like
                | FilterOp::Nlike
                | FilterOp::Ilike
                | FilterOp::Nilike
        )
}

/// A `_nin` outside an array operator, which a missing property passes.
fn accepts_missing(cond: &super::types::FieldCondition) -> bool {
    cond.op == FilterOp::Nin && cond.array_op.is_none()
}

/// The JSON array path of an `_all` condition: `None` for any other condition.
/// A typed array finds its empty arrays through `with_empty_arrays`.
fn all_path(cond: &super::types::FieldCondition, is_json: bool) -> Option<JsonPath> {
    if !is_json || cond.array_op != Some(FilterOp::All) {
        return None;
    }
    let mut path = cond.json_path.clone().unwrap_or_default();
    path.0.pop();
    Some(path)
}

/// The lookup of empty JSON arrays at `path`, which `_all` matches and no range finds.
fn empty_array_scan(path: JsonPath, is_composite: bool) -> IndexScanType {
    let marker = NormalValue::JsonLeaf(JsonLeafValue::new(path, JsonScalarValue::EmptyArray));
    if is_composite {
        IndexScanType::PrefixScan {
            prefix_values: vec![marker],
            reverse: false,
        }
    } else {
        IndexScanType::ExactMatch {
            values: vec![marker],
        }
    }
}

/// `values`, each once. With `scan_reverse` set they go in the order the
/// index holds their keys, backward when it is `true`; without it they keep
/// the order they came in. A value that does not encode is never a duplicate,
/// and sorts after those that do.
fn order_in_values(values: &mut Vec<NormalValue>, descending: bool, scan_reverse: Option<bool>) {
    let mut seen = RapidHashSet::default();
    let mut keyed: Vec<(Option<Vec<u8>>, NormalValue)> = values
        .drain(..)
        .filter_map(|value| {
            let key = storage::field_value::encode_field_value(Vec::new(), &value, descending).ok();
            match &key {
                Some(key) if !seen.insert(key.clone()) => None,
                _ => Some((key, value)),
            }
        })
        .collect();
    if let Some(reverse) = scan_reverse {
        keyed.sort_by(|(a, _), (b, _)| match (a, b) {
            (Some(a), Some(b)) => a.cmp(b),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        });
        if reverse {
            keyed.reverse();
        }
    }
    values.extend(keyed.into_iter().map(|(_, value)| value));
}
