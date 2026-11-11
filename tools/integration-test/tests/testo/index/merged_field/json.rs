//! A JSON index follows edits merged by the JSON CRDT
//! (`defra.index.merged_field`).

use integration_test::actions::{
    AddCollection, AddDoc, AddReplicator, NewNode, Request, UpdateDoc,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Eventually, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("defra.index.merged_field")]
fn a_json_index_follows_merged_edits() {
    TestCase {
        id: integration_test::scenario_id!(),
        without_db: true,
        actions: vec![
            NewNode {
                network: true,
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            NewNode {
                network: true,
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: "type Note { meta: JSON @crdt(type: json) @index }".into(),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: "type Note { meta: JSON @crdt(type: json) @index }".into(),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Note".into(),
                doc_map: Some(json!({ "meta": { "a": 1, "b": 1 } })),
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("doc", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            // The same content gives the same DocID on both nodes.
            AddDoc {
                collection: "Note".into(),
                doc_map: Some(json!({ "meta": { "a": 1, "b": 1 } })),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            // Both nodes edit before they replicate, so the edits are concurrent.
            UpdateDoc {
                collection: "Note".into(),
                doc_id: Text::reference("doc"),
                doc_map: json!({ "meta": { "a": 2, "b": 1 } }),
                expected_data: json!({}),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Note".into(),
                doc_id: Text::reference("doc"),
                doc_map: json!({ "meta": { "a": 1, "b": 2 } }),
                expected_data: json!({}),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddReplicator {
                target: "db1".into(),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddReplicator {
                target: "db0".into(),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { Note(filter: {meta: {a: {_eq: 2}, b: {_eq: 2}}}) { _docID } }"
                    .into(),
                expected_data: json!({ "Note": [{ "_docID": m::r("doc") }] }),
                common: Common {
                    node: Some("db0".into()),
                    eventually: Some(Eventually::default()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { Note(filter: {meta: {a: {_eq: 2}, b: {_eq: 2}}}) { _docID } }"
                    .into(),
                expected_data: json!({ "Note": [{ "_docID": m::r("doc") }] }),
                common: Common {
                    node: Some("db1".into()),
                    eventually: Some(Eventually::default()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: "query @explain(type: execute) { Note(filter: {meta: {a: {_eq: 1}}}) { _docID } }"
                    .into(),
                expected_data: m::explain_scan(json!({ "indexFetches": 0, "docFetches": 0 })),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: "query @explain(type: execute) { Note(filter: {meta: {b: {_eq: 1}}}) { _docID } }"
                    .into(),
                expected_data: m::explain_scan(json!({ "indexFetches": 0, "docFetches": 0 })),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: "query @explain(type: execute) { Note(filter: {meta: {a: {_eq: 1}}}) { _docID } }"
                    .into(),
                expected_data: m::explain_scan(json!({ "indexFetches": 0, "docFetches": 0 })),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: "query @explain(type: execute) { Note(filter: {meta: {b: {_eq: 1}}}) { _docID } }"
                    .into(),
                expected_data: m::explain_scan(json!({ "indexFetches": 0, "docFetches": 0 })),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
