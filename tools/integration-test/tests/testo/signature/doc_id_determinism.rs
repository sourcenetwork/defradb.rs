//! A signed document's ID is the same on every node that holds it
//! (`defra.document.id.replicated`), and the same key and content give the
//! same ID while a different key gives another
//! (`defra.document.id.signature_determinism`).

use integration_test::actions::shared::KeyType;
use integration_test::actions::{
    Actions, AddCollection, AddCollectionSubscription, AddDoc, ConnectPeers, NewNode, Request,
    WaitForPeersEvents, WaitForSync,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::{json, Value};
use testo::action::ActionInstance;

const USERS: &str = r#"
    type Users {
        name: String
        age: Int
    }
"#;

fn on(node: &str) -> Common {
    Common {
        node: Some(node.into()),
        ..Default::default()
    }
}

fn signing_node(name: &str, seed: u64, key_type: KeyType, network: bool) -> NewNode {
    NewNode {
        enable_signing: true,
        key_type: Some(key_type),
        node_identity_seed: Some(seed),
        network,
        common: on(name),
        ..Default::default()
    }
}

fn users(name: &str) -> AddCollection {
    AddCollection {
        sdl: USERS.into(),
        common: on(name),
        ..Default::default()
    }
}

fn john(name: &str, expected: Value) -> AddDoc {
    AddDoc {
        collection: "Users".into(),
        doc_map: Some(json!({ "name": "John", "age": 27 })),
        expected_data: expected,
        common: on(name),
        ..Default::default()
    }
}

fn two_nodes_create_john(
    seeds: [u64; 2],
    key_type: KeyType,
    id: Value,
) -> Vec<ActionInstance<Actions>> {
    vec![
        signing_node("db0", seeds[0], key_type, false).into(),
        signing_node("db1", seeds[1], key_type, false).into(),
        users("db0").into(),
        users("db1").into(),
        john("db0", json!({ "docID": id.clone() })).into(),
        john("db1", json!({ "docID": id })).into(),
    ]
}

#[test]
#[mantra_macros::req_test("defra.document.id.signature_determinism")]
fn the_same_key_and_content_give_the_same_doc_id_ed25519() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: two_nodes_create_john([7, 7], KeyType::Ed25519, m::same("doc")),
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.document.id.signature_determinism")]
fn the_same_key_and_content_give_the_same_doc_id_secp256k1() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: two_nodes_create_john([7, 7], KeyType::Secp256k1, m::same("doc")),
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.document.id.signature_determinism")]
fn a_different_key_gives_a_different_doc_id() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: two_nodes_create_john([7, 8], KeyType::Ed25519, m::unique("doc")),
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.document.id.replicated")]
fn a_peer_shows_the_creators_doc_id() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            signing_node("db0", 0, KeyType::Secp256k1, true).into(),
            signing_node("db1", 1, KeyType::Ed25519, true).into(),
            users("db0").into(),
            users("db1").into(),
            WaitForPeersEvents {
                start: true,
                common: on("db0"),
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                common: on("db1"),
                ..Default::default()
            }
            .into(),
            ConnectPeers {
                target: "db0".into(),
                common: on("db1"),
                ..Default::default()
            }
            .into(),
            AddCollectionSubscription {
                collections: vec!["Users".into()],
                common: on("db0"),
                ..Default::default()
            }
            .into(),
            AddCollectionSubscription {
                collections: vec!["Users".into()],
                common: on("db1"),
                ..Default::default()
            }
            .into(),
            AddDoc {
                common: Common {
                    retry: true,
                    capture: captures([("doc", "/docID")]),
                    ..on("db0")
                },
                ..john("db0", Value::Null)
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc")),
                common: Common {
                    capture: captures([("head", "/cid")]),
                    ..on("db0")
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc")),
                cid: Some(Text::reference("head")),
                expected_data: json!({}),
                common: on("db1"),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { Users { _docID name } }".into(),
                expected_data: json!({ "Users": [{ "_docID": m::r("doc"), "name": "John" }] }),
                common: on("db1"),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
