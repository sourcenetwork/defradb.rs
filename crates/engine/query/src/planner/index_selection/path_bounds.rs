//! Scan bounds and key order for JSON paths.

use document::{JsonLeafValue, JsonPath, JsonPathPart, JsonScalarValue, NormalValue};
use rapidhash::RapidHashSet;
use schema::ScalarKind;
use storage::index::Bound;

use super::types::{ConditionValue, FieldCondition};
use super::values::normalize_for_index_field;
use crate::mapper::FilterOp;

/// A bound below every value at `path`. A descending index inverts the
/// bytes, so its low end is the top of the path, where the empty-array
/// markers sit next to `PathMax`.
pub(super) fn path_low_end(path: &JsonPath, descending: bool, markers: bool) -> Bound {
    if descending {
        path_top(path, markers, Bound::Inclusive)
    } else {
        Bound::Inclusive(at_path(path, JsonScalarValue::PathMin))
    }
}

/// A bound above every value at `path`. On a descending index `PathMin`
/// sorts after the nulls, the last values of the path.
pub(super) fn path_high_end(path: &JsonPath, descending: bool, markers: bool) -> Bound {
    if descending {
        Bound::Inclusive(at_path(path, JsonScalarValue::PathMin))
    } else {
        path_top(path, markers, Bound::Exclusive)
    }
}

/// The top end of `path`. Without `markers` it stops before the empty-array
/// markers, which only element paths hold, so a scan that cannot match them
/// never reads them.
fn path_top(path: &JsonPath, markers: bool, past_max: fn(NormalValue) -> Bound) -> Bound {
    if markers || path.0.last() != Some(&JsonPathPart::Index) {
        past_max(at_path(path, JsonScalarValue::PathMax))
    } else {
        Bound::Exclusive(at_path(path, JsonScalarValue::EmptyArray))
    }
}

fn at_path(path: &JsonPath, value: JsonScalarValue) -> NormalValue {
    NormalValue::JsonLeaf(JsonLeafValue::new(path.clone(), value))
}

/// Each value once, in index key order when the scan's order is relied on.
pub(super) fn order_in_values(
    values: &mut Vec<NormalValue>,
    descending: bool,
    scan_reverse: Option<bool>,
) {
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
            // A value that does not encode has no key to compare, so it goes last.
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

pub(super) fn composite_range(
    conditions: &[FieldCondition],
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
