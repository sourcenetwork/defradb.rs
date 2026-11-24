//! A full-text index follows edits merged by the text CRDT
//! (`defra.index.merged_field`).

use integration_test::actions::{
    AddCollection, AddDoc, AddReplicator, NewNode, Request, UpdateDoc,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Eventually, Text};
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::json;

const SDL: &str = "type Note { body: String @crdt(type: text) @fulltext }";

fn search(term: &str, expected: serde_json::Value, eventually: bool) -> Request {
    Request {
        request: format!(
            r#"query {{ Note(filter: {{_alias: {{score: {{_gt: 0}}}}}}) {{ _docID score: BM25(query: "{term}", fields: ["body"]) }} }}"#
        )
        .into(),
        expected_data: expected,
        common: Common {
            node: Some("db1".into()),
            eventually: eventually.then(Eventually::default),
            ..Default::default()
        },
        ..Default::default()
    }
}

#[test]
#[mantra_macros::req_test("defra.index.merged_field")]
fn a_fulltext_index_follows_merged_edits() {
    TestCase {
        id: integration_test::scenario_id!(),
        without_db: true,
        actions: vec![
            NewNode {
                network: true,
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NewNode {
                network: true,
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: SDL.into(),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: SDL.into(),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddReplicator {
                target: "db1".into(),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Note".into(),
                doc_map: Some(json!({ "body": "apple pie" })),
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("doc", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            search(
                "apple",
                json!({ "Note": [m::object(json!({ "_docID": m::r("doc") }))] }),
                true,
            )
            .into(),
            UpdateDoc {
                collection: "Note".into(),
                doc_id: Text::reference("doc"),
                doc_map: json!({ "body": "cherry pie" }),
                expected_data: json!({}),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            search(
                "cherry",
                json!({ "Note": [m::object(json!({ "_docID": m::r("doc") }))] }),
                true,
            )
            .into(),
            search("apple", json!({ "Note": [] }), false).into(),
        ],
        client_types: Some(&[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli]),
        ..Default::default()
    }
    .execute();
}
