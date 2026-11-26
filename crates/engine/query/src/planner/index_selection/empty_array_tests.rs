use document::NormalValue;
use schema::{
    FieldDescription, FieldKind, IndexDescription, IndexedFieldDescription, ScalarArrayKind,
};
use serde_json::json;

use crate::mapper::Filter;

use super::{filter_to_index_scan, IndexScanType};

fn index(fields: &[&str], descending: bool) -> IndexDescription {
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
                descending,
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
    scan_on(filter, &index(fields, false))
}

fn scan_descending(filter: serde_json::Value) -> IndexScanType {
    scan_on(filter, &index(&["tags"], true))
}

fn scan_on(filter: serde_json::Value, index: &IndexDescription) -> IndexScanType {
    let filter = Filter::from_conditions(filter.as_object().unwrap().clone());
    filter_to_index_scan(&filter, index, None, &tags_field(), None, 0)
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

/// The range starts at the null entry, so the null branch would read it twice.
#[test]
fn all_lt_on_an_ascending_index_reads_the_null_entry_once() {
    let scan = scan(json!({"tags": {"_all": {"_lt": 5}}}), &["tags"]);
    assert!(matches!(scan, IndexScanType::RangeScan { .. }), "{scan:?}");
}

/// A descending index keeps smaller values and null last, so `_lt` runs
/// through the null entry.
#[test]
fn all_lt_on_a_descending_index_reads_the_null_entry_once() {
    let scan = scan_descending(json!({"tags": {"_all": {"_lt": 5}}}));
    assert!(matches!(scan, IndexScanType::RangeScan { .. }), "{scan:?}");
}

/// `_gt` stops short of a descending index's null entry, so it still needs
/// the null branch.
#[test]
fn all_gt_on_a_descending_index_also_reads_the_null_entry() {
    let scan = scan_descending(json!({"tags": {"_all": {"_gt": 5}}}));
    assert!(null_branch(&scan).is_some(), "{scan:?}");
}

/// No empty array has an element for the `_any`, so the scan stays the
/// `_any`'s own.
#[test]
fn any_beside_all_reads_only_its_elements() {
    let scan = scan(
        json!({"tags": {"_any": {"_eq": 2}, "_all": {"_gt": 0}}}),
        &["tags"],
    );
    assert!(matches!(scan, IndexScanType::ExactMatch { .. }), "{scan:?}");
}
