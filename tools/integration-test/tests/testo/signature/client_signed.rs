//! A client-signed create commits the ID the client computed before sending
//! anything (`defra.document.id.client_signed`). In-process and rust-http
//! only: go-http and cli have no client-signed create.

use integration_test::actions::{AddCollection, AddDocClientSigned, NewNode, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

const USERS: &str = r#"
    type Users {
        name: String
        age: Int
    }
"#;

fn client_signed_john(expected: serde_json::Value, common: Common) -> AddDocClientSigned {
    AddDocClientSigned {
        collection: "Users".into(),
        doc_map: json!({ "name": "John", "age": 27 }),
        signer_seed: 3,
        expected_data: expected,
        common,
        ..Default::default()
    }
}

#[test]
#[mantra_macros::req_test("defra.document.id.client_signed")]
fn a_client_signed_create_commits_the_precomputed_id() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode::default().into(),
            AddCollection {
                sdl: USERS.into(),
                ..Default::default()
            }
            .into(),
            client_signed_john(
                json!({ "docID": m::same("doc"), "precomputedDocID": m::same("doc") }),
                Common::default(),
            )
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.document.id.client_signed")]
fn a_client_signed_create_is_queryable_under_the_precomputed_id() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode::default().into(),
            AddCollection {
                sdl: USERS.into(),
                ..Default::default()
            }
            .into(),
            client_signed_john(
                serde_json::Value::Null,
                Common {
                    capture: captures([("doc", "/precomputedDocID")]),
                    ..Default::default()
                },
            )
            .into(),
            Request {
                request: "query { Users { _docID name } }".into(),
                expected_data: json!({ "Users": [{ "_docID": m::r("doc"), "name": "John" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
