//! Generated from the conformance scenarios of tests/integration/query/simple/with_version_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request, UpdateDoc};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.simple.version.latest-commit")]
fn query_add_doc_with_nested_latest_commit() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Email: String
                        Age: Int
                        HeightM: Float
                        Verified: Boolean
                        CreatedAt: DateTime
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "John", "Age": 21 })),
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
                                        Users {
                                            Name
                                            Age
                                            _version {
                                                cid
                                                links {
                                                    cid
                                                    fieldName
                                                    height
                                                }
                                                heads {
                                                    cid
                                                    height
                                                }
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        {
                            "Age": 21,
                            "Name": "John",
                            "_version": [
                                {
                                    "cid": m::unique("docCompositeCid"),
                                    "heads": [],
                                    "links": [
                                        {
                                            "cid": m::unique("nameCreateCid"),
                                            "fieldName": "Name",
                                            "height": 1,
                                        },
                                        {
                                            "cid": m::unique("ageCreateCid"),
                                            "fieldName": "Age",
                                            "height": 1,
                                        },
                                    ],
                                },
                            ],
                        },
                    ],
                }),
                common: Common {
                    unordered_documents: vec!["/data/Users".into()],
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
#[mantra_macros::req_test("go.query.simple.version.latest-commit")]
fn query_update_doc_with_nested_latest_commit() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Email: String
                        Age: Int
                        HeightM: Float
                        Verified: Boolean
                        CreatedAt: DateTime
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "John", "Age": 21 })),
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
                doc_map: json!({ "Age": 22 }),
                expected_data: json!({}),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            Name
                                            Age
                                            _version {
                                                cid
                                                links {
                                                    cid
                                                    fieldName
                                                    height
                                                }
                                                heads {
                                                    cid
                                                    height
                                                }
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        {
                            "Age": 22,
                            "Name": "John",
                            "_version": [
                                {
                                    "cid": m::unique("docUpdateCompositeCid"),
                                    "heads": [
                                        {
                                            "cid": m::same("docCreateCompositeCid"),
                                            "height": 1,
                                        },
                                    ],
                                    "links": [
                                        {
                                            "cid": m::unique("ageUpdateCid"),
                                            "fieldName": "Age",
                                            "height": 2,
                                        },
                                    ],
                                },
                                {
                                    "cid": m::same("docCreateCompositeCid"),
                                    "heads": [],
                                    "links": [
                                        {
                                            "cid": m::unique("nameCreateCid"),
                                            "fieldName": "Name",
                                            "height": 1,
                                        },
                                        {
                                            "cid": m::unique("ageCreateCid"),
                                            "fieldName": "Age",
                                            "height": 1,
                                        },
                                    ],
                                },
                            ],
                        },
                    ],
                }),
                common: Common {
                    unordered_documents: vec!["/data/Users".into()],
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
#[mantra_macros::req_test("go.query.simple.version.latest-commit")]
fn query_with_all_commit_fields_no_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Email: String
                        Age: Int
                        HeightM: Float
                        Verified: Boolean
                        CreatedAt: DateTime
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "John", "Age": 21 })),
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
                                        Users {
                                            Name
                                            _docID
                                            _version {
                                                cid
                                                delta
                                                docID
                                                fieldName
                                                height
                                                links {
                                                    cid
                                                    fieldName
                                                }
                                                collectionVersionId
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        {
                            "Name": "John",
                            "_docID": m::same("docID"),
                            "_version": [
                                {
                                    "cid": m::unique("docCreateCompositeCid"),
                                    "collectionVersionId": m::unique("collectionVersionID"),
                                    "delta": null,
                                    "docID": m::same("docID"),
                                    "fieldName": "_C",
                                    "height": 1,
                                    "links": [
                                        {
                                            "cid": m::unique("nameCreateCid"),
                                            "fieldName": "Name",
                                        },
                                        {
                                            "cid": m::unique("ageCreateCid"),
                                            "fieldName": "Age",
                                        },
                                    ],
                                },
                            ],
                        },
                    ],
                }),
                common: Common {
                    unordered_documents: vec!["/data/Users".into()],
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
#[mantra_macros::req_test("go.query.simple.version.latest-commit")]
fn query_with_all_commit_fields_with_update_no_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Email: String
                        Age: Int
                        HeightM: Float
                        Verified: Boolean
                        CreatedAt: DateTime
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "John", "Age": 21 })),
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
                doc_map: json!({ "Age": 22 }),
                expected_data: json!({}),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            Name
                                            Age
                                            _docID
                                            _version {
                                                cid
                                                delta
                                                docID
                                                fieldName
                                                height
                                                links {
                                                    cid
                                                    fieldName
                                                }
                                                heads {
                                                    cid
                                                }
                                                collectionVersionId
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        {
                            "Age": 22,
                            "Name": "John",
                            "_docID": m::same("docID"),
                            "_version": [
                                {
                                    "cid": m::unique("docUpdateCompositeCid"),
                                    "collectionVersionId": m::same("collectionVersionID"),
                                    "delta": null,
                                    "docID": m::same("docID"),
                                    "fieldName": "_C",
                                    "heads": [{ "cid": m::same("docCreateCompositeCid") }],
                                    "height": 2,
                                    "links": [
                                        {
                                            "cid": m::unique("ageUpdateCid"),
                                            "fieldName": "Age",
                                        },
                                    ],
                                },
                                {
                                    "cid": m::same("docCreateCompositeCid"),
                                    "collectionVersionId": m::same("collectionVersionID"),
                                    "delta": null,
                                    "docID": m::same("docID"),
                                    "fieldName": "_C",
                                    "heads": [],
                                    "height": 1,
                                    "links": [
                                        {
                                            "cid": m::unique("nameCreateCid"),
                                            "fieldName": "Name",
                                        },
                                        {
                                            "cid": m::unique("ageCreateCid"),
                                            "fieldName": "Age",
                                        },
                                    ],
                                },
                            ],
                        },
                    ],
                }),
                common: Common {
                    unordered_documents: vec!["/data/Users".into()],
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
#[mantra_macros::req_test("go.query.simple.version.aliased")]
fn query_with_multiple_aliased_filtered_embedded_latest_commit() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Email: String
                        Age: Int
                        HeightM: Float
                        Verified: Boolean
                        CreatedAt: DateTime
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "John", "Age": 21 })),
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
                                        Users {
                                            Name
                                            Age
                                            _version {
                                                cid
                                                L1: links(filter: {fieldName: {_eq: "Age"}}) {
                                                    cid
                                                    fieldName
                                                    height
                                                }
                                                L2: links(filter: {fieldName: {_eq: "Name"}}) {
                                                    fieldName
                                                    height
                                                }
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        {
                            "Age": 21,
                            "Name": "John",
                            "_version": [
                                {
                                    "L1": [
                                        {
                                            "cid": m::unique("ageCreateCid"),
                                            "fieldName": "Age",
                                            "height": 1,
                                        },
                                    ],
                                    "L2": [
                                        {
                                            "fieldName": m::unique("nameCreateCid"),
                                            "height": 1,
                                        },
                                    ],
                                    "cid": m::unique("docCreateCompositeCid"),
                                },
                            ],
                        },
                    ],
                }),
                common: Common {
                    unordered_documents: vec!["/data/Users".into()],
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
#[mantra_macros::req_test("go.query.simple.version.latest-commit")]
fn query_simple_with_embedded_latest_commit_with_collection_version_id() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Email: String
                        Age: Int
                        HeightM: Float
                        Verified: Boolean
                        CreatedAt: DateTime
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "John", "Age": 21 })),
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
                                        Users {
                                            Name
                                            _version {
                                                collectionVersionId
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        {
                            "Name": "John",
                            "_version": [
                                {
                                    "collectionVersionId": m::unique("collectionVersionID"),
                                },
                            ],
                        },
                    ],
                }),
                common: Common {
                    unordered_documents: vec!["/data/Users".into()],
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
#[mantra_macros::req_test("go.query.simple.version.latest-commit")]
fn query_simple_with_embedded_latest_commit_with_doc_id() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Email: String
                        Age: Int
                        HeightM: Float
                        Verified: Boolean
                        CreatedAt: DateTime
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "John", "Age": 21 })),
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
                                        Users {
                                            Name
                                            _docID
                                            _version {
                                                docID
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        {
                            "Name": "John",
                            "_docID": m::same("docID"),
                            "_version": [{ "docID": m::same("docID") }],
                        },
                    ],
                }),
                common: Common {
                    unordered_documents: vec!["/data/Users".into()],
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
#[mantra_macros::req_test("go.query.simple.version.aliased")]
fn query_simple_with_multiple_aliased_embedded_latest_commit() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Email: String
                        Age: Int
                        HeightM: Float
                        Verified: Boolean
                        CreatedAt: DateTime
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "John", "Age": 21 })),
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
                                        Users {
                                            Name
                                            Age
                                            _version {
                                                cid
                                                L1: links {
                                                    cid
                                                    fieldName
                                                }
                                                L2: links {
                                                    fieldName
                                                }
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        {
                            "Age": 21,
                            "Name": "John",
                            "_version": [
                                {
                                    "L1": [
                                        {
                                            "cid": m::unique("nameCreateCid"),
                                            "fieldName": "Name",
                                        },
                                        {
                                            "cid": m::unique("ageCreateCid"),
                                            "fieldName": "Age",
                                        },
                                    ],
                                    "L2": [{ "fieldName": "Name" }, { "fieldName": "Age" }],
                                    "cid": m::unique("docCreateCompositeCid"),
                                },
                            ],
                        },
                    ],
                }),
                common: Common {
                    unordered_documents: vec!["/data/Users".into()],
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
#[mantra_macros::req_test("go.query.simple.version.aliased")]
fn query_simple_with_multiple_aliased_interleaved_nested_latest_commit() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Email: String
                        Age: Int
                        HeightM: Float
                        Verified: Boolean
                        CreatedAt: DateTime
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "John", "Age": 21 })),
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
                doc_map: json!({ "Age": 22 }),
                expected_data: json!({}),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            Name
                                            Age
                                            _version {
                                                cid
                                                L1: links {
                                                    cid
                                                    fieldName
                                                }
                                                H1: heads {
                                                    cid
                                                }
                                                L2: links {
                                                    fieldName
                                                }
                                                H2: heads {
                                                    cid
                                                    height
                                                }
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        {
                            "Age": 22,
                            "Name": "John",
                            "_version": [
                                {
                                    "H1": [{ "cid": m::same("docCreateCompositeCid") }],
                                    "H2": [
                                        {
                                            "cid": m::same("docCreateCompositeCid"),
                                            "height": 1,
                                        },
                                    ],
                                    "L1": [
                                        {
                                            "cid": m::unique("ageUpdateCid"),
                                            "fieldName": "Age",
                                        },
                                    ],
                                    "L2": [{ "fieldName": "Age" }],
                                    "cid": m::unique("docUpdateCompositeCid"),
                                },
                                {
                                    "H1": [],
                                    "H2": [],
                                    "L1": [
                                        {
                                            "cid": m::unique("nameCreateCid"),
                                            "fieldName": "Name",
                                        },
                                        {
                                            "cid": m::unique("ageCreateCid"),
                                            "fieldName": "Age",
                                        },
                                    ],
                                    "L2": [{ "fieldName": "Name" }, { "fieldName": "Age" }],
                                    "cid": m::same("docCreateCompositeCid"),
                                },
                            ],
                        },
                    ],
                }),
                common: Common {
                    unordered_documents: vec!["/data/Users".into()],
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
#[mantra_macros::req_test("go.query.simple.version.latest-commit")]
fn query_simple_with_nested_latest_commit() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
                        Email: String
                        Age: Int
                        HeightM: Float
                        Verified: Boolean
                        CreatedAt: DateTime
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "John", "Age": 21 })),
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
                                        Users {
                                            Name
                                            Age
                                            _version {
                                                cid
                                                links {
                                                    cid
                                                    fieldName
                                                }
                                                heads {
                                                    cid
                                                }
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        {
                            "Age": 21,
                            "Name": "John",
                            "_version": [
                                {
                                    "cid": m::unique("docCreateCompositeCid"),
                                    "heads": [],
                                    "links": [
                                        {
                                            "cid": m::unique("ageCreateCid"),
                                            "fieldName": "Age",
                                        },
                                        {
                                            "cid": m::unique("nameCreateCid"),
                                            "fieldName": "Name",
                                        },
                                    ],
                                },
                            ],
                        },
                    ],
                }),
                common: Common {
                    unordered_documents: vec!["/data/Users".into()],
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
