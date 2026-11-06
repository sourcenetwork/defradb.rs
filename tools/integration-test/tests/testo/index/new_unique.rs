//! Generated from the conformance scenarios of tests/integration/index/new_unique_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::shared::{IndexField, IndexSpec};
use integration_test::actions::{AddCollection, AddDoc, ListIndexes, NewIndex, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.index.unique.backfill")]
fn add_unique_index_if_field_values_are_not_unique_return_error() {
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
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Andy", "age": 22 })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Shahzad", "age": 21 })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NewIndex {
                collection: "User".into(),
                index: IndexSpec {
                    name: "".into(),
                    unique: true,
                    fields: vec![IndexField {
                        name: "age".into(),
                        descending: false,
                    }],
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ListIndexes {
                collection: "User".into(),
                expected_data: json!({
                    "indexes": m::unordered(json!([
                        m::object(json!({
                            "Description": m::object(json!({
                                "Name": "User_age_ASC",
                                "ID": 1,
                                "Kind": 0,
                                "Unique": true,
                                "Fields": m::unordered(json!([
                                    m::object(json!({ "Name": "age", "Descending": false })),
                                ])),
                            })),
                            "Execution": m::object(json!({
                                "Status": 2,
                                "Action": 3,
                                "Reason": m::substring("can not index"),
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
#[mantra_macros::req_test("go.index.definition.request-config")]
fn index_new_both_ordered_and_vector_config_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                                        vector: [Float32!]
                                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            NewIndex {
                collection: "User".into(),
                index: IndexSpec {
                    name: "".into(),
                    unique: false,
                    fields: vec![IndexField {
                        name: "vector".into(),
                        descending: false,
                    }],
                    ordered: Some(json!({})),
                    vector: Some(json!({ "Dimensions": 3, "HNSW": {}, "Metric": "COSINE" })),
                },
                expected_error: Some("index request has more than one kind config".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.unique.nil")]
fn unique_index_new_adding_doc_with_nil_value_should_succeed() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String 
                        age: Int @index(unique: true)
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
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.definition.request-config")]
fn unique_index_new_both_spellings_agree_should_succeed() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                                        name: String
                                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            NewIndex {
                collection: "User".into(),
                index: IndexSpec {
                    name: "".into(),
                    unique: true,
                    fields: vec![IndexField {
                        name: "name".into(),
                        descending: false,
                    }],
                    ordered: Some(json!({ "Unique": true })),
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
#[mantra_macros::req_test("go.index.unique.backfill")]
fn unique_index_new_if_field_values_are_unique_succeed() {
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
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Shahzad", "age": 22 })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NewIndex {
                collection: "User".into(),
                index: IndexSpec {
                    name: "age_unique_index".into(),
                    unique: true,
                    fields: vec![IndexField {
                        name: "age".into(),
                        descending: false,
                    }],
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ListIndexes {
                collection: "User".into(),
                expected_data: json!({
                    "indexes": m::unordered(json!([
                        m::object(json!({
                            "Description": m::object(json!({
                                "Name": "age_unique_index",
                                "ID": 1,
                                "Kind": 0,
                                "Unique": true,
                                "Fields": m::unordered(json!([
                                    m::object(json!({ "Name": "age", "Descending": false })),
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
#[mantra_macros::req_test("go.index.definition.request-config")]
fn unique_index_new_set_through_ordered_config_should_succeed() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                                        name: String
                                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            NewIndex {
                collection: "User".into(),
                index: IndexSpec {
                    name: "".into(),
                    unique: false,
                    fields: vec![IndexField {
                        name: "name".into(),
                        descending: false,
                    }],
                    ordered: Some(json!({ "Unique": true })),
                    ..Default::default()
                },
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
#[mantra_macros::req_test("go.index.definition.request-config")]
fn unique_index_new_spellings_disagree_should_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                                        name: String
                                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            NewIndex {
                collection: "User".into(),
                index: IndexSpec {
                    name: "".into(),
                    unique: true,
                    fields: vec![IndexField {
                        name: "name".into(),
                        descending: false,
                    }],
                    ordered: Some(json!({ "Unique": false })),
                    ..Default::default()
                },
                expected_error: Some("index request sets both the deprecated unique field".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.unique.write-conflict")]
fn unique_index_new_upon_adding_doc_with_existing_field_value_return_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String 
                        age: Int @index(unique: true, name: "age_unique_index")
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Shahzad", "age": 21 })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "age": 21 })),
                expected_error: Some(
                    "can not index a doc's field(s) that violates unique index.".into(),
                ),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User(filter: {name: {_eq: "John"}}) {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "User": [] }),
                ..Default::default()
            }
            .into(),
            ListIndexes {
                collection: "User".into(),
                expected_data: json!({
                    "indexes": m::unordered(json!([
                        m::object(json!({
                            "Description": m::object(json!({
                                "Name": "age_unique_index",
                                "ID": 1,
                                "Kind": 0,
                                "Unique": true,
                                "Fields": m::unordered(json!([
                                    m::object(json!({ "Name": "age", "Descending": false })),
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
#[mantra_macros::req_test("go.index.unique.nil")]
fn unique_index_new_upon_adding_doc_with_existing_nil_value_should_succeed() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String 
                        age: Int @index(unique: true)
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
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Keenan" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Andy" })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
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
#[mantra_macros::req_test("go.index.unique.nil")]
fn unique_index_new_with_multiple_nil_fields_should_succeed() {
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
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Andy" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Keenan" })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NewIndex {
                collection: "User".into(),
                index: IndexSpec {
                    name: "age_unique_index".into(),
                    unique: true,
                    fields: vec![IndexField {
                        name: "age".into(),
                        descending: false,
                    }],
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ListIndexes {
                collection: "User".into(),
                expected_data: json!({
                    "indexes": m::unordered(json!([
                        m::object(json!({
                            "Description": m::object(json!({
                                "Name": "age_unique_index",
                                "ID": 1,
                                "Kind": 0,
                                "Unique": true,
                                "Fields": m::unordered(json!([
                                    m::object(json!({ "Name": "age", "Descending": false })),
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
#[mantra_macros::req_test("go.index.unique.write-conflict")]
fn unique_query_with_index_upon_adding_doc_with_same_date_time_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String 
                        birthday: DateTime @index(unique: true)
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Fred", "birthday": "2000-07-23T03:00:00-00:00" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Andy", "birthday": "2000-07-23T03:00:00-00:00" })),
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
