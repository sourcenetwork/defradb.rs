//! Generated from the conformance scenarios of tests/integration/query/commits/simple_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request, UpdateDoc};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.commits.listing")]
fn query_commits() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int
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
                                            }
                                        }
                "#
                .into(),
                expected_data: json!({
                    "_commits": [
                        {
                            "cid": m::all(json!([m::same("nameCid"), m::unique("uniqueCid")])),
                        },
                        {
                            "cid": m::all(json!([m::same("ageCid"), m::unique("uniqueCid")])),
                        },
                        {
                            "cid": m::all(json!([m::same("headCid"), m::unique("uniqueCid")])),
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
#[mantra_macros::req_test("go.query.commits.listing")]
fn query_commits_multiple_docs() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int
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
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Shahzad", "age": 28 })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
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
                                            }
                                        }
                "#
                .into(),
                expected_data: json!({
                    "_commits": [
                        { "cid": m::cid() },
                        { "cid": m::cid() },
                        { "cid": m::cid() },
                        { "cid": m::cid() },
                        { "cid": m::cid() },
                        { "cid": m::cid() },
                    ],
                }),
                common: Common {
                    unordered_documents: vec!["/data/_commits".into()],
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.commits.listing")]
fn query_commits_with_all_fields_with_update_no_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int
                        verified: Boolean
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "age": 21 })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
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
                        _commits {
                            cid
                            delta
                            docID
                            fieldName
                            height
                            links {
                                fieldName
                                cid
                            }
                            heads {
                                cid
                            }
                            signature {
                                type
                            }
                        }
                    }
                "#.into(),
                expected_data: json!({
                    "_commits": [
                        {
                            "cid": m::all(json!([m::same("ageUpdateCid"), m::unique("uniqueCid")])),
                            "delta": "Fg==",
                            "docID": m::r("doc0_0"),
                            "fieldName": "age",
                            "heads": [{ "cid": m::same("ageCreateCid") }],
                            "height": 2,
                            "links": [],
                            "signature": null,
                        },
                        {
                            "cid": m::all(json!([m::same("ageCreateCid"), m::unique("uniqueCid")])),
                            "delta": "FQ==",
                            "docID": m::r("doc0_0"),
                            "fieldName": "age",
                            "heads": [],
                            "height": 1,
                            "links": [],
                            "signature": null,
                        },
                        {
                            "cid": m::all(json!([m::same("nameCreateCid"), m::unique("uniqueCid")])),
                            "delta": "ZEpvaG4=",
                            "docID": m::r("doc0_0"),
                            "fieldName": "name",
                            "heads": [],
                            "height": 1,
                            "links": [],
                            "signature": null,
                        },
                        {
                            "cid": m::all(json!([m::same("updateCompositeCid"), m::unique("uniqueCid")])),
                            "delta": null,
                            "docID": m::r("doc0_0"),
                            "fieldName": "_C",
                            "heads": [{ "cid": m::same("createCompositeCid") }],
                            "height": 2,
                            "links": [{ "cid": m::same("ageUpdateCid"), "fieldName": "age" }],
                            "signature": null,
                        },
                        {
                            "cid": m::all(json!([m::same("createCompositeCid"), m::unique("uniqueCid")])),
                            "delta": null,
                            "docID": m::r("doc0_0"),
                            "fieldName": "_C",
                            "heads": [],
                            "height": 1,
                            "links": [
                                { "cid": m::same("ageCreateCid"), "fieldName": "age" },
                                { "cid": m::same("nameCreateCid"), "fieldName": "name" },
                            ],
                            "signature": null,
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
#[mantra_macros::req_test("go.query.commits.listing")]
fn query_commits_with_alias_succeeds() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int
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
                                        history: _commits {
                                            cid
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "history": [{ "cid": m::cid() }, { "cid": m::cid() }, { "cid": m::cid() }],
                }),
                common: Common {
                    unordered_documents: vec!["/data/history".into()],
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.commits.listing")]
fn query_commits_with_collection_version_id_field() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int
                        verified: Boolean
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "age": 21 })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                            _commits {
                                                cid
                                                collectionVersionId
                                            }
                                        }
                "#.into(),
                expected_data: json!({
                    "_commits": [
                        {
                            "cid": m::cid(),
                            "collectionVersionId": "bafyreicrgjxxcviov5jawe2haq5fbtd4jxt63vsdhqpcyaaahiothj72tu",
                        },
                        {
                            "cid": m::cid(),
                            "collectionVersionId": "bafyreicrgjxxcviov5jawe2haq5fbtd4jxt63vsdhqpcyaaahiothj72tu",
                        },
                        {
                            "cid": m::cid(),
                            "collectionVersionId": "bafyreicrgjxxcviov5jawe2haq5fbtd4jxt63vsdhqpcyaaahiothj72tu",
                        },
                    ],
                }),
                common: Common { unordered_documents: vec!["/data/_commits".into()], ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.commits.listing")]
fn query_commits_with_field_name_field_and_update() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int
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
                        _commits {
                            fieldName
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "_commits": [
                        { "fieldName": "age" },
                        { "fieldName": "age" },
                        { "fieldName": "name" },
                        { "fieldName": "_C" },
                        { "fieldName": "_C" },
                    ],
                }),
                common: Common {
                    compare: Some(true),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.commits.listing")]
fn query_commits_with_field_name_field() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        age: Int
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
                            fieldName
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "_commits": [
                        { "fieldName": "age" },
                        { "fieldName": "name" },
                        { "fieldName": "_C" },
                    ],
                }),
                common: Common {
                    compare: Some(true),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
