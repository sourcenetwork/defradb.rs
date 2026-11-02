//! Generated from the conformance scenarios of tests/integration/index/new_get_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, ListIndexes};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.index.definition.list")]
fn index_list_get_indexes_for_a_collection_return_collection_specific_list() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                        name: String 
                        age: Int @index
                    }

                    type Address {
                        street: String
                        postalCode: String @index
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
                                ])),
                            })),
                        })),
                    ])),
                }),
                ..Default::default()
            }
            .into(),
            ListIndexes {
                collection: "Address".into(),
                expected_data: json!({
                    "indexes": m::unordered(json!([
                        m::object(json!({
                            "Description": m::object(json!({
                                "Name": "Address_postalCode_ASC",
                                "ID": 1,
                                "Kind": 0,
                                "Unique": false,
                                "Fields": m::unordered(json!([
                                    m::object(json!({ "Name": "postalCode", "Descending": false })),
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
#[mantra_macros::req_test("go.index.definition.list")]
fn index_list_should_return_list_of_existing_indexes() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(name: "age_index", includes: [{field: "age"}]) {
                        name: String @index(name: "name_index")
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
                                "Name": "name_index",
                                "ID": 1,
                                "Kind": 0,
                                "Unique": false,
                                "Fields": m::unordered(json!([
                                    m::object(json!({ "Name": "name", "Descending": false })),
                                ])),
                            })),
                        })),
                        m::object(json!({
                            "Description": m::object(json!({
                                "Name": "age_index",
                                "ID": 2,
                                "Kind": 0,
                                "Unique": false,
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
