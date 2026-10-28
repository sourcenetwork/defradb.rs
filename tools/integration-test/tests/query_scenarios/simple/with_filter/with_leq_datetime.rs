//! Generated from the conformance scenarios of tests/integration/query/simple/with_filter/with_leq_datetime_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.simple.filter.range.leq")]
fn query_simple_with_date_time_le_filter_block_with_equal_value() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
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
                doc_map: Some(json!({ "Name": "John", "Age": 21, "CreatedAt": "2017-07-23T03:46:56-05:00" })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Bob", "Age": 32, "CreatedAt": "2019-07-23T03:46:56-05:00" })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {CreatedAt: {_leq: "2017-07-23T03:46:56-05:00"}}) {
                                            Name
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "Users": [{ "Name": "John" }] }),
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
#[mantra_macros::req_test("go.query.simple.filter.range.leq")]
fn query_simple_with_date_time_le_filter_block_with_greater_value() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
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
                doc_map: Some(json!({ "Name": "John", "Age": 21, "CreatedAt": "2017-07-23T03:46:56-05:00" })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Bob", "Age": 32, "CreatedAt": "2019-07-23T03:46:56-05:00" })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {CreatedAt: {_leq: "2018-07-23T03:46:56-05:00"}}) {
                                            Name
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "Users": [{ "Name": "John" }] }),
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
#[mantra_macros::req_test("go.query.simple.filter.range.null-operand")]
fn query_simple_with_date_time_le_filter_block_with_null_value() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
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
                    json!({ "Name": "John", "Age": 21, "CreatedAt": "2017-07-23T03:46:56-05:00" }),
                ),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Bob", "Age": 32 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {CreatedAt: {_leq: null}}) {
                                            Name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "Name": "Bob" }] }),
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
#[mantra_macros::req_test("go.query.simple.filter.range.leq")]
fn query_simple_with_nil_date_time_le_and_non_nil_filter_block_should_succeed() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        Name: String
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
                doc_map: Some(json!({ "Age": 21, "CreatedAt": "2017-07-23T03:46:56-05:00", "Name": "John" })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Age": 32, "CreatedAt": "2016-07-23T03:46:56-05:00", "Name": "Bob" })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Age": 44, "Name": "Fred" })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {CreatedAt: {_leq: "2017-07-23T03:46:56-05:00"}}) {
                                            Name
                                            Age
                                            CreatedAt
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "Users": [
                        {
                            "Age": 21,
                            "CreatedAt": "2017-07-23T03:46:56-05:00",
                            "Name": "John",
                        },
                        {
                            "Age": 32,
                            "CreatedAt": "2016-07-23T03:46:56-05:00",
                            "Name": "Bob",
                        },
                    ],
                }),
                common: Common { compare: Some(true), unordered: vec!["/data/Users".into()], ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
