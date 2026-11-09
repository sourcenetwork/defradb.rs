//! Generated from the conformance scenarios of tests/integration/index/new_composite_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::shared::{IndexField, IndexSpec};
use integration_test::actions::{AddCollection, AddDoc, ListIndexes, NewIndex};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.index.definition.composite-directive")]
fn composite_index_new_using_field_directive_implicitly_adds_field() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String @index(includes: [{field: "age"}])
                        age: Int 
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
                                "Name": "User_name_ASC",
                                "ID": 1,
                                "Kind": 0,
                                "Unique": false,
                                "Fields": m::unordered(json!([
                                    m::object(json!({ "Name": "name", "Descending": false })),
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
#[mantra_macros::req_test("go.index.definition.composite-directive")]
fn composite_index_new_using_field_directive_overrides_default_direction() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String @index(direction: DESC, includes: [{field: "age", direction: ASC}])
                        age: Int 
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
                                "Unique": false,
                                "Fields": m::unordered(json!([
                                    m::object(json!({ "Name": "name", "Descending": true })),
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
#[mantra_macros::req_test("go.index.definition.composite-directive")]
fn composite_index_new_using_field_directive_sets_default_direction() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String @index(direction: DESC, includes: [{field: "age"}])
                        age: Int 
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
                                "Name": "User_name_ASC",
                                "ID": 1,
                                "Kind": 0,
                                "Unique": false,
                                "Fields": m::unordered(json!([
                                    m::object(json!({ "Name": "name", "Descending": true })),
                                    m::object(json!({ "Name": "age", "Descending": true })),
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
#[mantra_macros::req_test("go.index.definition.composite-directive")]
fn composite_index_new_using_field_directive_with_explicit_includes_respects_order() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String @index(includes: [{field: "age"}, {field: "name"}])
                        age: Int 
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
                                "Name": "User_age_ASC",
                                "ID": 1,
                                "Kind": 0,
                                "Unique": false,
                                "Fields": m::unordered(json!([
                                    m::object(json!({ "Name": "age", "Descending": false })),
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
#[mantra_macros::req_test("go.index.definition.composite-directive")]
fn composite_index_new_using_object_directive_overrides_default_direction() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(direction: DESC, includes: [{field: "name"}, {field: "age", direction: ASC}]) {
                        name: String
                        age: Int 
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
                                "Unique": false,
                                "Fields": m::unordered(json!([
                                    m::object(json!({ "Name": "name", "Descending": true })),
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
#[mantra_macros::req_test("go.index.definition.composite-directive")]
fn composite_index_new_using_object_directive_sets_default_direction() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(ordered: {direction: DESC, includes: [{field: "name"}, {field: "age"}]}) {
                        name: String
                        age: Int 
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
                                "Unique": false,
                                "Fields": m::unordered(json!([
                                    m::object(json!({ "Name": "name", "Descending": true })),
                                    m::object(json!({ "Name": "age", "Descending": true })),
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
#[mantra_macros::req_test("go.index.definition.composite-directive")]
fn composite_index_new_when_added_can_retrieve() {
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
                    name: "name_age_index".into(),
                    unique: false,
                    fields: vec![
                        IndexField {
                            name: "name".into(),
                            descending: false,
                        },
                        IndexField {
                            name: "age".into(),
                            descending: false,
                        },
                    ],
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
                                "Name": "name_age_index",
                                "ID": 1,
                                "Kind": 0,
                                "Unique": false,
                                "Fields": m::unordered(json!([
                                    m::object(json!({ "Name": "name", "Descending": false })),
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
