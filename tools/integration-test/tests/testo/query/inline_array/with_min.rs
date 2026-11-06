//! Generated from the conformance scenarios of tests/integration/query/inline_array/with_min_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.inline_array.aggregate.min.basic")]
fn query_inline_float_array_with_min_and_empty_array_succeeds() {
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
                doc_map: Some(json!({ "name": "John", "favouriteFloats": [] })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            name
                                            MIN(favouriteFloats: {})
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "MIN": null, "name": "John" }] }),
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
#[mantra_macros::req_test("go.query.inline_array.aggregate.min.basic")]
fn query_inline_float_array_with_min_and_null_array_succeeds() {
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
                doc_map: Some(json!({ "name": "John", "favouriteFloats": null })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            name
                                            MIN(favouriteFloats: {})
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "MIN": null, "name": "John" }] }),
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
#[mantra_macros::req_test("go.query.inline_array.aggregate.min.basic")]
fn query_inline_float_array_with_min_and_populated_array_succeeds() {
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
                doc_map: Some(json!({ "name": "John", "favouriteFloats": [3.1425, 1e-11, 10] })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            name
                                            MIN(favouriteFloats: {})
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "MIN": 1e-11, "name": "John" }] }),
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
#[mantra_macros::req_test("go.query.inline_array.aggregate.min.basic")]
fn query_inline_integer_array_with_min_and_empty_array_succeeds() {
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
                doc_map: Some(json!({ "name": "John", "favouriteIntegers": [] })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            name
                                            MIN(favouriteIntegers: {})
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "MIN": null, "name": "John" }] }),
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
#[mantra_macros::req_test("go.query.inline_array.aggregate.min.basic")]
fn query_inline_integer_array_with_min_and_null_array_succeeds() {
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
                doc_map: Some(json!({ "name": "John", "favouriteIntegers": null })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            name
                                            MIN(favouriteIntegers: {})
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "MIN": null, "name": "John" }] }),
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
#[mantra_macros::req_test("go.query.inline_array.aggregate.min.basic")]
fn query_inline_integer_array_with_min_and_populated_array_succeeds() {
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
                doc_map: Some(json!({ "name": "Shahzad", "favouriteIntegers": [-1, 2, -1, 1, 0] })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            name
                                            MIN(favouriteIntegers: {})
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "MIN": -1, "name": "Shahzad" }] }),
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
#[mantra_macros::req_test("go.query.inline_array.aggregate.min.basic")]
fn query_inline_nillable_float_array_with_min_and_populated_array_succeeds() {
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
                    json!({ "name": "Shahzad", "pageRatings": [3.1425, 1e-11, 10, null] }),
                ),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            name
                                            MIN(pageRatings: {})
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "MIN": 1e-11, "name": "Shahzad" }] }),
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
#[mantra_macros::req_test("go.query.inline_array.aggregate.min.basic")]
fn query_inline_nillable_integer_array_with_min_and_populated_array_succeeds() {
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
                doc_map: Some(json!({ "name": "Shahzad", "testScores": [-1, 2, null, 1, 0] })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            name
                                            MIN(testScores: {})
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "MIN": -1, "name": "Shahzad" }] }),
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
