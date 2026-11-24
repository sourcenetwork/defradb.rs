//! An index created while its collection is updated finishes its backfill
//! (`defra.index.backfill_under_writes`).

use integration_test::actions::shared::{IndexField, IndexGate, IndexSpec};
use integration_test::actions::{
    AddCollection, AddDoc, NewIndex, NewNode, Request, UpdateDoc, WaitForIndexReady,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("defra.index.backfill_under_writes")]
fn an_index_created_under_updates_is_built_and_used() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                index_gate: Some(IndexGate::Single),
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: "type U { n: Int @crdt(type: pcounter)  g: Int }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "U".into(),
                doc_map: Some(json!({ "n": 1, "g": 10 })),
                common: Common {
                    capture: captures([("updated", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "U".into(),
                doc_map: Some(json!({ "n": 1, "g": 20 })),
                ..Default::default()
            }
            .into(),
            NewIndex {
                collection: "U".into(),
                index: IndexSpec {
                    name: "by_g".into(),
                    unique: false,
                    fields: vec![IndexField {
                        name: "g".into(),
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
            UpdateDoc {
                collection: "U".into(),
                doc_id: Text::reference("updated"),
                doc_map: json!({ "n": 1, "g": 30 }),
                expected_data: json!({}),
                ..Default::default()
            }
            .into(),
            WaitForIndexReady {
                release_gate: true,
                ..Default::default()
            }
            .into(),
            WaitForIndexReady {
                collection: Some("U".into()),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { U(filter: {g: {_eq: 30}}) { n } }".into(),
                expected_data: json!({ "U": [{ "n": 2 }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query @explain(type: execute) { U(filter: {g: {_eq: 30}}) { n } }".into(),
                expected_data: m::explain_scan(json!({ "indexFetches": 1 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query @explain(type: execute) { U(filter: {g: {_eq: 20}}) { n } }".into(),
                expected_data: m::explain_scan(json!({ "indexFetches": 1 })),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
