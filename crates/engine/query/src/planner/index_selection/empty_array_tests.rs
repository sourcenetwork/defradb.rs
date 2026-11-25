use document::NormalValue;
use schema::{
    FieldDescription, FieldKind, IndexDescription, IndexedFieldDescription, ScalarArrayKind,
};
use serde_json::json;

use crate::mapper::Filter;

use super::{filter_to_index_scan, IndexScanType};

fn index(fields: &[&str]) -> IndexDescription {
    IndexDescription {
        id: 1,
        name: "tags_idx".to_string(),
        unique: false,
        kind: None,
        auto_generated: false,
        fields: fields
            .iter()
            .map(|name| IndexedFieldDescription {
                name: name.to_string(),
                descending: false,
            })
            .collect(),
    }
}

fn tags_field() -> Vec<FieldDescription> {
    vec![FieldDescription::new(
        "tags",
        "tags",
        FieldKind::ScalarArray(ScalarArrayKind::NillableIntArray),
        schema::CType::LwwRegister,
    )]
}

fn scan(filter: serde_json::Value, fields: &[&str]) -> IndexScanType {
    let filter = Filter::from_conditions(filter.as_object().unwrap().clone());
    filter_to_index_scan(&filter, &index(fields), None, &tags_field(), None, 0)
        .unwrap()
        .scan_type
}

fn null_branch(scan: &IndexScanType) -> Option<&IndexScanType> {
    match scan {
        IndexScanType::OrScan { branches } => branches.get(1),
        _ => None,
    }
}

#[test]
fn all_eq_also_reads_the_null_entry() {
    let scan = scan(json!({"tags": {"_all": {"_eq": 1}}}), &["tags"]);
    match null_branch(&scan) {
        Some(IndexScanType::ExactMatch { values }) => {
            assert!(matches!(values.as_slice(), [NormalValue::Null]));
        }
        other => panic!("expected a null ExactMatch branch, got {other:?}"),
    }
}

#[test]
fn all_on_a_composite_prefix_reads_every_null_entry() {
    let scan = scan(json!({"tags": {"_all": {"_gt": 0}}}), &["tags", "name"]);
    match null_branch(&scan) {
        Some(IndexScanType::PrefixScan { prefix_values, .. }) => {
            assert!(matches!(prefix_values.as_slice(), [NormalValue::Null]));
        }
        other => panic!("expected a null PrefixScan branch, got {other:?}"),
    }
}

#[test]
fn any_reads_only_its_elements() {
    let scan = scan(json!({"tags": {"_any": {"_eq": 1}}}), &["tags"]);
    assert!(matches!(scan, IndexScanType::ExactMatch { .. }));
}

#[test]
fn all_eq_null_reads_the_null_entry_once() {
    let scan = scan(json!({"tags": {"_all": {"_eq": null}}}), &["tags"]);
    assert!(matches!(scan, IndexScanType::ExactMatch { .. }));
}

#[test]
fn all_over_the_whole_index_reads_the_null_entry_once() {
    let scan = scan(json!({"tags": {"_all": {"_ne": 1}}}), &["tags"]);
    assert!(matches!(scan, IndexScanType::PrefixScan { .. }));
}
