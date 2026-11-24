//! Encrypted index changes outside a transaction, run at the same time on
//! different fields of one collection. Six contenders fit the default retry
//! budget of six attempts, since a change loses at most once to each of the
//! others, so every change succeeds and none is lost. Past the budget, a
//! change that loses fails with the exhaustion error.

use integration_test::actions::{
    Actions, AddCollection, DeleteEncryptedIndex, ListEncryptedIndexes, NewEncryptedIndex, NewNode,
    Parallel,
};
use integration_test::m;
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use integration_test::ClientType;
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

/// Adds racing past the retry budget: the node re-runs none, so every add
/// that loses the race fails with the exhaustion error. At least one loses,
/// since they overlap. The CLI client starts one process per add, which
/// spreads them out: with 12 adds, 2 runs in about 400 had none overlap.
#[test]
#[mantra_macros::req_test("defra.txn.indexes.concurrent-encrypted-changes")]
fn concurrent_encrypted_index_adds_out_of_retries_fail() {
    const CONTENDERS: u64 = 24;
    let fields: Vec<String> = (0..CONTENDERS).map(|field| format!("f{field}")).collect();
    let sdl = format!(
        "type User {{ {} }}",
        fields
            .iter()
            .map(|field| format!("{field}: String"))
            .collect::<Vec<_>>()
            .join(" ")
    );
    let add_or_exhaust = |field: &String| -> Actions {
        NewEncryptedIndex {
            collection: "User".into(),
            field_name: field.clone(),
            index_type: "".into(),
            expected_data: m::object(json!({
                "error": m::any_of(json!([
                    "",
                    m::substring("storage error: transaction retry attempts exhausted: "),
                ])),
            })),
            common: Common {
                envelope: true,
                ..Default::default()
            },
            ..Default::default()
        }
        .into()
    };
    let fewer_than_all: Vec<Value> = (1..CONTENDERS).map(m::length).collect();
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                max_txn_retries: Some(0),
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: sdl.into(),
                ..Default::default()
            }
            .into(),
            Parallel {
                actions: fields.iter().map(add_or_exhaust).collect(),
                ..Default::default()
            }
            .into(),
            ListEncryptedIndexes {
                collection: "User".into(),
                expected_data: json!({ "indexes": m::any_of(Value::Array(fewer_than_all)) }),
                ..Default::default()
            }
            .into(),
        ],
        client_types: Some(&[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli]),
        ..Default::default()
    }
    .execute();
}
