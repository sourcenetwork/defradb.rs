//! Generated from the conformance scenarios of tests/integration/query/simple/with_group_average_filter_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.simple.group.aggregate.filtered-average")]
fn query_simple_with_group_by_string_with_rendered_group_and_child_average_with_date_time_filter() {
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
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "John", "Age": 34, "CreatedAt": "2019-07-23T03:46:56-05:00" })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "John", "Age": 32, "CreatedAt": "2018-07-23T03:46:56-05:00" })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Alice", "Age": 19, "CreatedAt": "2011-07-23T03:46:56-05:00" })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(groupBy: [Name]) {
                                            Name
                                            AVG(GROUP: {field: Age, filter: {CreatedAt: {_gt: "2017-07-23T03:46:56-05:00"}}})
                                            GROUP {
                                                Age
                                            }
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "Users": [
                        {
                            "AVG": 33.0,
                            "GROUP": [{ "Age": 34 }, { "Age": 32 }],
                            "Name": "John",
                        },
                        { "AVG": 0.0, "GROUP": [{ "Age": 19 }], "Name": "Alice" },
                    ],
                }),
                common: Common { compare: Some(true), unordered_documents: vec!["/data/Users".into()], ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.simple.group.aggregate.filtered-average")]
fn query_simple_with_group_by_string_with_rendered_group_and_child_average_with_filter() {
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
                doc_map: Some(json!({ "Name": "John", "Age": 34 })),
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
                doc_map: Some(json!({ "Name": "Alice", "Age": 19 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(groupBy: [Name]) {
                                            Name
                                            AVG(GROUP: {field: Age, filter: {Age: {_gt: 26}}})
                                            GROUP {
                                                Age
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        {
                            "AVG": 33.0,
                            "GROUP": [{ "Age": 34 }, { "Age": 32 }],
                            "Name": "John",
                        },
                        { "AVG": 0.0, "GROUP": [{ "Age": 19 }], "Name": "Alice" },
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
#[mantra_macros::req_test("go.query.simple.group.aggregate.filtered-average")]
fn query_simple_with_group_by_string_with_rendered_group_with_filter_and_child_average_with_different_filter(
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
                doc_map: Some(json!({ "Name": "John", "Age": 34 })),
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
                doc_map: Some(json!({ "Name": "Alice", "Age": 19 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(groupBy: [Name]) {
                                            Name
                                            AVG(GROUP: {field: Age, filter: {Age: {_gt: 33}}})
                                            GROUP(filter: {Age: {_lt: 33}}) {
                                                Age
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        { "AVG": 34.0, "GROUP": [{ "Age": 32 }], "Name": "John" },
                        { "AVG": 0.0, "GROUP": [{ "Age": 19 }], "Name": "Alice" },
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
#[mantra_macros::req_test("go.query.simple.group.aggregate.filtered-average")]
fn query_simple_with_group_by_string_with_rendered_group_with_filter_and_child_average_with_matching_date_time_filter(
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
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "John", "Age": 34, "CreatedAt": "2017-07-23T03:46:56-05:00" })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "John", "Age": 32, "CreatedAt": "2011-07-23T03:46:56-05:00" })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Alice", "Age": 19, "CreatedAt": "2010-07-23T03:46:56-05:00" })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(groupBy: [Name]) {
                                            Name
                                            AVG(GROUP: {field: Age, filter: {CreatedAt: {_gt: "2016-07-23T03:46:56-05:00"}}})
                                            GROUP(filter: {CreatedAt: {_gt: "2016-07-23T03:46:56-05:00"}}) {
                                                Age
                                            }
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "Users": [
                        { "AVG": 34.0, "GROUP": [{ "Age": 34 }], "Name": "John" },
                        { "AVG": 0.0, "GROUP": [], "Name": "Alice" },
                    ],
                }),
                common: Common { compare: Some(true), unordered_documents: vec!["/data/Users".into()], ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.simple.group.aggregate.filtered-average")]
fn query_simple_with_group_by_string_with_rendered_group_with_filter_and_child_average_with_matching_filter(
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
                doc_map: Some(json!({ "Name": "John", "Age": 34 })),
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
                doc_map: Some(json!({ "Name": "Alice", "Age": 19 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(groupBy: [Name]) {
                                            Name
                                            AVG(GROUP: {field: Age, filter: {Age: {_gt: 33}}})
                                            GROUP(filter: {Age: {_gt: 33}}) {
                                                Age
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        { "AVG": 34.0, "GROUP": [{ "Age": 34 }], "Name": "John" },
                        { "AVG": 0.0, "GROUP": [], "Name": "Alice" },
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
#[mantra_macros::req_test("go.query.simple.group.aggregate.filtered-average")]
fn query_simple_with_group_by_string_without_rendered_group_and_child_average_with_filter_and_nil_item(
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
                doc_map: Some(json!({ "Name": "John", "Age": 34 })),
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
                doc_map: Some(json!({ "Name": "John", "Age": 30 })),
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
                                            AVG(GROUP: {field: Age, filter: {Age: {_lt: 33}}})
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [{ "AVG": 31.0, "Name": "John" }, { "AVG": 19.0, "Name": "Alice" }],
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
#[mantra_macros::req_test("go.query.simple.group.aggregate.filtered-average")]
fn query_simple_with_group_by_string_without_rendered_group_and_child_average_with_filter() {
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
                doc_map: Some(json!({ "Name": "John", "Age": 34 })),
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
                doc_map: Some(json!({ "Name": "Alice", "Age": 19 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(groupBy: [Name]) {
                                            Name
                                            AVG(GROUP: {field: Age, filter: {Age: {_gt: 26}}})
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [{ "AVG": 33.0, "Name": "John" }, { "AVG": 0.0, "Name": "Alice" }],
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
#[mantra_macros::req_test("go.query.simple.group.aggregate.filtered-average")]
fn query_simple_with_group_by_string_without_rendered_group_and_child_averages_with_different_filters(
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
                doc_map: Some(json!({ "Name": "John", "Age": 34 })),
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
                doc_map: Some(json!({ "Name": "Alice", "Age": 19 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(groupBy: [Name]) {
                                            Name
                                            A1: AVG(GROUP: {field: Age, filter: {Age: {_gt: 26}}})
                                            A2: AVG(GROUP: {field: Age, filter: {Age: {_lt: 26}}})
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        { "A1": 33.0, "A2": 0.0, "Name": "John" },
                        { "A1": 0.0, "A2": 19.0, "Name": "Alice" },
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
