//! A failed unique-index backfill is reported and stops maintaining the index
//! (`defra.index.backfill_failed`).

use integration_test::actions::shared::{IndexField, IndexSpec};
use integration_test::actions::{AddCollection, AddDoc, ListIndexes, NewIndex, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

fn add(name: &str, capture: &str) -> integration_test::actions::Actions {
    AddDoc {
        collection: "User".into(),
        doc_map: Some(json!({ "name": name, "age": 21 })),
        common: Common {
            capture: captures([(capture, "/docID")]),
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.index.backfill_failed")]
fn a_failed_unique_backfill_stops_maintaining_the_index() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type User { name: String  age: Int }".into(),
                ..Default::default()
            }
            .into(),
            add("Alice", "doc0").into(),
            add("Bob", "doc1").into(),
            NewIndex {
                collection: "User".into(),
                index: IndexSpec {
                    name: "by_age".into(),
                    unique: true,
                    fields: vec![IndexField {
                        name: "age".into(),
                        descending: false,
                    }],
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ListIndexes {
                collection: "User".into(),
                expected_data: json!({
                    "indexes": [m::object(json!({
                        "Description": m::object(json!({ "Name": "by_age", "Unique": true })),
                        "Execution": m::object(json!({
                            "Status": 2,
                            "Action": 3,
                            "Reason": m::substring("can not index"),
                        })),
                    }))],
                }),
                ..Default::default()
            }
            .into(),
            add("Carol", "doc2").into(),
            Request {
                request: "query { User(filter: {age: {_eq: 21}}) { name } }".into(),
                expected_data: json!({
                    "User": [{ "name": "Alice" }, { "name": "Bob" }, { "name": "Carol" }],
                }),
                common: Common {
                    unordered_documents: vec!["/data/User".into()],
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request:
                    "query @explain(type: execute) { User(filter: {age: {_eq: 21}}) { name } }"
                        .into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 0 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
