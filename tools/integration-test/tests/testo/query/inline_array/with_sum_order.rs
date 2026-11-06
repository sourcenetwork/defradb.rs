//! Generated from the conformance scenarios of tests/integration/query/inline_array/with_sum_order_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.inline_array.aggregate.alias-order")]
fn query_inline_integer_array_with_null_and_sum_and_order_succeeds() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        likedIndexes: [Boolean!]
                        indexLikesDislikes: [Boolean]
                        favouriteIntegers: [Int!]
                        testScores: [Int]
                        favouriteFloats: [Float!]
                        pageRatings: [Float]
                        favouriteFloat32s: [Float32!]
                        pageRatings32: [Float32]
                        favouriteFloat64s: [Float64!]
                        pageRatings64: [Float64]
                        preferredStrings: [String!]
                        pageHeaders: [String]
                        preferredDateTimes: [DateTime!]
                        loginTimes: [DateTime]
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(
                    json!({ "testScores": [3, 4, 5, null], "pageRatings": [1.0, 2.0, 3.0, null] }),
                ),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({
                    "testScores": [30, 40, 50, null],
                    "pageRatings": [10.0, 20.0, 30.0, null],
                })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(order: {_alias: {total: DESC}}) {
                                            total: SUM(testScores: {}, pageRatings: {})
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "total": 180 }, { "total": 18 }] }),
                common: Common {
                    compare: Some(true),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(order: {_alias: {total: ASC}}) {
                                            total: SUM(testScores: {}, pageRatings: {})
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "total": 18 }, { "total": 180 }] }),
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
#[mantra_macros::req_test("go.query.inline_array.aggregate.alias-order")]
fn query_inline_integer_array_with_sum_and_order_succeeds() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Users {
                        name: String
                        likedIndexes: [Boolean!]
                        indexLikesDislikes: [Boolean]
                        favouriteIntegers: [Int!]
                        testScores: [Int]
                        favouriteFloats: [Float!]
                        pageRatings: [Float]
                        favouriteFloat32s: [Float32!]
                        pageRatings32: [Float32]
                        favouriteFloat64s: [Float64!]
                        pageRatings64: [Float64]
                        preferredStrings: [String!]
                        pageHeaders: [String]
                        preferredDateTimes: [DateTime!]
                        loginTimes: [DateTime]
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "testScores": [3, 4, 5], "pageRatings": [1.0, 2.0, 3.0] })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(
                    json!({ "testScores": [30, 40, 50], "pageRatings": [10.0, 20.0, 30.0] }),
                ),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(order: {_alias: {total: DESC}}) {
                                            total: SUM(testScores: {}, pageRatings: {})
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "total": 180 }, { "total": 18 }] }),
                common: Common {
                    compare: Some(true),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(order: {_alias: {total: ASC}}) {
                                            total: SUM(testScores: {}, pageRatings: {})
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "total": 18 }, { "total": 180 }] }),
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
