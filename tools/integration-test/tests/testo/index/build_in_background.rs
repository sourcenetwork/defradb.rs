//! An index created without waiting for its build: the create returns while
//! the backfill is held at a shut gate, the listing reports it building, and
//! it turns ready once the gate opens. Over HTTP this is the request's
//! `BuildInBackground` field.

use integration_test::actions::shared::{IndexField, IndexGate, IndexSpec};
use integration_test::actions::{
    AddCollection, AddDoc, ListIndexes, NewIndex, NewNode, Request, WaitForIndexReady,
};
use integration_test::m;
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

fn by_name(status: i64, action: i64) -> Value {
    json!({
        "indexes": m::unordered(json!([
            m::object(json!({
                "Description": m::object(json!({ "Name": "User_name_ASC", "ID": 1 })),
                "Execution": m::object(json!({ "Status": status, "Action": action })),
            })),
        ])),
    })
}

fn add_user(name: &str, age: i64) -> AddDoc {
    AddDoc {
        collection: "User".into(),
        doc_map: Some(json!({ "name": name, "age": age })),
        common: Common {
            retry: true,
            ..Default::default()
        },
        ..Default::default()
    }
}

const EXPLAIN_FRED: &str =
    r#"query @explain(type: execute) { User(filter: {name: {_eq: "Fred"}}) { age } }"#;

#[test]
#[mantra_macros::req_test("defra.index.backfill.http-build-mode")]
fn an_index_created_in_the_background_lists_as_building_until_its_backfill_ends() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                index_gate: Some(IndexGate::Single),
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: "type User { name: String  age: Int }".into(),
                ..Default::default()
            }
            .into(),
            add_user("Fred", 33).into(),
            add_user("John", 41).into(),
            NewIndex {
                collection: "User".into(),
                index: IndexSpec {
                    name: "".into(),
                    unique: false,
                    fields: vec![IndexField {
                        name: "name".into(),
                        descending: false,
                    }],
                    ..Default::default()
                },
                r#async: true,
                ..Default::default()
            }
            .into(),
            ListIndexes {
                collection: "User".into(),
                expected_data: by_name(1, 3),
                ..Default::default()
            }
            .into(),
            Request {
                request: EXPLAIN_FRED.into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 0 }))),
                ..Default::default()
            }
            .into(),
            WaitForIndexReady {
                release_gate: true,
                ..Default::default()
            }
            .into(),
            WaitForIndexReady {
                collection: Some("User".into()),
                ..Default::default()
            }
            .into(),
            ListIndexes {
                collection: "User".into(),
                expected_data: by_name(3, 0),
                ..Default::default()
            }
            .into(),
            Request {
                request: EXPLAIN_FRED.into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 1 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
