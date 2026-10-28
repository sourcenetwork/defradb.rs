//! Generated from the conformance scenarios of tests/integration/encryption/field_commit_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request, UpdateDoc};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.encryption.field.storage")]
fn doc_encryption_field_upon_update_with_doc_and_field_encryption_should_use_dedicated_enc_key_for_individual_fields(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name1: String
                        name2: String
                        name3: String
                        name4: String
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name1": "John", "name2": "John", "name3": "John", "name4": "John" })),
                is_doc_encrypted: Some(true),
                encrypted_fields: Some(vec!["name1".into(), "name3".into()]),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                doc_map: json!({ "name1": "Andy", "name2": "Andy", "name3": "Andy", "name4": "Andy" }),
                expected_data: json!({}),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        _commits(order: {height: DESC}, limit: 5) {
                            cid
                            delta
                            fieldName
                            height
                        }
                    }
                "#.into(),
                expected_data: json!({
                    "_commits": m::all(json!([
                        m::first_where("fieldName", "name1", json!(m::object(json!({ "delta": m::unique("fieldCiphertext") })))),
                        m::first_where("fieldName", "name2", json!(m::object(json!({
                            "delta": m::all(json!([m::unique("fieldCiphertext"), m::same("docCiphertext")])),
                        })))),
                        m::first_where("fieldName", "name3", json!(m::object(json!({ "delta": m::unique("fieldCiphertext") })))),
                        m::first_where("fieldName", "name4", json!(m::object(json!({ "delta": m::same("docCiphertext") })))),
                    ])),
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
#[mantra_macros::req_test("go.encryption.field.storage")]
fn doc_encryption_field_with_doc_and_field_encryption_should_use_dedicated_enc_key_for_individual_fields(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name1: String
                        name2: String
                        name3: String
                        name4: String
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name1": "John", "name2": "John", "name3": "John", "name4": "John" })),
                is_doc_encrypted: Some(true),
                encrypted_fields: Some(vec!["name1".into(), "name3".into()]),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        _commits {
                            cid
                            delta
                            fieldName
                        }
                    }
                "#.into(),
                expected_data: json!({
                    "_commits": m::all(json!([
                        m::first_where("fieldName", "name1", json!(m::object(json!({ "delta": m::unique("fieldCiphertext") })))),
                        m::first_where("fieldName", "name2", json!(m::object(json!({
                            "delta": m::all(json!([m::unique("fieldCiphertext"), m::same("docCiphertext")])),
                        })))),
                        m::first_where("fieldName", "name3", json!(m::object(json!({ "delta": m::unique("fieldCiphertext") })))),
                        m::first_where("fieldName", "name4", json!(m::object(json!({ "delta": m::same("docCiphertext") })))),
                    ])),
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
#[mantra_macros::req_test("go.encryption.field.storage")]
fn doc_encryption_field_with_encryption_on_field_should_store_only_fields_delta_encrypted() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int @crdt(type: lww)
                        verified: Boolean
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "age": 21 })),
                is_doc_encrypted: Some(false),
                encrypted_fields: Some(vec!["age".into()]),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
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
                    "_commits": m::unordered(json!([
                        {
                            "delta": "ZGV0ZXJtaW5pc3RpOMcQ38zs1gB3oXyfppd9CGo=",
                            "docID": m::r("doc0_0"),
                            "fieldName": "age",
                        },
                        {
                            "delta": "ZEpvaG4=",
                            "docID": m::r("doc0_0"),
                            "fieldName": "name",
                        },
                        { "delta": null, "docID": m::r("doc0_0"), "fieldName": "_C" },
                    ])),
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
