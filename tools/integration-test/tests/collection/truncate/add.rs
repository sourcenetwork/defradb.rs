//! Generated from the conformance scenarios of tests/integration/collection/truncate/add_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::shared::KeyType;
use integration_test::actions::{
    AddCollection, AddDoc, NewNode, Request, Truncate, WaitForPeersEvents, WaitForSync,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.collection.truncate.re_add")]
fn truncate_collection_add_adds_doc_with_blocks_at_height1() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
                ..Default::default()
            }
            .into(),
            Truncate {
                collection: "Users".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        _commits {
                            fieldName
                            height
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "_commits": [
                        { "fieldName": "name", "height": 1 },
                        { "fieldName": "_C", "height": 1 },
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
#[mantra_macros::req_test("go.collection.truncate.re_add")]
fn truncate_collection_add_adds_doc_with_same_cid_as_original() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                            _commits(filter: {fieldName: {_eq: "_C"}}) {
                                                cid
                                            }
                                        }
                "#
                .into(),
                expected_data: json!({ "_commits": [{ "cid": m::same("originalValue") }] }),
                ..Default::default()
            }
            .into(),
            Truncate {
                collection: "Users".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                            _commits(filter: {fieldName: {_eq: "_C"}}) {
                                                cid
                                            }
                                        }
                "#
                .into(),
                expected_data: json!({ "_commits": [{ "cid": m::same("originalValue") }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection.truncate.re_add")]
fn truncate_collection_add_adds_doc_with_same_doc_id_as_original() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            _docID
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "_docID": m::same("originalValue") }] }),
                ..Default::default()
            }
            .into(),
            Truncate {
                collection: "Users".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            _docID
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "_docID": m::same("originalValue") }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection.truncate.removes_data")]
fn truncate_collection_add_removes_blocks() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
                ..Default::default()
            }
            .into(),
            Truncate {
                collection: "Users".into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                            _commits (filter: {fieldName: {_eq: "_C"}}) {
                                                cid
                                            }
                                        }
                "#
                .into(),
                expected_data: json!({ "_commits": [] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection.truncate.removes_data")]
fn truncate_collection_add_removes_document() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
                ..Default::default()
            }
            .into(),
            Truncate {
                collection: "Users".into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection.truncate.removes_data")]
fn truncate_collection_add_removes_encrypted_document() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John" })),
                is_doc_encrypted: Some(true),
                encrypted_fields: Some(vec![]),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Truncate {
                collection: "Users".into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection.truncate.removes_data")]
fn truncate_collection_add_removes_shared_field_block() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "age": 30, "name": "Shared" })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "age": 40, "name": "Shared" })),
                ..Default::default()
            }
            .into(),
            Truncate {
                collection: "Users".into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        _commits {
                                            cid
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "_commits": [] }),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "age": 30, "name": "Shared" })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "age": 40, "name": "Shared" })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            name
                                            age
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "Users": m::unordered(json!([{ "age": 30, "name": "Shared" }, { "age": 40, "name": "Shared" }])),
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
#[mantra_macros::req_test("go.collection.truncate.removes_data")]
fn truncate_collection_add_removes_signed_document() {
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
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    retry: true,
                    capture: captures([("doc0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForSync {
                doc_id: Some(Text::reference("doc0")),
                common: Common {
                    capture: captures([("head0", "/cid")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Truncate {
                collection: "Users".into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
