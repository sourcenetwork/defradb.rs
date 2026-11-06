//! Generated from the conformance scenarios of tests/integration/encryption/peer_sec_index_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddCollection, AddCollectionSubscription, AddDoc, ConnectPeers, NewNode, Request,
    WaitForPeersEvents, WaitForSync,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.encryption.peer.index")]
fn doc_encryption_peer_if_doc_doc_has_encrypted_indexed_field_should_index_after_decryption() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                network: true,
                kms: true,
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NewNode {
                network: true,
                kms: true,
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        age: Int @index
                    }
                "#
                .into(),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        age: Int @index
                    }
                "#
                .into(),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ConnectPeers {
                target: "db0".into(),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollectionSubscription {
                collections: vec!["User".into()],
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Shahzad", "age": 25 })),
                is_doc_encrypted: Some(false),
                encrypted_fields: Some(vec![]),
                common: Common {
                    node: Some("db0".into()),
                    retry: true,
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0_0")),
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("head0", "/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Islam", "age": 33 })),
                is_doc_encrypted: Some(false),
                encrypted_fields: Some(vec!["age".into()]),
                common: Common {
                    node: Some("db0".into()),
                    retry: true,
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0_1")),
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("head1", "/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Andy", "age": 21 })),
                is_doc_encrypted: Some(false),
                encrypted_fields: Some(vec![]),
                common: Common {
                    node: Some("db0".into()),
                    retry: true,
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0_2")),
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("head2", "/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 21 })),
                is_doc_encrypted: Some(false),
                encrypted_fields: Some(vec!["age".into()]),
                common: Common {
                    node: Some("db0".into()),
                    retry: true,
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0_3")),
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("head3", "/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0_0")),
                cid: Some(Text::reference("head0")),
                expected_data: json!({}),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0_1")),
                cid: Some(Text::reference("head1")),
                expected_data: json!({}),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0_2")),
                cid: Some(Text::reference("head2")),
                expected_data: json!({}),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0_3")),
                cid: Some(Text::reference("head3")),
                expected_data: json!({}),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute) {
                        User(filter: {age: {_eq: 21}}) {
                            age
                        }
                    }
                "#
                .into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 2 }))),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute) {
                        User(filter: {age: {_eq: 21}}) {
                            age
                        }
                    }
                "#
                .into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 2 }))),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        without_db: true,
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.encryption.peer.index")]
fn doc_encryption_peer_if_encrypted_doc_has_indexed_field_should_index_after_decryption() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                network: true,
                kms: true,
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NewNode {
                network: true,
                kms: true,
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        age: Int @index
                    }
                "#
                .into(),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        age: Int @index
                    }
                "#
                .into(),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ConnectPeers {
                target: "db0".into(),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollectionSubscription {
                collections: vec!["User".into()],
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Shahzad", "age": 25 })),
                is_doc_encrypted: Some(false),
                encrypted_fields: Some(vec![]),
                common: Common {
                    node: Some("db0".into()),
                    retry: true,
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0_0")),
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("head0", "/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Islam", "age": 33 })),
                is_doc_encrypted: Some(true),
                encrypted_fields: Some(vec![]),
                common: Common {
                    node: Some("db0".into()),
                    retry: true,
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0_1")),
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("head1", "/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Andy", "age": 21 })),
                is_doc_encrypted: Some(false),
                encrypted_fields: Some(vec![]),
                common: Common {
                    node: Some("db0".into()),
                    retry: true,
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0_2")),
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("head2", "/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 21 })),
                is_doc_encrypted: Some(true),
                encrypted_fields: Some(vec![]),
                common: Common {
                    node: Some("db0".into()),
                    retry: true,
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0_3")),
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("head3", "/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0_0")),
                cid: Some(Text::reference("head0")),
                expected_data: json!({}),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0_1")),
                cid: Some(Text::reference("head1")),
                expected_data: json!({}),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0_2")),
                cid: Some(Text::reference("head2")),
                expected_data: json!({}),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0_3")),
                cid: Some(Text::reference("head3")),
                expected_data: json!({}),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute) {
                        User(filter: {age: {_eq: 21}}) {
                            age
                        }
                    }
                "#
                .into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 2 }))),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute) {
                        User(filter: {age: {_eq: 21}}) {
                            age
                        }
                    }
                "#
                .into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 2 }))),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        User(filter: {age: {_eq: 21}}) {
                            name
                        }
                    }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": "Andy" }, { "name": "John" }] }),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        User(filter: {age: {_eq: 21}}) {
                            name
                        }
                    }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": "Andy" }, { "name": "John" }] }),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        without_db: true,
        ..Default::default()
    }
    .execute();
}
