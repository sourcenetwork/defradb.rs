//! Generated from the conformance scenarios of tests/integration/signature/verify_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::shared::KeyType;
use integration_test::actions::{
    AddCollection, AddDoc, DeleteDoc, NewNode, UpdateDoc, VerifyBlockSignature, WaitForPeersEvents,
    WaitForSync,
};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.signature.verify.basic")]
fn signature_verify_with_different_key_type_should_verify() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                enable_signing: true,
                key_type: Some(KeyType::Ed25519),
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
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "age": 21, "name": "John" })),
                common: Common {
                    retry: true,
                    capture: captures([("doc", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc")),
                common: Common {
                    capture: captures([("head0", "/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            VerifyBlockSignature {
                signer_node: Some("db".into()),
                cid: Text::reference("head0"),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.signature.verify.basic")]
fn signature_verify_with_valid_data_should_verify() {
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
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "age": 21, "name": "John" })),
                common: Common {
                    retry: true,
                    capture: captures([("doc", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc")),
                common: Common {
                    capture: captures([("head0", "/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            VerifyBlockSignature {
                signer_node: Some("db".into()),
                cid: Text::reference("head0"),
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc"),
                doc_map: json!({ "age": 23 }),
                common: Common {
                    retry: true,
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc")),
                common: Common {
                    capture: captures([("head1", "/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            VerifyBlockSignature {
                signer_node: Some("db".into()),
                cid: Text::reference("head1"),
                ..Default::default()
            }
            .into(),
            DeleteDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc"),
                common: Common {
                    retry: true,
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc")),
                common: Common {
                    capture: captures([("head2", "/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            VerifyBlockSignature {
                signer_node: Some("db".into()),
                cid: Text::reference("head2"),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.signature.verify.basic")]
fn signature_verify_with_wrong_cid_should_error() {
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
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "age": 21, "name": "John" })),
                common: Common {
                    retry: true,
                    capture: captures([("doc", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc")),
                common: Common {
                    capture: captures([("head0", "/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            VerifyBlockSignature {
                signer_node: Some("db".into()),
                cid: "bafyreichuvsbsr3oo4xeqfi55mrh4us77z2bg2foemuzhn5idomya6epl4".into(),
                expected_error: Some("could not find".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.signature.verify.basic")]
fn signature_verify_with_wrong_identity_should_error() {
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
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "age": 21, "name": "John" })),
                common: Common {
                    retry: true,
                    capture: captures([("doc", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc")),
                common: Common {
                    capture: captures([("head0", "/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            VerifyBlockSignature {
                signer_seed: Some(1),
                cid: Text::reference("head0"),
                expected_error: Some("signature was created by a different key".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
