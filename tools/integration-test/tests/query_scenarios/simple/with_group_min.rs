//! Generated from the conformance scenarios of tests/integration/query/simple/with_group_min_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.simple.group.aggregate.of-same")]
fn query_simple_with_group_by_string_with_inner_group_boolean_and_min_of_min_of_float_succeeds() {
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
                doc_map: Some(json!({ "Name": "John", "HeightM": 1.82, "Verified": true })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "John", "HeightM": 1.61, "Verified": true })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "John", "HeightM": 2.22, "Verified": false })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Carlo", "HeightM": 1.74, "Verified": true })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Alice", "HeightM": 2.04, "Verified": false })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(groupBy: [Name]) {
                                            Name
                                            MIN(GROUP: {field: MIN})
                                            GROUP (groupBy: [Verified]){
                                                Verified
                                                MIN(GROUP: {field: HeightM})
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        {
                            "GROUP": [{ "MIN": 1.74, "Verified": true }],
                            "MIN": 1.74,
                            "Name": "Carlo",
                        },
                        {
                            "GROUP": [
                                { "MIN": 1.61, "Verified": true },
                                { "MIN": 2.22, "Verified": false },
                            ],
                            "MIN": 1.61,
                            "Name": "John",
                        },
                        {
                            "GROUP": [{ "MIN": 2.04, "Verified": false }],
                            "MIN": 2.04,
                            "Name": "Alice",
                        },
                    ],
                }),
                common: Common {
                    compare: Some(true),
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
#[mantra_macros::req_test("go.query.simple.group.aggregate.of-same")]
fn query_simple_with_group_by_string_with_inner_group_boolean_and_min_of_min_of_int_succeeds() {
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
                doc_map: Some(json!({ "Name": "John", "Age": 25, "Verified": true })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "John", "Age": 32, "Verified": true })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "John", "Age": 34, "Verified": false })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Carlo", "Age": 55, "Verified": true })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Alice", "Age": 19, "Verified": false })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(groupBy: [Name]) {
                                            Name
                                            MIN(GROUP: {field: MIN})
                                            GROUP (groupBy: [Verified]){
                                                Verified
                                                MIN(GROUP: {field: Age})
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        {
                            "GROUP": [
                                { "MIN": 25, "Verified": true },
                                { "MIN": 34, "Verified": false },
                            ],
                            "MIN": 25,
                            "Name": "John",
                        },
                        {
                            "GROUP": [{ "MIN": 19, "Verified": false }],
                            "MIN": 19,
                            "Name": "Alice",
                        },
                        {
                            "GROUP": [{ "MIN": 55, "Verified": true }],
                            "MIN": 55,
                            "Name": "Carlo",
                        },
                    ],
                }),
                common: Common {
                    compare: Some(true),
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
#[mantra_macros::req_test("go.query.simple.group.aggregate.of-same")]
fn query_simple_with_group_by_string_with_inner_group_boolean_and_min_of_min_of_min_of_float_succeeds(
) {
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
                doc_map: Some(
                    json!({ "Name": "John", "HeightM": 1.82, "Age": 25, "Verified": true }),
                ),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(
                    json!({ "Name": "John", "HeightM": 1.61, "Age": 32, "Verified": true }),
                ),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(
                    json!({ "Name": "John", "HeightM": 2.22, "Age": 34, "Verified": false }),
                ),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(
                    json!({ "Name": "Carlo", "HeightM": 1.74, "Age": 55, "Verified": true }),
                ),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(
                    json!({ "Name": "Alice", "HeightM": 2.04, "Age": 19, "Verified": false }),
                ),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(groupBy: [Name]) {
                                            Name
                                            MIN(GROUP: {field: MIN})
                                            GROUP (groupBy: [Verified]){
                                                Verified
                                                MIN(GROUP: {field: HeightM})
                                                GROUP (groupBy: [Age]){
                                                    Age
                                                    MIN(GROUP: {field: HeightM})
                                                }
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        {
                            "GROUP": [
                                {
                                    "GROUP": [{ "Age": 19, "MIN": 2.04 }],
                                    "MIN": 2.04,
                                    "Verified": false,
                                },
                            ],
                            "MIN": 2.04,
                            "Name": "Alice",
                        },
                        {
                            "GROUP": [
                                {
                                    "GROUP": [
                                        { "Age": 32, "MIN": 1.61 },
                                        { "Age": 25, "MIN": 1.82 },
                                    ],
                                    "MIN": 1.61,
                                    "Verified": true,
                                },
                                {
                                    "GROUP": [{ "Age": 34, "MIN": 2.22 }],
                                    "MIN": 2.22,
                                    "Verified": false,
                                },
                            ],
                            "MIN": 1.61,
                            "Name": "John",
                        },
                        {
                            "GROUP": [
                                {
                                    "GROUP": [{ "Age": 55, "MIN": 1.74 }],
                                    "MIN": 1.74,
                                    "Verified": true,
                                },
                            ],
                            "MIN": 1.74,
                            "Name": "Carlo",
                        },
                    ],
                }),
                common: Common {
                    compare: Some(true),
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
#[mantra_macros::req_test("go.query.simple.group.aggregate.min-max")]
fn query_simple_with_group_by_string_without_rendered_group_and_child_empty_float_min_succeeds() {
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
                doc_map: Some(json!({ "Name": "John", "HeightM": 1.82 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "John", "HeightM": 1.89 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Alice" })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(groupBy: [Name]) {
                                            Name
                                            MIN(GROUP: {field: HeightM})
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [{ "MIN": 1.82, "Name": "John" }, { "MIN": null, "Name": "Alice" }],
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
#[mantra_macros::req_test("go.query.simple.group.aggregate.min-max")]
fn query_simple_with_group_by_string_without_rendered_group_and_child_float_min_succeeds() {
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
                doc_map: Some(json!({ "Name": "John", "HeightM": 1.82 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "John", "HeightM": 1.89 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Alice", "HeightM": 2.04 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(groupBy: [Name]) {
                                            Name
                                            MIN(GROUP: {field: HeightM})
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [{ "MIN": 1.82, "Name": "John" }, { "MIN": 2.04, "Name": "Alice" }],
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
#[mantra_macros::req_test("go.query.simple.group.aggregate.min-max")]
fn query_simple_with_group_by_string_without_rendered_group_and_child_integer_min_on_empty_collection_succeeds(
) {
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
            Request {
                request: r#"
                    query {
                                        Users(groupBy: [Age]) {
                                            Age
                                            MIN(GROUP: {field: Age})
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [] }),
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
#[mantra_macros::req_test("go.query.simple.group.aggregate.min-max")]
fn query_simple_with_group_by_string_without_rendered_group_and_child_integer_min_succeeds() {
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
                doc_map: Some(json!({ "Name": "John", "Age": 32 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "John", "Age": 38 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Alice", "Age": -19 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(groupBy: [Name]) {
                                            Name
                                            MIN(GROUP: {field: Age})
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [{ "MIN": 32, "Name": "John" }, { "MIN": -19, "Name": "Alice" }],
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
#[mantra_macros::req_test("go.query.simple.group.aggregate.min-max")]
fn query_simple_with_group_by_string_without_rendered_group_and_child_nil_min_succeeds() {
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
                doc_map: Some(json!({ "Name": "John", "Age": 32 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "John" })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Alice", "Age": 19 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(groupBy: [Name]) {
                                            Name
                                            MIN(GROUP: {field: Age})
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [{ "MIN": 32, "Name": "John" }, { "MIN": 19, "Name": "Alice" }],
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
#[mantra_macros::req_test("go.query.simple.group.validation")]
fn query_simple_with_group_by_string_without_rendered_group_and_min_of_undefined_returns_error() {
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
                doc_map: Some(json!({ "Name": "John", "Age": 32 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users (groupBy: [Name]) {
                                            Name
                                            MIN
                                        }
                                    }
                "#
                .into(),
                expected_error: Some(
                    "aggregate must be provided with a property to aggregate".into(),
                ),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
