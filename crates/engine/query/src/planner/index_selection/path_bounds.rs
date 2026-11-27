//! Scan bounds and key order for JSON paths.

use document::{JsonLeafValue, JsonPath, JsonScalarValue, NormalValue};
use rapidhash::RapidHashSet;
use schema::ScalarKind;
use storage::index::Bound;

use super::types::{ConditionValue, FieldCondition};
use super::values::normalize_for_index_field;
use crate::mapper::FilterOp;

/// A bound below every value at `path`. A descending index inverts the
/// bytes, so `PathMax` is its low end.
pub(super) fn path_low_end(path: &JsonPath, descending: bool) -> Bound {
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
pub(super) fn path_high_end(path: &JsonPath, descending: bool) -> Bound {
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

/// `values`, each once. With `scan_reverse` set they go in the order the
/// index holds their keys, backward when it is `true`; without it they keep
/// the order they came in. A value that does not encode is never a duplicate,
/// and sorts after those that do.
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
