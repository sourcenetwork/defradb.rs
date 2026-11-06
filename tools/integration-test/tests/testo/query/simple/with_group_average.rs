//! Generated from the conformance scenarios of tests/integration/query/simple/with_group_average_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.simple.group.aggregate.of-same")]
fn query_simple_with_group_by_string_with_inner_group_boolean_and_average_of_average_of_average_of_float(
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
                                            AVG(GROUP: {field: AVG})
                                            GROUP (groupBy: [Verified]){
                                                Verified
                                                AVG(GROUP: {field: HeightM})
                                                GROUP (groupBy: [Age]){
                                                    Age
                                                    AVG(GROUP: {field: HeightM})
                                                }
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        {
                            "AVG": 2.04,
                            "GROUP": [
                                {
                                    "AVG": 2.04,
                                    "GROUP": [{ "AVG": 2.04, "Age": 19 }],
                                    "Verified": false,
                                },
                            ],
                            "Name": "Alice",
                        },
                        {
                            "AVG": 1.9675000000000002,
                            "GROUP": [
                                {
                                    "AVG": 1.715,
                                    "GROUP": [
                                        { "AVG": 1.61, "Age": 32 },
                                        { "AVG": 1.82, "Age": 25 },
                                    ],
                                    "Verified": true,
                                },
                                {
                                    "AVG": 2.22,
                                    "GROUP": [{ "AVG": 2.22, "Age": 34 }],
                                    "Verified": false,
                                },
                            ],
                            "Name": "John",
                        },
                        {
                            "AVG": 1.74,
                            "GROUP": [
                                {
                                    "AVG": 1.74,
                                    "GROUP": [{ "AVG": 1.74, "Age": 55 }],
                                    "Verified": true,
                                },
                            ],
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
fn query_simple_with_group_by_string_with_inner_group_boolean_and_average_of_average_of_float() {
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
                                            AVG(GROUP: {field: AVG})
                                            GROUP (groupBy: [Verified]){
                                                Verified
                                                AVG(GROUP: {field: HeightM})
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        {
                            "AVG": 1.74,
                            "GROUP": [{ "AVG": 1.74, "Verified": true }],
                            "Name": "Carlo",
                        },
                        {
                            "AVG": 1.9675000000000002,
                            "GROUP": [
                                { "AVG": 1.715, "Verified": true },
                                { "AVG": 2.22, "Verified": false },
                            ],
                            "Name": "John",
                        },
                        {
                            "AVG": 2.04,
                            "GROUP": [{ "AVG": 2.04, "Verified": false }],
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
fn query_simple_with_group_by_string_with_inner_group_boolean_and_average_of_average_of_int() {
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
                                            AVG(GROUP: {field: AVG})
                                            GROUP (groupBy: [Verified]){
                                                Verified
                                                AVG(GROUP: {field: Age})
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        {
                            "AVG": 31.25,
                            "GROUP": [
                                { "AVG": 28.5, "Verified": true },
                                { "AVG": 34.0, "Verified": false },
                            ],
                            "Name": "John",
                        },
                        {
                            "AVG": 19.0,
                            "GROUP": [{ "AVG": 19.0, "Verified": false }],
                            "Name": "Alice",
                        },
                        {
                            "AVG": 55.0,
                            "GROUP": [{ "AVG": 55.0, "Verified": true }],
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
#[mantra_macros::req_test("go.query.simple.group.validation")]
fn query_simple_with_group_by_string_without_rendered_group_and_average_of_undefined() {
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
                                            AVG
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

#[test]
#[mantra_macros::req_test("go.query.simple.group.aggregate.average")]
fn query_simple_with_group_by_string_without_rendered_group_and_child_empty_float_average() {
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
                                            AVG(GROUP: {field: HeightM})
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [{ "AVG": 1.855, "Name": "John" }, { "AVG": 0.0, "Name": "Alice" }],
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
#[mantra_macros::req_test("go.query.simple.group.aggregate.average")]
fn query_simple_with_group_by_string_without_rendered_group_and_child_float_average() {
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
                                            AVG(GROUP: {field: HeightM})
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [{ "AVG": 1.855, "Name": "John" }, { "AVG": 2.04, "Name": "Alice" }],
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
#[mantra_macros::req_test("go.query.simple.group.aggregate.average")]
fn query_simple_with_group_by_string_without_rendered_group_and_child_integer_average_on_empty_collection(
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
                                            AVG(GROUP: {field: Age})
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
#[mantra_macros::req_test("go.query.simple.group.aggregate.average")]
fn query_simple_with_group_by_string_without_rendered_group_and_child_integer_average() {
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
                                            AVG(GROUP: {field: Age})
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [{ "AVG": 35.0, "Name": "John" }, { "AVG": -19.0, "Name": "Alice" }],
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
#[mantra_macros::req_test("go.query.simple.group.aggregate.average")]
fn query_simple_with_group_by_string_without_rendered_group_and_child_nil_average() {
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
                                            AVG(GROUP: {field: Age})
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [{ "AVG": 32.0, "Name": "John" }, { "AVG": 19.0, "Name": "Alice" }],
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
