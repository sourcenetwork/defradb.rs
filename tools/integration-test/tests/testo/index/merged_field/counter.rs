//! A counter index follows increments, local and merged
//! (`defra.index.counter`).

use integration_test::actions::{
    AddCollection, AddDoc, AddReplicator, NewNode, Request, UpdateDoc,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Eventually, Text};
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::json;

#[test]
#[mantra_macros::req_test("defra.index.counter")]
fn a_counter_index_follows_local_increments() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Note { count: Int @crdt(type: pcounter) @index }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Note".into(),
                doc_map: Some(json!({ "count": 1 })),
                common: Common {
                    capture: captures([("doc", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Note".into(),
                doc_id: Text::reference("doc"),
                doc_map: json!({ "count": 2 }),
                expected_data: json!({}),
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Note".into(),
                doc_id: Text::reference("doc"),
                doc_map: json!({ "count": 3 }),
                expected_data: json!({}),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query @explain(type: execute) { Note(filter: {count: {_eq: 6}}) { _docID } }"
                    .into(),
                expected_data: m::explain_scan(json!({ "indexFetches": 1, "docFetches": 1 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query @explain(type: execute) { Note(filter: {count: {_gt: 4}}) { _docID } }"
                    .into(),
                expected_data: m::explain_scan(json!({ "indexFetches": 1, "docFetches": 1 })),
                ..Default::default()
            }
            .into(),
            Request {
                request:
                    "query @explain(type: execute) { Note(filter: {count: {_in: [1, 3]}}) { _docID } }"
                        .into(),
                expected_data: m::explain_scan(json!({ "indexFetches": 0, "docFetches": 0 })),
                ..Default::default()
            }
            .into(),
        ],
        client_types: Some(&[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli]),
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.index.counter")]
fn a_counter_index_follows_merged_increments() {
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
                sdl: "type Note { count: Int @crdt(type: pcounter) @index }".into(),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: "type Note { count: Int @crdt(type: pcounter) @index }".into(),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Note".into(),
                doc_map: Some(json!({ "count": 1 })),
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
                doc_map: Some(json!({ "count": 1 })),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            // Both nodes increment before they replicate, so the increments are concurrent.
            UpdateDoc {
                collection: "Note".into(),
                doc_id: Text::reference("doc"),
                doc_map: json!({ "count": 2 }),
                expected_data: json!({}),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Note".into(),
                doc_id: Text::reference("doc"),
                doc_map: json!({ "count": 3 }),
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
                request: "query { Note(filter: {count: {_eq: 6}}) { _docID } }".into(),
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
                request: "query { Note(filter: {count: {_eq: 6}}) { _docID } }".into(),
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
                request: "query @explain(type: execute) { Note(filter: {count: {_in: [1, 3, 4]}}) { _docID } }"
                    .into(),
                expected_data: m::explain_scan(json!({ "indexFetches": 0, "docFetches": 0 })),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: "query @explain(type: execute) { Note(filter: {count: {_in: [1, 3, 4]}}) { _docID } }"
                    .into(),
                expected_data: m::explain_scan(json!({ "indexFetches": 0, "docFetches": 0 })),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        client_types: Some(&[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli]),
        ..Default::default()
    }
    .execute();
}
