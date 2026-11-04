//! Generated from the conformance scenarios of tests/integration/query/simple/with_group_count_filter_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.simple.group.aggregate.filtered-count")]
fn query_simple_with_group_by_number_with_rendered_group_and_child_count_with_filter() {
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
                doc_map: Some(json!({ "Name": "Bob", "Age": 32 })),
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
                                        Users(groupBy: [Age]) {
                                            Age
                                            COUNT(GROUP: {filter: {Age: {_gt: 26}}})
                                            GROUP {
                                                Name
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        {
                            "Age": 32,
                            "COUNT": 2,
                            "GROUP": [{ "Name": "Bob" }, { "Name": "John" }],
                        },
                        { "Age": 19, "COUNT": 0, "GROUP": [{ "Name": "Alice" }] },
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
#[mantra_macros::req_test("go.query.simple.group.aggregate.filtered-count")]
fn query_simple_with_group_by_number_with_rendered_group_with_filter_and_child_count_with_different_filter(
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
                doc_map: Some(json!({ "Name": "John", "Age": 32 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Bob", "Age": 32 })),
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
                                        Users(groupBy: [Age]) {
                                            Age
                                            COUNT(GROUP: {filter: {Age: {_gt: 26}}})
                                            GROUP(filter: {Name: {_eq: "John"}}) {
                                                Name
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        { "Age": 32, "COUNT": 2, "GROUP": [{ "Name": "John" }] },
                        { "Age": 19, "COUNT": 0, "GROUP": [] },
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
#[mantra_macros::req_test("go.query.simple.group.aggregate.filtered-count")]
fn query_simple_with_group_by_number_with_rendered_group_with_filter_and_child_count_with_matching_filter(
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
                doc_map: Some(json!({ "Name": "John", "Age": 32 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Bob", "Age": 32 })),
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
                                        Users(groupBy: [Age]) {
                                            Age
                                            COUNT(GROUP: {filter: {Name: {_eq: "John"}}})
                                            GROUP(filter: {Name: {_eq: "John"}}) {
                                                Name
                                            }
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        { "Age": 32, "COUNT": 1, "GROUP": [{ "Name": "John" }] },
                        { "Age": 19, "COUNT": 0, "GROUP": [] },
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
#[mantra_macros::req_test("go.query.simple.group.aggregate.filtered-count")]
fn query_simple_with_group_by_number_without_rendered_group_and_child_count_with_filter() {
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
                doc_map: Some(json!({ "Name": "John", "Age": 32 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Bob", "Age": 32 })),
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
                                        Users(groupBy: [Age]) {
                                            Age
                                            COUNT(GROUP: {filter: {Age: {_gt: 26}}})
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "Users": [{ "Age": 32, "COUNT": 2 }, { "Age": 19, "COUNT": 0 }] }),
                common: Common { compare: Some(true), ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.simple.group.aggregate.filtered-count")]
fn query_simple_with_group_by_number_without_rendered_group_and_child_counts_with_different_filters(
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
                doc_map: Some(json!({ "Name": "John", "Age": 32 })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Bob", "Age": 32 })),
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
                                        Users(groupBy: [Age]) {
                                            Age
                                            C1: COUNT(GROUP: {filter: {Age: {_gt: 26}}})
                                            C2: COUNT(GROUP: {filter: {Age: {_lt: 26}}})
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [{ "Age": 32, "C1": 2, "C2": 0 }, { "Age": 19, "C1": 0, "C2": 1 }],
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
