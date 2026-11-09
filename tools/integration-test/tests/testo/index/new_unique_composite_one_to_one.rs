//! Generated from the conformance scenarios of tests/integration/index/new_unique_composite_one_to_one_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, ListIndexes, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.index.definition.one-to-one")]
fn one_to_one_unique_index_composite_index_relation_not_first_auto_index_still_added() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(unique: true, includes: [{field: "name"}, {field: "_addressID"}]) {
                        name: String
                        address: Address @primary
                    }

                    type Address {
                        street: String
                        user: User
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            ListIndexes {
                collection: "User".into(),
                expected_data: json!({
                    "indexes": m::unordered(json!([
                        m::object(json!({
                            "Description": m::object(json!({
                                "Name": "User_name_ASC",
                                "ID": 1,
                                "Kind": 0,
                                "Unique": true,
                                "Fields": m::unordered(json!([
                                    m::object(json!({ "Name": "name", "Descending": false })),
                                    m::object(json!({ "Name": "_addressID", "Descending": false })),
                                ])),
                            })),
                        })),
                        m::object(json!({
                            "Description": m::object(json!({
                                "Name": "User__addressID_ASC",
                                "ID": 2,
                                "Kind": 0,
                                "Unique": true,
                                "Fields": m::unordered(json!([
                                    m::object(json!({ "Name": "_addressID", "Descending": false })),
                                ])),
                            })),
                        })),
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
#[mantra_macros::req_test("go.index.definition.one-to-one")]
fn one_to_one_unique_index_multiple_null_relations_allowed() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        address: Address @primary
                    }

                    type Address {
                        street: String
                        user: User
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Jane" })),
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
                                        User {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": "John" }, { "name": "Jane" }] }),
                common: Common {
                    unordered_documents: vec!["/data/User".into()],
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
#[mantra_macros::req_test("go.index.definition.one-to-one")]
fn one_to_one_unique_index_on_primary_side_auto_added() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        address: Address @primary
                    }

                    type Address {
                        street: String
                        user: User
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            ListIndexes {
                collection: "User".into(),
                expected_data: json!({
                    "indexes": m::unordered(json!([
                        m::object(json!({
                            "Description": m::object(json!({
                                "Name": "User__addressID_ASC",
                                "ID": 1,
                                "Kind": 0,
                                "Unique": true,
                                "Fields": m::unordered(json!([
                                    m::object(json!({ "Name": "_addressID", "Descending": false })),
                                ])),
                            })),
                        })),
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
#[mantra_macros::req_test("go.index.definition.one-to-one")]
fn one_to_one_unique_index_one_to_many_should_not_make_new_index() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Author {
                        name: String
                        books: [Book]
                    }

                    type Book {
                        title: String
                        author: Author
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            ListIndexes {
                collection: "Author".into(),
                expected_data: json!({ "indexes": m::unordered(json!([])) }),
                ..Default::default()
            }
            .into(),
            ListIndexes {
                collection: "Book".into(),
                expected_data: json!({ "indexes": m::unordered(json!([])) }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.definition.one-to-one")]
fn one_to_one_unique_index_reference_same_related_doc_rejects_duplicate_link() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        address: Address @primary
                    }

                    type Address {
                        street: String
                        user: User
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Address".into(),
                doc_map: Some(json!({ "street": "123 Main St" })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "_addressID": m::r("doc1_0"), "name": "John" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "_addressID": m::r("doc1_0"), "name": "Jane" })),
                expected_error: Some(
                    "can not index a doc's field(s) that violates unique index".into(),
                ),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.definition.one-to-one")]
fn one_to_one_unique_index_type_level_composite_unique_index_succeeds() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(unique: true, includes: [{field: "_addressID"}, {field: "name"}]) {
                        name: String
                        address: Address @primary
                    }

                    type Address {
                        street: String
                        user: User
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            ListIndexes {
                collection: "User".into(),
                expected_data: json!({
                    "indexes": m::unordered(json!([
                        m::object(json!({
                            "Description": m::object(json!({
                                "Name": "User__addressID_ASC",
                                "ID": 1,
                                "Kind": 0,
                                "Unique": true,
                                "Fields": m::unordered(json!([
                                    m::object(json!({ "Name": "_addressID", "Descending": false })),
                                    m::object(json!({ "Name": "name", "Descending": false })),
                                ])),
                            })),
                        })),
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
#[mantra_macros::req_test("go.index.definition.one-to-one")]
fn one_to_one_unique_index_type_level_non_unique_index_returns_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![AddCollection {
            sdl: r#"
                    type User @index(unique: false, includes: [{field: "_addressID"}]) {
                        name: String
                        address: Address @primary
                    }

                    type Address {
                        street: String
                        user: User
                    }
                "#
            .into(),
            expected_error: Some("one-to-one relation must have a unique index".into()),
            ..Default::default()
        }
        .into()],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.definition.one-to-one")]
fn one_to_one_unique_index_user_defined_non_unique_index_returns_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![AddCollection {
            sdl: r#"
                    type User {
                        name: String
                        address: Address @primary @index(unique: false)
                    }

                    type Address {
                        street: String
                        user: User
                    }
                "#
            .into(),
            expected_error: Some("one-to-one relation must have a unique index".into()),
            ..Default::default()
        }
        .into()],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.definition.one-to-one")]
fn one_to_one_unique_index_user_defined_unique_index_with_name_succeeds() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        address: Address @primary @index(unique: true, name: "user_address_unique")
                    }

                    type Address {
                        street: String
                        user: User
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            ListIndexes {
                collection: "User".into(),
                expected_data: json!({
                    "indexes": m::unordered(json!([
                        m::object(json!({
                            "Description": m::object(json!({
                                "Name": "user_address_unique",
                                "ID": 1,
                                "Kind": 0,
                                "Unique": true,
                                "Fields": m::unordered(json!([
                                    m::object(json!({ "Name": "_addressID", "Descending": false })),
                                ])),
                            })),
                        })),
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
