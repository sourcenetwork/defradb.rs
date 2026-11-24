//! An import writes as a create on the same node does: signed by the node
//! that signs creates, and pushed to a replicator configured before it.

use integration_test::actions::shared::KeyType;
use integration_test::actions::{
    AddCollection, AddReplicator, ImportBackup, NewNode, Request, WaitForPeersEvents, WaitForSync,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

const NODE_KEY: &str = "02ae92ce7553993f04400c6976f8cd4540ae076bf0131eec8b35ae0ff9fc577a90";

#[test]
#[mantra_macros::req_test("defra.backup.import.signed_and_replicated")]
fn an_import_on_a_signing_node_signs_every_block() {
    let signed = |field: &str| {
        json!({
            "fieldName": field,
            "signature": { "type": "ES256K", "identity": NODE_KEY },
        })
    };
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                enable_signing: true,
                key_type: Some(KeyType::Secp256k1),
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                ..Default::default()
            }
            .into(),
            ImportBackup {
                import_content: r#"{"Users":[{"name":"John","age":21}]}"#.into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        _commits {
                            fieldName
                            signature {
                                type
                                identity
                            }
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "_commits": m::unordered(json!([signed("age"), signed("name"), signed("_C")])),
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.backup.import.signed_and_replicated")]
fn an_import_reaches_a_replicator_set_before_it() {
    let sdl = r#"
        type Users {
            name: String
            age: Int
        }
    "#;
    let on = |node: &str| Common {
        node: Some(node.into()),
        ..Default::default()
    };
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                network: true,
                common: on("db0"),
                ..Default::default()
            }
            .into(),
            NewNode {
                network: true,
                common: on("db1"),
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: sdl.into(),
                common: on("db0"),
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: sdl.into(),
                common: on("db1"),
                ..Default::default()
            }
            .into(),
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
            // Set before the import: a replicator set after it pushes the
            // existing documents itself, whatever the import did.
            AddReplicator {
                target: "db1".into(),
                common: on("db0"),
                ..Default::default()
            }
            .into(),
            ImportBackup {
                import_content: r#"{"Users":[{"name":"John","age":21}]}"#.into(),
                common: on("db0"),
                ..Default::default()
            }
            .into(),
            Request {
                request: "query { Users { _docID } }".into(),
                expected_data: json!({ "Users": [{ "_docID": m::doc_id() }] }),
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("doc", "/data/Users/0/_docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc")),
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("head", "/cid")]),
                    ..Default::default()
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
                request: "query { Users { name age } }".into(),
                expected_data: json!({ "Users": [{ "name": "John", "age": 21 }] }),
                common: on("db1"),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
