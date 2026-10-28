//! Generated from the conformance scenarios of tests/integration/encryption/query_relation_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.encryption.doc.query")]
fn doc_encryption_with_encryption_on_both_relations_should_fetch_decrypted() {
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
                                        User {
                                            name
                                            devices {
                                                model
                                                manufacturer
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "User": [
                        {
                            "devices": [{ "manufacturer": "Sony", "model": "Walkman" }],
                            "name": "Chris",
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

#[test]
#[mantra_macros::req_test("go.encryption.doc.query")]
fn doc_encryption_with_encryption_on_primary_relations_should_fetch_decrypted() {
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
                                        User {
                                            name
                                            devices {
                                                model
                                                manufacturer
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "User": [
                        {
                            "devices": [{ "manufacturer": "Sony", "model": "Walkman" }],
                            "name": "Chris",
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

#[test]
#[mantra_macros::req_test("go.encryption.doc.query")]
fn doc_encryption_with_encryption_on_secondary_relations_should_fetch_decrypted() {
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
                                        User {
                                            name
                                            devices {
                                                model
                                                manufacturer
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "User": [
                        {
                            "devices": [{ "manufacturer": "Sony", "model": "Walkman" }],
                            "name": "Chris",
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
