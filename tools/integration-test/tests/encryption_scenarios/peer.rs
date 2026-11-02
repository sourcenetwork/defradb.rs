//! Generated from the conformance scenarios of tests/integration/encryption/peer_test.go at defradb 137dc624052b.
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
#[mantra_macros::req_test("go.encryption.peer.key_fetch")]
fn doc_encryption_peer_if_peer_did_not_receive_key_should_not_fetch() {
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
                    type Users {
                        name: String
                        age: Int @crdt(type: lww)
                        verified: Boolean
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
                    type Users {
                        name: String
                        age: Int @crdt(type: lww)
                        verified: Boolean
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
                collections: vec!["Users".into()],
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "age": 21 })),
                is_doc_encrypted: Some(true),
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
                                        Users {
                                            age
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": m::any_of(json!([[], [{ "age": 21 }]])) }),
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
#[mantra_macros::req_test("go.encryption.peer.key_fetch")]
fn doc_encryption_peer_upon_sync_should_sync_encrypted_dag() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                network: true,
                kms: true,
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            NewNode {
                network: true,
                kms: true,
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int @crdt(type: lww)
                        verified: Boolean
                    }
                "#.into(),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int @crdt(type: lww)
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
                collections: vec!["Users".into()],
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "age": 21 })),
                is_doc_encrypted: Some(true),
                encrypted_fields: Some(vec![]),
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
            Request {
                request: r#"
                    query {
                        _commits {
                            cid
                            delta
                            docID
                            fieldName
                            height
                            links {
                                cid
                                fieldName
                            }
                        }
                    }
                "#.into(),
                expected_data: json!({
                    "_commits": m::unordered(json!([
                        {
                            "cid": m::all(json!([m::same("ageCid"), m::unique("uniqueCid-node1")])),
                            "delta": "ZGV0ZXJtaW5pc3RpNvlq4Y5atkDryMa8ki/0KQ8=",
                            "docID": m::r("doc0_0"),
                            "fieldName": "age",
                            "height": 1,
                            "links": m::unordered(json!([])),
                        },
                        {
                            "cid": m::all(json!([m::same("nameCid"), m::unique("uniqueCid-node1")])),
                            "delta": "ZGV0ZXJtaW5pc3RpR0Wllwd91ZElqz2KPgZMmkX/QWRp",
                            "docID": m::r("doc0_0"),
                            "fieldName": "name",
                            "height": 1,
                            "links": m::unordered(json!([])),
                        },
                        {
                            "cid": m::unique("uniqueCid-node1"),
                            "delta": null,
                            "docID": m::r("doc0_0"),
                            "fieldName": "_C",
                            "height": 1,
                            "links": m::unordered(json!([
                                { "cid": m::same("nameCid"), "fieldName": "name" },
                                { "cid": m::same("ageCid"), "fieldName": "age" },
                            ])),
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
