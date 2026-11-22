//! Encrypted index changes outside a transaction, run at the same time on
//! different fields of one collection. Six contenders fit the default retry
//! budget of six attempts, since a change loses at most once to each of the
//! others, so every change succeeds and none is lost.

use integration_test::actions::{
    Actions, AddCollection, DeleteEncryptedIndex, ListEncryptedIndexes, NewEncryptedIndex, Parallel,
};
use integration_test::m;
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

const SDL: &str = r#"
    type User {
        f0: String
        f1: String
        f2: String
        f3: String
        f4: String
        f5: String
    }
"#;

fn succeeds() -> Value {
    m::object(json!({ "error": "" }))
}

fn add(field: &str) -> Actions {
    NewEncryptedIndex {
        collection: "User".into(),
        field_name: field.into(),
        index_type: "".into(),
        expected_data: succeeds(),
        common: Common {
            envelope: true,
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

fn listed(fields: &[&str]) -> Value {
    let indexes: Vec<Value> = fields
        .iter()
        .map(|field| json!({ "FieldName": field, "Type": "equality" }))
        .collect();
    json!({ "indexes": m::unordered(Value::Array(indexes)) })
}

#[test]
#[mantra_macros::req_test("defra.txn.indexes.concurrent-encrypted-changes")]
fn concurrent_encrypted_index_adds_all_land() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: SDL.into(),
                ..Default::default()
            }
            .into(),
            Parallel {
                actions: ["f0", "f1", "f2", "f3", "f4", "f5"]
                    .into_iter()
                    .map(add)
                    .collect(),
                ..Default::default()
            }
            .into(),
            ListEncryptedIndexes {
                collection: "User".into(),
                expected_data: listed(&["f0", "f1", "f2", "f3", "f4", "f5"]),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.txn.indexes.concurrent-encrypted-changes")]
fn a_concurrent_encrypted_index_delete_and_adds_all_land() {
    let mut contenders: Vec<Actions> = vec![DeleteEncryptedIndex {
        collection: "User".into(),
        field_name: "f0".into(),
        expected_data: succeeds(),
        common: Common {
            envelope: true,
            ..Default::default()
        },
        ..Default::default()
    }
    .into()];
    contenders.extend(["f1", "f2", "f3", "f4", "f5"].into_iter().map(add));
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: SDL.into(),
                ..Default::default()
            }
            .into(),
            NewEncryptedIndex {
                collection: "User".into(),
                field_name: "f0".into(),
                index_type: "".into(),
                ..Default::default()
            }
            .into(),
            Parallel {
                actions: contenders,
                ..Default::default()
            }
            .into(),
            ListEncryptedIndexes {
                collection: "User".into(),
                expected_data: listed(&["f1", "f2", "f3", "f4", "f5"]),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
