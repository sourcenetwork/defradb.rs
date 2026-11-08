//! `_commits(cid:)` on the one block that holds every `@immutable` field of a
//! document lists each of those fields, and each field's commit is that
//! block, carrying the field's own delta.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

fn by_immutable_cid(filter: &str) -> Text {
    Text::concat([
        Text::from(r#"query { _commits(cid: ""#),
        Text::reference("immutable"),
        Text::from(format!(r#""{filter}) {{ fieldName }} }}"#)),
    ])
}

#[test]
#[mantra_macros::req_test("defra.commits.immutable_block")]
fn commits_by_cid_list_every_immutable_field() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: "type Record { author: String @immutable  kind: String @immutable  body: String }"
                    .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Record".into(),
                doc_map: Some(json!({ "author": "alice", "kind": "note", "body": "hi" })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"query { _commits(filter: {fieldName: {_in: ["author", "kind"]}}) { cid fieldName delta } }"#
                    .into(),
                expected_data: json!({ "_commits": [
                    { "cid": m::same("block"), "fieldName": "author", "delta": "ZWFsaWNl" },
                    { "cid": m::same("block"), "fieldName": "kind", "delta": "ZG5vdGU=" },
                ] }),
                common: Common {
                    unordered_documents: vec!["/data/_commits".into()],
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"query { _commits(filter: {fieldName: {_eq: "author"}}) { cid } }"#.into(),
                common: Common {
                    capture: captures([("immutable", "/data/_commits/0/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: by_immutable_cid(""),
                expected_data: json!({
                    "_commits": [{ "fieldName": "author" }, { "fieldName": "kind" }],
                }),
                common: Common {
                    unordered_documents: vec!["/data/_commits".into()],
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: by_immutable_cid(r#", filter: {fieldName: {_eq: "kind"}}"#),
                expected_data: json!({ "_commits": [{ "fieldName": "kind" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
