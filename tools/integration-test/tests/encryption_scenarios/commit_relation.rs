//! Generated from the conformance scenarios of tests/integration/encryption/commit_relation_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.encryption.doc.commits")]
fn doc_encryption_with_encryption_secondary_relations_should_store_encrypted_commit() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        devices: [Device]
                    }

                    type Device {
                        model: String
                        manufacturer: String
                        owner: User
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Chris" })),
                is_doc_encrypted: Some(true),
                encrypted_fields: Some(vec![]),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Device".into(),
                doc_map: Some(
                    json!({ "manufacturer": "Sony", "model": "Walkman", "owner": m::r("doc0_0") }),
                ),
                is_doc_encrypted: Some(true),
                encrypted_fields: Some(vec![]),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        _commits {
                            delta
                            docID
                            fieldName
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "_commits": [
                        {
                            "delta": "ZGV0ZXJtaW5pc3RpRkyijQDNoZem1MW626WQhBgvZWETvg==",
                            "docID": m::r("doc0_0"),
                            "fieldName": "name",
                        },
                        { "delta": null, "docID": m::r("doc0_0"), "fieldName": "_C" },
                        {
                            "delta": m::not_plain_cbor_string(json!(m::r("doc0_0"))),
                            "docID": m::r("doc1_0"),
                            "fieldName": "_ownerID",
                        },
                        {
                            "delta": m::not_plain_cbor_string(json!("Sony")),
                            "docID": m::r("doc1_0"),
                            "fieldName": "manufacturer",
                        },
                        {
                            "delta": m::not_plain_cbor_string(json!("Walkman")),
                            "docID": m::r("doc1_0"),
                            "fieldName": "model",
                        },
                        { "delta": null, "docID": m::r("doc1_0"), "fieldName": "_C" },
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
