use document::{JsonLeafValue, JsonPath, JsonScalarValue, NormalValue};
use schema::{
    CType, FieldDescription, FieldKind, IndexDescription, IndexedFieldDescription, ScalarKind,
};
use serde_json::json;
use storage::index::Bound;

use crate::mapper::Filter;

use super::{filter_to_index_scan, IndexScanType};

fn index() -> IndexDescription {
    IndexDescription {
        id: 1,
        name: "custom_idx".to_string(),
        unique: false,
        kind: None,
        auto_generated: false,
        fields: vec![IndexedFieldDescription {
            name: "custom".to_string(),
            descending: false,
        }],
    }
}

fn fields() -> Vec<FieldDescription> {
    vec![FieldDescription::new(
        "custom",
        "custom",
        FieldKind::Scalar(ScalarKind::NillableJson),
        CType::LwwRegister,
    )]
}

fn scan(filter: serde_json::Value) -> Option<IndexScanType> {
    let filter = Filter::from_conditions(json!({ "custom": filter }).as_object().unwrap().clone());
    filter_to_index_scan(&filter, &index(), None, &fields(), None, 0).map(|p| p.scan_type)
}

fn marker(name: &str) -> NormalValue {
    let path = JsonPath::new().append_property(name).append_index();
    NormalValue::JsonLeaf(JsonLeafValue::new(path, JsonScalarValue::EmptyArray))
}

fn marker_branch(scan: &IndexScanType) -> Option<&NormalValue> {
    let IndexScanType::OrScan { branches } = scan else {
        return None;
    };
    match branches.get(1)? {
        IndexScanType::ExactMatch { values } => values.first(),
        _ => None,
    }
}

#[test]
fn filters_the_index_cannot_answer_take_a_full_scan() {
    for filter in [
        json!({"_all": {"_ne": 5}}),
        json!({"_all": {"_nin": [5]}}),
        json!({"_all": {"_nlike": "x%"}}),
        json!({"_any": {"_ne": 5}}),
        json!({"_all": {"_all": {"_eq": 1}}}),
        json!({"_any": {"_all": {"_eq": 1}}}),
        json!({"_none": {"_all": {"_eq": 1}}}),
        json!({"_all": {"a": {"_eq": 1}}}),
        json!({"tags": {"_all": {"a": {"_eq": 1}}}}),
        json!({"tags": {"_any": {"a": {"_eq": 1}}}}),
        json!({"tags": {"_all": {"_in": [1, {"x": 1}]}}}),
        json!({"tags": {"_in": [1, [2]]}}),
        json!({"title": {"_eq": null}}),
        json!({"title": {"_in": [null, 5]}}),
    ] {
        assert!(scan(filter.clone()).is_none(), "{filter}");
    }
}

#[test]
fn a_top_level_all_looks_up_the_top_level_marker() {
    let scan = scan(json!({"_all": {"_eq": 1}})).expect("an index scan");
    let top = NormalValue::JsonLeaf(JsonLeafValue::new(
        JsonPath::new().append_index(),
        JsonScalarValue::EmptyArray,
    ));
    assert_eq!(marker_branch(&scan), Some(&top));
}

#[test]
fn bare_negation_at_a_path_takes_a_full_scan() {
    assert!(scan(json!({"tags": {"_ne": 5}})).is_none());
    assert!(scan(json!({"tags": {"_nin": [5]}})).is_none());
}

#[test]
fn bare_negation_at_a_path_leaves_the_scan_to_another_condition() {
    let scan = scan(json!({"a": {"_eq": 1}, "tags": {"_ne": 5}})).unwrap();
    assert!(matches!(scan, IndexScanType::ExactMatch { .. }), "{scan:?}");
}

#[test]
fn bare_negation_at_the_top_reads_every_entry() {
    for filter in [json!({"_ne": 5}), json!({"_nin": [5]})] {
        let scan = scan(filter.clone()).unwrap();
        assert!(
            matches!(&scan, IndexScanType::PrefixScan { prefix_values, .. } if prefix_values.is_empty()),
            "{filter}: {scan:?}"
        );
    }
}

#[test]
fn an_any_on_the_same_array_skips_the_marker() {
    let scan = scan(json!({"tags": {"_all": {"_eq": 1}, "_any": {"_eq": 2}}})).unwrap();
    assert!(marker_branch(&scan).is_none());
}

#[test]
fn an_any_on_another_array_keeps_the_marker() {
    let scan = scan(json!({"a": {"_all": {"_eq": 1}}, "b": {"_any": {"_eq": 2}}})).unwrap();
    assert_eq!(marker_branch(&scan), Some(&marker("a")));
}

#[test]
fn a_range_without_all_stops_below_the_markers() {
    for (filter, descending) in [
        (json!({"tags": {"_any": {"_gt": 1}}}), false),
        (json!({"tags": {"_any": {"_gt": 1}}}), true),
        (json!({"tags": {"_any": {"_like": "x%"}}}), false),
        (json!({"tags": {"_any": {"_like": "x%"}}}), true),
    ] {
        let filter =
            Filter::from_conditions(json!({ "custom": filter }).as_object().unwrap().clone());
        let mut index = index();
        index.fields[0].descending = descending;
        let Some(IndexScanType::RangeScan { lower, upper, .. }) =
            filter_to_index_scan(&filter, &index, None, &fields(), None, 0).map(|p| p.scan_type)
        else {
            panic!("no range scan");
        };
        let top = if descending { lower } else { upper };
        assert_eq!(
            top,
            Bound::Exclusive(marker("tags")),
            "descending {descending}"
        );
    }
}

#[test]
fn a_top_level_in_looks_null_up_as_stored() {
    let Some(IndexScanType::InScan { values, .. }) = scan(json!({"_in": [null, 5]})) else {
        panic!("no in scan");
    };
    assert!(values.contains(&NormalValue::Null), "{values:?}");
}

#[test]
fn null_equality_under_any_keeps_the_index() {
    assert!(scan(json!({"tags": {"_any": {"_eq": null}}})).is_some());
}

fn keeps_value_filter(filter: serde_json::Value) -> bool {
    let filter = Filter::from_conditions(filter.as_object().unwrap().clone());
    filter_to_index_scan(&filter, &index(), None, &fields(), None, 0)
        .unwrap()
        .value_filter
        .is_some()
}

#[test]
fn a_pattern_from_another_any_clause_is_left_to_the_filter() {
    let any = |op: serde_json::Value| json!({"custom": {"tags": {"_any": op}}});
    for scan in [json!({"_eq": 5}), json!({"_lt": 100})] {
        let filter = json!({"_and": [any(scan), any(json!({"_like": "x%"}))]});
        assert!(!keeps_value_filter(filter.clone()), "{filter}");
    }
    assert!(keeps_value_filter(any(json!({"_lt": 100, "_like": "x%"}))));
}
