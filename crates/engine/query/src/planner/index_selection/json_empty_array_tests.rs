use document::{JsonLeafValue, JsonPath, JsonScalarValue, NormalValue};
use schema::{FieldDescription, FieldKind, IndexDescription, IndexedFieldDescription, ScalarKind};
use serde_json::json;

use crate::mapper::Filter;

use super::{filter_to_index_scan, IndexScanType};

fn index(fields: &[(&str, bool)]) -> IndexDescription {
    IndexDescription {
        id: 1,
        name: "custom_idx".to_string(),
        unique: false,
        kind: None,
        auto_generated: false,
        fields: fields
            .iter()
            .map(|(name, descending)| IndexedFieldDescription {
                name: name.to_string(),
                descending: *descending,
            })
            .collect(),
    }
}

fn fields() -> Vec<FieldDescription> {
    vec![
        FieldDescription::new(
            "custom",
            "custom",
            FieldKind::Scalar(ScalarKind::NillableJson),
            schema::CType::LwwRegister,
        ),
        FieldDescription::new(
            "name",
            "name",
            FieldKind::Scalar(ScalarKind::NillableString),
            schema::CType::LwwRegister,
        ),
    ]
}

fn scan(custom: serde_json::Value, index_fields: &[(&str, bool)]) -> IndexScanType {
    let filter = Filter::from_conditions(json!({ "custom": custom }).as_object().unwrap().clone());
    filter_to_index_scan(&filter, &index(index_fields), None, &fields(), None, 0)
        .unwrap()
        .scan_type
}

fn marker() -> NormalValue {
    NormalValue::JsonLeaf(JsonLeafValue::new(
        JsonPath::new().append_property("a").append_index(),
        JsonScalarValue::EmptyArray,
    ))
}

fn marker_branch(scan: &IndexScanType) -> Option<&IndexScanType> {
    match scan {
        IndexScanType::OrScan { branches } => branches.get(1),
        _ => None,
    }
}

#[test]
fn all_eq_also_reads_the_empty_array_marker() {
    let scan = scan(json!({"a": {"_all": {"_eq": 1}}}), &[("custom", false)]);
    match marker_branch(&scan) {
        Some(IndexScanType::ExactMatch { values }) => assert_eq!(values, &vec![marker()]),
        other => panic!("expected a marker ExactMatch branch, got {other:?}"),
    }
}

#[test]
fn all_on_a_composite_index_reads_every_marker_entry() {
    let scan = scan(
        json!({"a": {"_all": {"_eq": 1}}}),
        &[("custom", false), ("name", false)],
    );
    match marker_branch(&scan) {
        Some(IndexScanType::PrefixScan { prefix_values, .. }) => {
            assert_eq!(prefix_values, &vec![marker()]);
        }
        other => panic!("expected a marker PrefixScan branch, got {other:?}"),
    }
}

#[test]
fn all_on_two_paths_reads_a_marker_for_each_path() {
    let scan = scan(
        json!({
            "a": {"_all": {"_gt": 0}},
            "b": {"_all": {"_eq": 1}}
        }),
        &[("custom", false)],
    );
    let IndexScanType::OrScan { branches } = scan else {
        panic!("expected an OrScan, got {scan:?}");
    };
    assert_eq!(branches.len(), 3);
    let marker_paths: Vec<&JsonPath> = branches
        .iter()
        .filter_map(|branch| match branch {
            IndexScanType::ExactMatch { values } => values.first(),
            _ => None,
        })
        .filter_map(|value| match value {
            NormalValue::JsonLeaf(leaf) if leaf.is_empty_array_marker() => Some(&leaf.path),
            _ => None,
        })
        .collect();
    assert!(marker_paths.contains(&&JsonPath::new().append_property("a").append_index()));
    assert!(marker_paths.contains(&&JsonPath::new().append_property("b").append_index()));
}

#[test]
fn all_below_a_bound_reads_the_marker_as_a_branch() {
    for descending in [false, true] {
        let scan = scan(
            json!({"a": {"_all": {"_lt": 2}}}),
            &[("custom", descending)],
        );
        assert!(
            matches!(marker_branch(&scan), Some(IndexScanType::ExactMatch { .. })),
            "descending {descending}: {scan:?}"
        );
    }
}

#[test]
fn all_up_to_the_top_of_the_path_reads_the_marker_once() {
    for descending in [false, true] {
        let scan = scan(
            json!({"a": {"_all": {"_gt": 0}}}),
            &[("custom", descending)],
        );
        assert!(
            matches!(scan, IndexScanType::RangeScan { .. }),
            "descending {descending}: {scan:?}"
        );
    }
}

#[test]
fn no_other_filter_reads_the_marker() {
    for custom in [
        json!({"a": {"_any": {"_eq": 1}}}),
        json!({"a": {"_any": {"_lt": 2}}}),
        json!({"a": {"_eq": 1}}),
    ] {
        let scan = scan(custom.clone(), &[("custom", false)]);
        assert!(marker_branch(&scan).is_none(), "{custom}: {scan:?}");
    }
}
