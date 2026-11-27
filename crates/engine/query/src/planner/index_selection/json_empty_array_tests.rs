use document::{JsonLeafValue, JsonPath, JsonScalarValue, NormalValue};
use schema::{
    CType, FieldDescription, FieldKind, IndexDescription, IndexedFieldDescription, ScalarKind,
};
use serde_json::json;

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

fn marker(path: &[&str]) -> NormalValue {
    let path = path
        .iter()
        .fold(JsonPath::new(), |path, name| path.append_property(name));
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

fn assert_marker_at(filter: serde_json::Value, path: &[&str]) {
    let scan = scan(filter.clone()).unwrap_or_else(|| panic!("no index scan for {filter}"));
    let found = marker_branch(&scan).unwrap_or_else(|| panic!("no marker branch for {filter}"));
    assert_eq!(found, &marker(path), "{filter}");
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
    ] {
        assert!(scan(filter.clone()).is_none(), "{filter}");
    }
}

#[test]
fn all_eq_looks_up_the_marker_at_the_array_path() {
    assert_marker_at(json!({"_all": {"_eq": 1}}), &[]);
    assert_marker_at(json!({"tags": {"_all": {"_eq": 1}}}), &["tags"]);
    assert_marker_at(json!({"tags": {"_all": {"a": {"_eq": 1}}}}), &["tags"]);
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
    assert_eq!(marker_branch(&scan), Some(&marker(&["a"])));
}
