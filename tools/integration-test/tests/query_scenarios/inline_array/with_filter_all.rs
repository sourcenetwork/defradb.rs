//! Generated from the conformance scenarios of tests/integration/query/inline_array/with_filter_all_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.inline_array.filter.all")]
fn query_inline_boolean_array_with_all_filter_succeeds() {
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
                doc_map: Some(json!({ "name": "Shahzad", "indexLikesDislikes": [false, false] })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Fred", "indexLikesDislikes": [null, true] })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {indexLikesDislikes: {_all: {_neq: null}}}) {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "name": "Shahzad" }] }),
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
#[mantra_macros::req_test("go.query.inline_array.filter.all")]
fn query_inline_float_array_with_all_filter_succeeds() {
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
                doc_map: Some(json!({ "name": "Shahzad", "pageRatings": [50, 80] })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Fred", "pageRatings": [null, 60] })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {pageRatings: {_all: {_neq: null}}}) {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "name": "Shahzad" }] }),
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
#[mantra_macros::req_test("go.query.inline_array.filter.all")]
fn query_inline_int_array_with_all_filter_succeeds() {
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
                doc_map: Some(json!({ "name": "Shahzad", "testScores": [50, 80] })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Fred", "testScores": [null, 60] })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {testScores: {_all: {_neq: null}}}) {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "name": "Shahzad" }] }),
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
#[mantra_macros::req_test("go.query.inline_array.filter.all")]
fn query_inline_not_null_boolean_array_with_all_filter_succeeds() {
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
                doc_map: Some(json!({ "name": "Shahzad", "likedIndexes": [false, false] })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Fred", "likedIndexes": [true, true] })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {likedIndexes: {_all: {_eq: true}}}) {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "name": "Fred" }] }),
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
#[mantra_macros::req_test("go.query.inline_array.filter.all")]
fn query_inline_not_null_float_array_with_all_filter_succeeds() {
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
                doc_map: Some(json!({ "name": "Shahzad", "pageRatings": [50, 80] })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Fred", "pageRatings": [0, 60] })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {pageRatings: {_all: {_lt: 70}}}) {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "name": "Fred" }] }),
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
#[mantra_macros::req_test("go.query.inline_array.filter.all")]
fn query_inline_not_null_int_array_with_all_filter_succeeds() {
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
                doc_map: Some(json!({ "name": "Shahzad", "testScores": [50, 80] })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Fred", "testScores": [0, 60] })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {testScores: {_all: {_lt: 70}}}) {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "name": "Fred" }] }),
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
#[mantra_macros::req_test("go.query.inline_array.filter.all")]
fn query_inline_not_null_string_array_with_all_filter_succeeds() {
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
                    json!({ "name": "Shahzad", "preferredStrings": ["first", "second"] }),
                ),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Fred", "preferredStrings": ["", "second"] })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {preferredStrings: {_all: {_neq: ""}}}) {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "name": "Shahzad" }] }),
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
#[mantra_macros::req_test("go.query.inline_array.filter.all")]
fn query_inline_string_array_with_all_filter_and_null_value_succeeds() {
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
                doc_map: Some(json!({ "name": "Islam", "pageHeaders": null })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {pageHeaders: {_all: {_eq: null}}}) {
                                            name
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
#[mantra_macros::req_test("go.query.inline_array.filter.all")]
fn query_inline_string_array_with_all_filter_succeeds() {
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
                doc_map: Some(json!({ "name": "Shahzad", "pageHeaders": ["first", "second"] })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "Fred", "pageHeaders": [null, "second"] })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {pageHeaders: {_all: {_neq: null}}}) {
                                            name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "name": "Shahzad" }] }),
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
