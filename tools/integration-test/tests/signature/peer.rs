//! Generated from the conformance scenarios of tests/integration/signature/peer_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::shared::KeyType;
use integration_test::actions::{
    AddCollection, AddCollectionSubscription, AddDoc, ConnectPeers, NewNode, Request, UpdateDoc,
    WaitForPeersEvents, WaitForSync,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.signature.peer")]
fn doc_signature_with_peers_an_different_key_types_should_sync() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                key_type: Some(KeyType::Secp256k1),
                node_identity_seed: Some(0),
                network: true,
                enable_signing: true,
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            NewNode {
                key_type: Some(KeyType::Ed25519),
                node_identity_seed: Some(1),
                network: true,
                enable_signing: true,
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        age: Int
                    }
                "#.into(),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        age: Int
                    }
                "#.into(),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            ConnectPeers {
                target: "db0".into(),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollectionSubscription {
                collections: vec!["User".into()],
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollectionSubscription {
                collections: vec!["User".into()],
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 21 })),
                common: Common { node: Some("db0".into()), retry: true, capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0_0")),
                common: Common { node: Some("db0".into()), capture: captures([("head0", "/cid")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Fred", "age": 22 })),
                common: Common { node: Some("db1".into()), retry: true, capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0_1")),
                common: Common { node: Some("db1".into()), capture: captures([("head1", "/cid")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0_1")),
                cid: Some(Text::reference("head1")),
                expected_data: json!({}),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0_0")),
                cid: Some(Text::reference("head0")),
                expected_data: json!({}),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User {
                                            name
                                            age
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "User": m::unordered(json!([{ "age": 22, "name": "Fred" }, { "age": 21, "name": "John" }])),
                }),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User {
                                            name
                                            age
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "User": m::unordered(json!([{ "age": 22, "name": "Fred" }, { "age": 21, "name": "John" }])),
                }),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                            _commits(filter: {fieldName: {_eq: "_C"}}) {
                                                signature {
                                                    type
                                                    identity
                                                }
                                            }
                                        }
                "#.into(),
                expected_data: json!({
                    "_commits": m::unordered(json!([
                        {
                            "signature": {
                                "identity": "6f1581709bb7b1ef030d210db18e3b0ba1c776fba65d8cdaad05415142d189f8",
                                "type": "EdDSA",
                            },
                        },
                        {
                            "signature": {
                                "identity": "02ae92ce7553993f04400c6976f8cd4540ae076bf0131eec8b35ae0ff9fc577a90",
                                "type": "ES256K",
                            },
                        },
                    ])),
                }),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                            _commits(filter: {fieldName: {_eq: "_C"}}) {
                                                signature {
                                                    type
                                                    identity
                                                }
                                            }
                                        }
                "#.into(),
                expected_data: json!({
                    "_commits": m::unordered(json!([
                        {
                            "signature": {
                                "identity": "6f1581709bb7b1ef030d210db18e3b0ba1c776fba65d8cdaad05415142d189f8",
                                "type": "EdDSA",
                            },
                        },
                        {
                            "signature": {
                                "identity": "02ae92ce7553993f04400c6976f8cd4540ae076bf0131eec8b35ae0ff9fc577a90",
                                "type": "ES256K",
                            },
                        },
                    ])),
                }),
                common: Common { node: Some("db1".into()), ..Default::default() },
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
#[mantra_macros::req_test("go.signature.peer")]
fn doc_signature_with_peers_an_different_key_types_updating_same_doc_should_sync() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                key_type: Some(KeyType::Secp256k1),
                node_identity_seed: Some(0),
                network: true,
                enable_signing: true,
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            NewNode {
                key_type: Some(KeyType::Ed25519),
                node_identity_seed: Some(1),
                network: true,
                enable_signing: true,
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        age: Int
                        verified: Boolean
                    }
                "#.into(),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        age: Int
                        verified: Boolean
                    }
                "#.into(),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            ConnectPeers {
                target: "db0".into(),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollectionSubscription {
                collections: vec!["User".into()],
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollectionSubscription {
                collections: vec!["User".into()],
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 21 })),
                common: Common { node: Some("db0".into()), retry: true, capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0_0")),
                common: Common { node: Some("db0".into()), capture: captures([("head0", "/cid")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0_0")),
                cid: Some(Text::reference("head0")),
                expected_data: json!({}),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "User".into(),
                doc_id: Text::reference("doc0_0"),
                doc_map: json!({ "verified": true }),
                expected_data: json!({}),
                common: Common { node: Some("db1".into()), retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0_0")),
                common: Common { node: Some("db1".into()), capture: captures([("head1", "/cid")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0_0")),
                cid: Some(Text::reference("head1")),
                expected_data: json!({}),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "User".into(),
                doc_id: Text::reference("doc0_0"),
                doc_map: json!({ "age": 23 }),
                expected_data: json!({}),
                common: Common { node: Some("db0".into()), retry: true, ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0_0")),
                common: Common { node: Some("db0".into()), capture: captures([("head2", "/cid")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0_0")),
                cid: Some(Text::reference("head2")),
                expected_data: json!({}),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User {
                                            name
                                            age
                                            verified
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "User": [{ "age": 23, "name": "John", "verified": true }] }),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User {
                                            name
                                            age
                                            verified
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "User": [{ "age": 23, "name": "John", "verified": true }] }),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                            _commits(filter: {fieldName: {_eq: "_C"}}, order: {height: DESC}) {
                                                signature {
                                                    type
                                                    identity
                                                }
                                            }
                                        }
                "#.into(),
                expected_data: json!({
                    "_commits": [
                        {
                            "signature": {
                                "identity": "02ae92ce7553993f04400c6976f8cd4540ae076bf0131eec8b35ae0ff9fc577a90",
                                "type": "ES256K",
                            },
                        },
                        {
                            "signature": {
                                "identity": "6f1581709bb7b1ef030d210db18e3b0ba1c776fba65d8cdaad05415142d189f8",
                                "type": "EdDSA",
                            },
                        },
                        {
                            "signature": {
                                "identity": "02ae92ce7553993f04400c6976f8cd4540ae076bf0131eec8b35ae0ff9fc577a90",
                                "type": "ES256K",
                            },
                        },
                    ],
                }),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                            _commits(filter: {fieldName: {_eq: "_C"}}, order: {height: DESC}) {
                                                signature {
                                                    type
                                                    identity
                                                }
                                            }
                                        }
                "#.into(),
                expected_data: json!({
                    "_commits": [
                        {
                            "signature": {
                                "identity": "02ae92ce7553993f04400c6976f8cd4540ae076bf0131eec8b35ae0ff9fc577a90",
                                "type": "ES256K",
                            },
                        },
                        {
                            "signature": {
                                "identity": "6f1581709bb7b1ef030d210db18e3b0ba1c776fba65d8cdaad05415142d189f8",
                                "type": "EdDSA",
                            },
                        },
                        {
                            "signature": {
                                "identity": "02ae92ce7553993f04400c6976f8cd4540ae076bf0131eec8b35ae0ff9fc577a90",
                                "type": "ES256K",
                            },
                        },
                    ],
                }),
                common: Common { node: Some("db1".into()), ..Default::default() },
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
#[mantra_macros::req_test("go.signature.peer")]
fn doc_signature_with_peers_and_ed25519_key_type_should_sync() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                key_type: Some(KeyType::Ed25519),
                node_identity_seed: Some(0),
                network: true,
                enable_signing: true,
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NewNode {
                key_type: Some(KeyType::Ed25519),
                node_identity_seed: Some(1),
                network: true,
                enable_signing: true,
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
                        age: Int
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
                        age: Int
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
                doc_map: Some(json!({ "name": "John", "age": 21 })),
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
            Request {
                request: r#"
                    query {
                                        User {
                                            name
                                            age
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "User": [{ "age": 21, "name": "John" }] }),
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
#[mantra_macros::req_test("go.signature.peer")]
fn doc_signature_with_peers_and_secp256k1_key_type_should_sync() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                key_type: Some(KeyType::Secp256k1),
                node_identity_seed: Some(0),
                network: true,
                enable_signing: true,
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NewNode {
                key_type: Some(KeyType::Secp256k1),
                node_identity_seed: Some(1),
                network: true,
                enable_signing: true,
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
                        age: Int
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
                        age: Int
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
                doc_map: Some(json!({ "name": "John", "age": 21 })),
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
            Request {
                request: r#"
                    query {
                                        User {
                                            name
                                            age
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "User": [{ "age": 21, "name": "John" }] }),
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
