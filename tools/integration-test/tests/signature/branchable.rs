//! Generated from the conformance scenarios of tests/integration/signature/branchable_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::shared::KeyType;
use integration_test::actions::{
    AddCollection, AddDoc, GetCollections, NewNode, Request, WaitForPeersEvents, WaitForSync,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.signature.commits.create")]
fn signature_with_branchable_collection_should_sign_collection_blocks() {
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
                    type Users @branchable {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            GetCollections {
                collection_name: Some("Users".into()),
                filtered: true,
                common: Common {
                    capture: captures([("collection", "/collections/0/CollectionID")]),
                    ..Default::default()
                },
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
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    retry: true,
                    capture: captures([("doc", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_ids: Some(vec![Text::reference("doc"), Text::reference("collection")]),
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
                                                    value
                                                }
                                            }
                                        }
                "#
                .into(),
                expected_data: json!({
                    "_commits": [
                        {
                            "fieldName": null,
                            "signature": {
                                "identity": m::nonempty_string(),
                                "type": "ES256K",
                                "value": m::nonempty_string(),
                            },
                        },
                        {
                            "fieldName": "name",
                            "signature": {
                                "identity": m::nonempty_string(),
                                "type": "ES256K",
                                "value": m::nonempty_string(),
                            },
                        },
                        {
                            "fieldName": "_C",
                            "signature": {
                                "identity": m::nonempty_string(),
                                "type": "ES256K",
                                "value": m::nonempty_string(),
                            },
                        },
                    ],
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
