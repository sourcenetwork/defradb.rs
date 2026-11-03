//! Generated from the conformance scenarios of tests/integration/query/inline_array/simple_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.inline_array.read.bool-int")]
fn query_inline_array_with_booleans_empty_list() {
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
                doc_map: Some(json!({ "name": "John", "likedIndexes": [] })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                            Users {
                                                name
                                                likedIndexes
                                            }
                                        }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "likedIndexes": [], "name": "John" }] }),
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
#[mantra_macros::req_test("go.query.inline_array.read.bool-int")]
fn query_inline_array_with_booleans_not_empty() {
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
                doc_map: Some(json!({ "name": "John", "likedIndexes": [true, true, false, true] })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                            Users {
                                                name
                                                likedIndexes
                                            }
                                        }
                "#
                .into(),
                expected_data: json!({
                    "Users": [{ "likedIndexes": [true, true, false, true], "name": "John" }],
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
#[mantra_macros::req_test("go.query.inline_array.read.bool-int")]
fn query_inline_array_with_booleans_null() {
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
                doc_map: Some(json!({ "name": "John", "likedIndexes": null })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                             Users {
                                                 name
                                                 likedIndexes
                                             }
                                         }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "likedIndexes": null, "name": "John" }] }),
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
#[mantra_macros::req_test("go.query.inline_array.read.float-string")]
fn query_inline_array_with_floats_empty_list() {
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
                                                favouriteFloats
                                            }
                                        }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "favouriteFloats": [], "name": "John" }] }),
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
#[mantra_macros::req_test("go.query.inline_array.read.float-string")]
fn query_inline_array_with_floats_not_empty() {
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
                                                favouriteFloats
                                            }
                                        }
                "#
                .into(),
                expected_data: json!({
                    "Users": [{ "favouriteFloats": [3.1425, 1e-11, 10.0], "name": "John" }],
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
#[mantra_macros::req_test("go.query.inline_array.read.float-string")]
fn query_inline_array_with_floats_null() {
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
                                                favouriteFloats
                                            }
                                        }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "favouriteFloats": null, "name": "John" }] }),
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
#[mantra_macros::req_test("go.query.inline_array.read.bool-int")]
fn query_inline_array_with_integers_empty_list() {
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
                                                favouriteIntegers
                                            }
                                        }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "favouriteIntegers": [], "name": "John" }] }),
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
#[mantra_macros::req_test("go.query.inline_array.read.bool-int")]
fn query_inline_array_with_integers_missing() {
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
                doc_map: Some(json!({ "name": "John" })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                            Users {
                                                name
                                                favouriteIntegers
                                            }
                                        }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "favouriteIntegers": null, "name": "John" }] }),
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
#[mantra_macros::req_test("go.query.inline_array.read.bool-int")]
fn query_inline_array_with_integers_not_empty_list() {
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
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "favouriteIntegers": [1, 2, 3, 5, 8] })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                            Users {
                                                name
                                                favouriteIntegers
                                            }
                                        }
                "#.into(),
                expected_data: json!({ "Users": [{ "favouriteIntegers": [1, 2, 3, 5, 8], "name": "John" }] }),
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
#[mantra_macros::req_test("go.query.inline_array.read.bool-int")]
fn query_inline_array_with_integers_null() {
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
                                                favouriteIntegers
                                            }
                                        }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "favouriteIntegers": null, "name": "John" }] }),
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
#[mantra_macros::req_test("go.query.inline_array.read.bool-int")]
fn query_inline_array_with_mix_integers_not_empty_list() {
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
                                                favouriteIntegers
                                            }
                                        }
                "#
                .into(),
                expected_data: json!({
                    "Users": [{ "favouriteIntegers": [-1, 2, -1, 1, 0], "name": "Shahzad" }],
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
#[mantra_macros::req_test("go.query.inline_array.read.bool-int")]
fn query_inline_array_with_negative_integers_not_empty_list() {
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
                doc_map: Some(json!({ "name": "Andy", "favouriteIntegers": [-1, -2, -3, -5, -8] })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                            Users {
                                                name
                                                favouriteIntegers
                                            }
                                        }
                "#
                .into(),
                expected_data: json!({
                    "Users": [{ "favouriteIntegers": [-1, -2, -3, -5, -8], "name": "Andy" }],
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
#[mantra_macros::req_test("go.query.inline_array.read.bool-int")]
fn query_inline_array_with_nillable_booleans() {
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
                    json!({ "name": "John", "indexLikesDislikes": [true, true, false, null] }),
                ),
                common: Common {
                    capture: captures([("doc0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            name
                                            indexLikesDislikes
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        {
                            "indexLikesDislikes": [true, true, false, null],
                            "name": "John",
                        },
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
#[mantra_macros::req_test("go.query.inline_array.read.float-string")]
fn query_inline_array_with_nillable_floats() {
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
                doc_map: Some(json!({ "name": "John", "pageRatings": [3.1425, null, -1e-11, 10] })),
                common: Common {
                    capture: captures([("doc0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            name
                                            pageRatings
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [{ "name": "John", "pageRatings": [3.1425, null, -1e-11, 10.0] }],
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
#[mantra_macros::req_test("go.query.inline_array.read.bool-int")]
fn query_inline_array_with_nillable_ints() {
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
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "name": "John", "testScores": [-1, null, -1, 2, 0] })),
                common: Common { capture: captures([("doc0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            name
                                            testScores
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "Users": [{ "name": "John", "testScores": [-1, null, -1, 2, 0] }] }),
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
#[mantra_macros::req_test("go.query.inline_array.read.float-string")]
fn query_inline_array_with_nillable_string() {
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
                doc_map: Some(json!({
                    "name": "John",
                    "pageHeaders": ["", "the previous", "the first", "empty string", null],
                })),
                common: Common {
                    capture: captures([("doc0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users {
                                            name
                                            pageHeaders
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        {
                            "name": "John",
                            "pageHeaders": ["", "the previous", "the first", "empty string", null],
                        },
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
#[mantra_macros::req_test("go.query.inline_array.read.float-string")]
fn query_inline_array_with_strings_empty_list() {
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
                doc_map: Some(json!({ "name": "John", "preferredStrings": [] })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                            Users {
                                                name
                                                preferredStrings
                                            }
                                        }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "name": "John", "preferredStrings": [] }] }),
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
#[mantra_macros::req_test("go.query.inline_array.read.float-string")]
fn query_inline_array_with_strings_not_empty() {
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
                doc_map: Some(json!({
                    "name": "John",
                    "preferredStrings": ["", "the previous", "the first", "empty string"],
                })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                            Users {
                                                name
                                                preferredStrings
                                            }
                                        }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        {
                            "name": "John",
                            "preferredStrings": ["", "the previous", "the first", "empty string"],
                        },
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
#[mantra_macros::req_test("go.query.inline_array.read.float-string")]
fn query_inline_array_with_strings_null() {
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
                doc_map: Some(json!({ "name": "John", "preferredStrings": null })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                            Users {
                                                name
                                                preferredStrings
                                            }
                                        }
                "#
                .into(),
                expected_data: json!({ "Users": [{ "name": "John", "preferredStrings": null }] }),
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
