//! Generated from the conformance scenarios of tests/integration/encryption/commit_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request, UpdateDoc};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.encryption.doc.commits")]
fn doc_encryption_if_two_docs_have_same_field_value_cipher_text_should_be_different() {
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
                is_doc_encrypted: Some(true),
                encrypted_fields: Some(vec![]),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Islam", "age": 21 })),
                is_doc_encrypted: Some(true),
                encrypted_fields: Some(vec![]),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        _commits(filter: {fieldName: {_eq: "age"}}) {
                            delta
                            fieldName
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "_commits": [
                        { "fieldName": "age", "delta": m::unique("ageCiphertext") },
                        m::object(json!({ "delta": m::unique("ageCiphertext") })),
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
#[mantra_macros::req_test("go.encryption.doc.commits")]
fn doc_encryption_upon_encryption_several_docs_should_store_all_commits_delta_encrypted() {
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
                docs: Some(vec![
                    json!({ "name": "Islam", "age": 33 }),
                    json!({ "name": "John", "age": 21 }),
                ]),
                is_doc_encrypted: Some(true),
                encrypted_fields: Some(vec![]),
                common: Common {
                    capture: captures([("doc0_0", "/docIDs/0"), ("doc0_1", "/docIDs/1")]),
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
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "_commits": m::unordered(json!([
                        {
                            "delta": "ZGV0ZXJtaW5pc3RpOy7ywNoeSYWtvyxyJTjPfkEQ",
                            "docID": m::r("doc0_0"),
                        },
                        {
                            "delta": "ZGV0ZXJtaW5pc3RpRka5kwjTvDV1Nm3enhG+fWXWFgsk5w==",
                            "docID": m::r("doc0_0"),
                        },
                        { "delta": null, "docID": m::r("doc0_0") },
                        {
                            "delta": "ZGV0ZXJtaW5pc3RpZrmcPOluHnfCCfg/57RFavY=",
                            "docID": m::r("doc0_1"),
                        },
                        {
                            "delta": "ZGV0ZXJtaW5pc3RpFyeGyNytpkYin+Tx51jDx2U7AJ3z",
                            "docID": m::r("doc0_1"),
                        },
                        { "delta": null, "docID": m::r("doc0_1") },
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
#[mantra_macros::req_test("go.encryption.doc.commits")]
fn doc_encryption_upon_update_on_counter_crdt_should_encrypted_commit_delta() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        points: Int @crdt(type: pcounter)
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "points": 5 })),
                is_doc_encrypted: Some(true),
                encrypted_fields: Some(vec![]),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                doc_map: json!({ "points": 3 }),
                expected_data: json!({}),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        _commits(filter: {fieldName: {_eq: "points"}}) {
                            delta
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "_commits": [
                        { "delta": "ZGV0ZXJtaW5pc3RpIKDQD1XFaPB1jGDBMbU2+t4=" },
                        { "delta": "ZGV0ZXJtaW5pc3RpJriRsXQcFCWC48H4v0OqhDc=" },
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
#[mantra_macros::req_test("go.encryption.doc.commits")]
fn doc_encryption_upon_update_on_lwwcrdt_should_encrypt_commit_delta() {
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
                is_doc_encrypted: Some(true),
                encrypted_fields: Some(vec![]),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                doc_map: json!({ "age": 22 }),
                expected_data: json!({}),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        _commits(filter: {fieldName: {_eq: "age"}}) {
                            delta
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "_commits": m::unordered(json!([
                        { "delta": "ZGV0ZXJtaW5pc3RpNRRKPp62CCoQfxYgVVS6Fns=" },
                        { "delta": "ZGV0ZXJtaW5pc3RpNvlq4Y5atkDryMa8ki/0KQ8=" },
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
#[mantra_macros::req_test("go.encryption.doc.commits")]
fn doc_encryption_with_encryption_on_counter_crdt_should_store_commits_delta_encrypted() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        points: Int @crdt(type: pcounter)
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "points": 5 })),
                is_doc_encrypted: Some(true),
                encrypted_fields: Some(vec![]),
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
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "_commits": [
                        {
                            "delta": "ZGV0ZXJtaW5pc3RpJriRsXQcFCWC48H4v0OqhDc=",
                            "docID": m::r("doc0_0"),
                        },
                        { "delta": null, "docID": m::r("doc0_0") },
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
#[mantra_macros::req_test("go.encryption.doc.commits")]
fn doc_encryption_with_encryption_on_lwwcrdt_should_store_commits_delta_encrypted() {
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
                is_doc_encrypted: Some(true),
                encrypted_fields: Some(vec![]),
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
                "#
                .into(),
                expected_data: json!({
                    "_commits": m::unordered(json!([
                        {
                            "cid": m::all(json!([m::same("ageCid"), m::unique("commitCID")])),
                            "delta": "ZGV0ZXJtaW5pc3RpNvlq4Y5atkDryMa8ki/0KQ8=",
                            "docID": m::r("doc0_0"),
                            "fieldName": "age",
                            "height": 1,
                            "links": m::unordered(json!([])),
                        },
                        {
                            "cid": m::all(json!([m::same("nameCid"), m::unique("commitCID")])),
                            "delta": "ZGV0ZXJtaW5pc3RpR0Wllwd91ZElqz2KPgZMmkX/QWRp",
                            "docID": m::r("doc0_0"),
                            "fieldName": "name",
                            "height": 1,
                            "links": m::unordered(json!([])),
                        },
                        {
                            "cid": m::unique("commitCID"),
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
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.encryption.doc.commits")]
fn doc_encryption_with_multiple_docs_upon_update_should_encrypt_only_relevant_docs() {
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
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Islam", "age": 33 })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_0"),
                doc_map: json!({ "age": 22 }),
                expected_data: json!({}),
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Users".into(),
                doc_id: Text::reference("doc0_1"),
                doc_map: json!({ "age": 34 }),
                expected_data: json!({}),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        _commits(filter: {fieldName: {_eq: "age"}}) {
                            delta
                            docID
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "_commits": m::unordered(json!([
                        { "delta": "GCI=", "docID": m::r("doc0_1") },
                        { "delta": "GCE=", "docID": m::r("doc0_1") },
                        {
                            "delta": "ZGV0ZXJtaW5pc3RpNRRKPp62CCoQfxYgVVS6Fns=",
                            "docID": m::r("doc0_0"),
                        },
                        {
                            "delta": "ZGV0ZXJtaW5pc3RpNvlq4Y5atkDryMa8ki/0KQ8=",
                            "docID": m::r("doc0_0"),
                        },
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
