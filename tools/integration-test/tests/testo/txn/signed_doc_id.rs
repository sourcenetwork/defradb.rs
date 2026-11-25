//! A node-signed create commits the `_docID` it returned
//! (`defra.document.id.node_signed`).

use integration_test::actions::shared::KeyType;
use integration_test::actions::{AddCollection, AddDoc, NewNode, Request, TxnBegin, TxnCommit};
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

fn created_in_a_txn_then_committed(key_type: KeyType) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                enable_signing: true,
                key_type: Some(key_type),
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: USERS.into(),
                ..Default::default()
            }
            .into(),
            TxnBegin::default().into(),
            Request {
                txn: Some(0),
                request: r#"mutation { add_Users(input: { name: "John", age: 27 }) { _docID } }"#
                    .into(),
                expected_data: json!({ "add_Users": [{ "_docID": m::same("doc") }] }),
                ..Default::default()
            }
            .into(),
            TxnCommit {
                txn: 0,
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { Users { _docID name } }".into(),
                expected_data: json!({ "Users": [{ "_docID": m::same("doc"), "name": "John" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.document.id.node_signed")]
fn a_node_signed_create_in_a_txn_commits_the_id_it_returned_ed25519() {
    created_in_a_txn_then_committed(KeyType::Ed25519);
}

#[test]
#[mantra_macros::req_test("defra.document.id.node_signed")]
fn a_node_signed_create_in_a_txn_commits_the_id_it_returned_secp256k1() {
    created_in_a_txn_then_committed(KeyType::Secp256k1);
}

#[test]
#[mantra_macros::req_test("defra.document.id.node_signed")]
fn an_autocommit_node_signed_create_stores_the_id_it_returned() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                enable_signing: true,
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: USERS.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "age": 27 })),
                common: Common {
                    capture: captures([("doc", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
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
