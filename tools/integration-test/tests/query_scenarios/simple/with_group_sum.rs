//! Generated from the conformance scenarios of tests/integration/query/simple/with_group_sum_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.simple.group.aggregate.of-same")]
fn query_simple_with_group_by_string_with_inner_group_boolean_and_sum_of_sum_of_float() {
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
                                            SUM(GROUP: {field: SUM})
                                            GROUP (groupBy: [Verified]){
                                                Verified
                                                SUM(GROUP: {field: HeightM})
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        {
                            "GROUP": [{ "SUM": 1.74, "Verified": true }],
                            "Name": "Carlo",
                            "SUM": 1.74,
                        },
                        {
                            "GROUP": [
                                { "SUM": 3.43, "Verified": true },
                                { "SUM": 2.22, "Verified": false },
                            ],
                            "Name": "John",
                            "SUM": 5.65,
                        },
                        {
                            "GROUP": [{ "SUM": 2.04, "Verified": false }],
                            "Name": "Alice",
                            "SUM": 2.04,
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
fn query_simple_with_group_by_string_with_inner_group_boolean_and_sum_of_sum_of_int() {
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
                                            SUM(GROUP: {field: SUM})
                                            GROUP (groupBy: [Verified]){
                                                Verified
                                                SUM(GROUP: {field: Age})
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        {
                            "GROUP": [
                                { "SUM": 57, "Verified": true },
                                { "SUM": 34, "Verified": false },
                            ],
                            "Name": "John",
                            "SUM": 91,
                        },
                        {
                            "GROUP": [{ "SUM": 19, "Verified": false }],
                            "Name": "Alice",
                            "SUM": 19,
                        },
                        {
                            "GROUP": [{ "SUM": 55, "Verified": true }],
                            "Name": "Carlo",
                            "SUM": 55,
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
fn query_simple_with_group_by_string_with_inner_group_boolean_and_sum_of_sum_of_sum_of_float() {
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
                                            SUM(GROUP: {field: SUM})
                                            GROUP (groupBy: [Verified]){
                                                Verified
                                                SUM(GROUP: {field: HeightM})
                                                GROUP (groupBy: [Age]){
                                                    Age
                                                    SUM(GROUP: {field: HeightM})
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
                                    "GROUP": [{ "Age": 19, "SUM": 2.04 }],
                                    "SUM": 2.04,
                                    "Verified": false,
                                },
                            ],
                            "Name": "Alice",
                            "SUM": 2.04,
                        },
                        {
                            "GROUP": [
                                {
                                    "GROUP": [
                                        { "Age": 32, "SUM": 1.61 },
                                        { "Age": 25, "SUM": 1.82 },
                                    ],
                                    "SUM": 3.43,
                                    "Verified": true,
                                },
                                {
                                    "GROUP": [{ "Age": 34, "SUM": 2.22 }],
                                    "SUM": 2.22,
                                    "Verified": false,
                                },
                            ],
                            "Name": "John",
                            "SUM": 5.65,
                        },
                        {
                            "GROUP": [
                                {
                                    "GROUP": [{ "Age": 55, "SUM": 1.74 }],
                                    "SUM": 1.74,
                                    "Verified": true,
                                },
                            ],
                            "Name": "Carlo",
                            "SUM": 1.74,
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
#[mantra_macros::req_test("go.query.simple.group.aggregate.sum")]
fn query_simple_with_group_by_string_without_rendered_group_and_child_empty_float_sum() {
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
                                            SUM(GROUP: {field: HeightM})
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [{ "Name": "John", "SUM": 3.71 }, { "Name": "Alice", "SUM": 0.0 }],
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
#[mantra_macros::req_test("go.query.simple.group.aggregate.sum")]
fn query_simple_with_group_by_string_without_rendered_group_and_child_float_sum() {
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
                                            SUM(GROUP: {field: HeightM})
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [{ "Name": "John", "SUM": 3.71 }, { "Name": "Alice", "SUM": 2.04 }],
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
#[mantra_macros::req_test("go.query.simple.group.aggregate.sum")]
fn query_simple_with_group_by_string_without_rendered_group_and_child_integer_sum_on_empty_collection(
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
                                            SUM(GROUP: {field: Age})
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
#[mantra_macros::req_test("go.query.simple.group.aggregate.sum")]
fn query_simple_with_group_by_string_without_rendered_group_and_child_integer_sum() {
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
                                            SUM(GROUP: {field: Age})
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [{ "Name": "John", "SUM": 70 }, { "Name": "Alice", "SUM": -19 }],
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
#[mantra_macros::req_test("go.query.simple.group.aggregate.sum")]
fn query_simple_with_group_by_string_without_rendered_group_and_child_nil_sum() {
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
                                            SUM(GROUP: {field: Age})
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [{ "Name": "John", "SUM": 32 }, { "Name": "Alice", "SUM": 19 }],
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
fn query_simple_with_group_by_string_without_rendered_group_and_sum_of_undefined() {
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
                                            SUM
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
