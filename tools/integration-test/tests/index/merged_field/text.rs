//! A text index follows edits merged by the text CRDT
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
fn a_text_index_follows_local_edits() {
    TestCase::from_actions(vec![
        AddCollection {
            sdl: "type Note { body: String @crdt(type: text) @index }".into(),
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "Note".into(),
            doc_map: Some(json!({ "body": "hello world" })),
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
            doc_map: json!({ "body": "hello brave world" }),
            expected_data: json!({}),
            ..Default::default()
        }
        .into(),
        Request {
            request: r#"query @explain(type: execute) {
                Note(filter: {body: {_eq: "hello brave world"}}) { _docID }
            }"#
            .into(),
            expected_data: m::explain_scan(json!({ "indexFetches": 1, "docFetches": 1 })),
            ..Default::default()
        }
        .into(),
        Request {
            request: r#"query @explain(type: execute) {
                Note(filter: {body: {_eq: "hello world"}}) { _docID }
            }"#
            .into(),
            expected_data: m::explain_scan(json!({ "indexFetches": 0, "docFetches": 0 })),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.index.merged_field")]
fn a_text_index_follows_merged_edits() {
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
                sdl: "type Note { body: String @crdt(type: text) @index }".into(),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: "type Note { body: String @crdt(type: text) @index }".into(),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Note".into(),
                doc_map: Some(json!({ "body": "hello world" })),
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
                doc_map: Some(json!({ "body": "hello world" })),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            // Both nodes edit before they replicate, so the edits are concurrent.
            UpdateDoc {
                collection: "Note".into(),
                doc_id: Text::reference("doc"),
                doc_map: json!({ "body": "hello brave world" }),
                expected_data: json!({}),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Note".into(),
                doc_id: Text::reference("doc"),
                doc_map: json!({ "body": "hello world!" }),
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
                request: r#"query { Note(filter: {body: {_eq: "hello brave world!"}}) { _docID } }"#
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
                request: r#"query { Note(filter: {body: {_eq: "hello brave world!"}}) { _docID } }"#
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
                request: r#"query @explain(type: execute) {
                    Note(filter: {body: {_in: ["hello world", "hello brave world", "hello world!"]}}) { _docID }
                }"#
                .into(),
                expected_data: m::explain_scan(json!({ "indexFetches": 0, "docFetches": 0 })),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"query @explain(type: execute) {
                    Note(filter: {body: {_in: ["hello world", "hello brave world", "hello world!"]}}) { _docID }
                }"#
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
