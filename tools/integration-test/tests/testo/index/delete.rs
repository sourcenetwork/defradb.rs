//! Generated from the conformance scenarios of tests/integration/index/delete_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::shared::Filter;
use integration_test::actions::{
    AddCollection, AddDoc, DeleteIndex, DeleteWithFilter, ListIndexes, Request,
};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.index.maintenance.delete-with-filter")]
fn doc_delete_with_filter_with_multiple_matching_docs_cleans_up_all_index_entries() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        city:  String @index
                        email: String @index(unique: true)
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "city": "London", "email": "a@example.com" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "city": "London", "email": "b@example.com" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "city": "London", "email": "c@example.com" })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "city": "London", "email": "d@example.com" })),
                common: Common {
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            DeleteWithFilter {
                collection: "User".into(),
                filter: Filter {
                    kind: "string".into(),
                    value: Some(json!(r#"{city: {_eq: "London"}}"#)),
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "city": "Paris", "email": "a@example.com" })),
                common: Common {
                    capture: captures([("doc0_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "city": "Paris", "email": "b@example.com" })),
                common: Common {
                    capture: captures([("doc0_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "city": "Paris", "email": "c@example.com" })),
                common: Common {
                    capture: captures([("doc0_6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "city": "Paris", "email": "d@example.com" })),
                common: Common {
                    capture: captures([("doc0_7", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User {
                                            city
                                            email
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "User": [
                        { "city": "Paris", "email": "a@example.com" },
                        { "city": "Paris", "email": "b@example.com" },
                        { "city": "Paris", "email": "c@example.com" },
                        { "city": "Paris", "email": "d@example.com" },
                    ],
                }),
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
#[mantra_macros::req_test("go.index.maintenance.delete-with-filter")]
fn doc_delete_with_filter_with_unique_index_cleans_up_index_entries() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String @index(unique: true)
                        age: Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 30 })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            DeleteWithFilter {
                collection: "User".into(),
                filter: Filter {
                    kind: "string".into(),
                    value: Some(json!(r#"{name: {_eq: "John"}}"#)),
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 31 })),
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
                                            age
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "User": [{ "age": 31, "name": "John" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.definition.delete")]
fn index_delete_if_index_does_not_exist_return_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                        age: Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 21 })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            DeleteIndex {
                collection: "User".into(),
                index_name: "non_existing_index".into(),
                expected_error: Some(
                    "index with name doesn't exists. Name: non_existing_index".into(),
                ),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query  {
                        User {
                            name
                            age
                        }
                    }
                "#
                .into(),
                expected_data: json!({ "User": [{ "age": 21, "name": "John" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.definition.delete")]
fn index_delete_should_not_hinder_querying() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String @index
                        age: Int 
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 21 })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            DeleteIndex {
                collection: "User".into(),
                index_name: "User_name_ASC".into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query  {
                        User {
                            name
                            age
                        }
                    }
                "#
                .into(),
                expected_data: json!({ "User": [{ "age": 21, "name": "John" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.definition.delete")]
fn index_delete_should_remove_index_from_collection() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String @index
                        age: Int @index
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 21 })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            DeleteIndex {
                collection: "User".into(),
                index_name: "User_age_ASC".into(),
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
                                "Unique": false,
                                "Fields": m::unordered(json!([
                                    m::object(json!({ "Name": "name", "Descending": false })),
                                ])),
                            })),
                        })),
                    ])),
                }),
                ..Default::default()
            }
            .into(),
            DeleteIndex {
                collection: "User".into(),
                index_name: "User_name_ASC".into(),
                ..Default::default()
            }
            .into(),
            ListIndexes {
                collection: "User".into(),
                expected_data: json!({ "indexes": m::unordered(json!([])) }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
