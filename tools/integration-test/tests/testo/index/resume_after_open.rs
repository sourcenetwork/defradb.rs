//! A restart with an index build held at a shut gate: the reopen returns
//! while the resumed build still waits, and the build finishes once the gate
//! opens, as Go's resume does (#2760).

use integration_test::actions::shared::{IndexField, IndexGate, IndexSpec};
use integration_test::actions::{
    AddCollection, AddDoc, ListIndexes, NewIndex, NewNode, Request, Restart, WaitForIndexReady,
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

#[test]
#[mantra_macros::req_test("go.index.backfill.async")]
fn restart_returns_while_the_resumed_build_is_gated() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                index_gate: Some(IndexGate::Single),
                database_dir: Some("".into()),
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: "type User { name: String  age: Int }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Fred", "age": 33 })),
                common: Common {
                    retry: true,
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 41 })),
                common: Common {
                    retry: true,
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
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
            WaitForIndexReady {
                blocked: true,
                ..Default::default()
            }
            .into(),
            Restart {
                ..Default::default()
            }
            .into(),
            ListIndexes {
                collection: "User".into(),
                expected_data: by_name(1, 3),
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
                request: r#"query @explain(type: execute) { User(filter: {name: {_eq: "Fred"}}) { age } }"#.into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 1 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
