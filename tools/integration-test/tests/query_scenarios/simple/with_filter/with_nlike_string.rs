//! Generated from the conformance scenarios of tests/integration/query/simple/with_filter/with_nlike_string_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.simple.filter.pattern.nlike")]
fn query_simple_with_not_case_insensitive_like_string_match_exact_string() {
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
                doc_map: Some(json!({
                    "Name": "Daenerys Stormborn of House Targaryen, the First of Her Name",
                    "HeightM": 1.65,
                })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Viserys I Targaryen, King of the Andals", "HeightM": 1.82 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {Name: {_nilike: "daenerys stormborn of house targaryen, the first of her name"}}) {
                                            Name
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "Users": [{ "Name": "Viserys I Targaryen, King of the Andals" }] }),
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
#[mantra_macros::req_test("go.query.simple.filter.pattern.nlike")]
fn query_simple_with_not_case_insensitive_like_string_should_match_prefix_string() {
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
                doc_map: Some(json!({
                    "Name": "Daenerys Stormborn of House Targaryen, the First of Her Name",
                    "HeightM": 1.65,
                })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(
                    json!({ "Name": "Viserys I Targaryen, King of the Andals", "HeightM": 1.82 }),
                ),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {Name: {_nilike: "viserys%"}}) {
                                            Name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        {
                            "Name": "Daenerys Stormborn of House Targaryen, the First of Her Name",
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
#[mantra_macros::req_test("go.query.simple.filter.pattern.nlike")]
fn query_simple_with_not_case_insensitive_like_string_should_match_string() {
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
                doc_map: Some(json!({
                    "Name": "Daenerys Stormborn of House Targaryen, the First of Her Name",
                    "HeightM": 1.65,
                })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Viserys I Targaryen, King of the Andals", "HeightM": 1.82 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {Name: {_nilike: "%stormborn%"}}) {
                                            Name
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "Users": [{ "Name": "Viserys I Targaryen, King of the Andals" }] }),
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
#[mantra_macros::req_test("go.query.simple.filter.pattern.nlike")]
fn query_simple_with_not_case_insensitive_like_string_should_match_suffix_string() {
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
                doc_map: Some(json!({
                    "Name": "Daenerys Stormborn of House Targaryen, the First of Her Name",
                    "HeightM": 1.65,
                })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(
                    json!({ "Name": "Viserys I Targaryen, King of the Andals", "HeightM": 1.82 }),
                ),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {Name: {_nilike: "%andals"}}) {
                                            Name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        {
                            "Name": "Daenerys Stormborn of House Targaryen, the First of Her Name",
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
#[mantra_macros::req_test("go.query.simple.filter.pattern.nlike")]
fn query_simple_with_not_like_string_contains_filter_block_as_prefix_string() {
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
                doc_map: Some(json!({
                    "Name": "Daenerys Stormborn of House Targaryen, the First of Her Name",
                    "HeightM": 1.65,
                })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(
                    json!({ "Name": "Viserys I Targaryen, King of the Andals", "HeightM": 1.82 }),
                ),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {Name: {_nlike: "Viserys%"}}) {
                                            Name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        {
                            "Name": "Daenerys Stormborn of House Targaryen, the First of Her Name",
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
#[mantra_macros::req_test("go.query.simple.filter.pattern.nlike")]
fn query_simple_with_not_like_string_contains_filter_block_as_suffix_string() {
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
                doc_map: Some(json!({
                    "Name": "Daenerys Stormborn of House Targaryen, the First of Her Name",
                    "HeightM": 1.65,
                })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(
                    json!({ "Name": "Viserys I Targaryen, King of the Andals", "HeightM": 1.82 }),
                ),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {Name: {_nlike: "%Andals"}}) {
                                            Name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        {
                            "Name": "Daenerys Stormborn of House Targaryen, the First of Her Name",
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
#[mantra_macros::req_test("go.query.simple.filter.pattern.nlike")]
fn query_simple_with_not_like_string_contains_filter_block_contains_string_muplitple_results() {
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
                doc_map: Some(json!({
                    "Name": "Daenerys Stormborn of House Targaryen, the First of Her Name",
                    "HeightM": 1.65,
                })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(
                    json!({ "Name": "Viserys I Targaryen, King of the Andals", "HeightM": 1.82 }),
                ),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {Name: {_nlike: "%Targaryen%"}}) {
                                            Name
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
#[mantra_macros::req_test("go.query.simple.filter.pattern.nlike")]
fn query_simple_with_not_like_string_contains_filter_block_contains_string() {
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
                doc_map: Some(json!({
                    "Name": "Daenerys Stormborn of House Targaryen, the First of Her Name",
                    "HeightM": 1.65,
                })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Viserys I Targaryen, King of the Andals", "HeightM": 1.82 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {Name: {_nlike: "%Stormborn%"}}) {
                                            Name
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "Users": [{ "Name": "Viserys I Targaryen, King of the Andals" }] }),
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
#[mantra_macros::req_test("go.query.simple.filter.pattern.nlike")]
fn query_simple_with_not_like_string_contains_filter_block_exact_string() {
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
                doc_map: Some(json!({
                    "Name": "Daenerys Stormborn of House Targaryen, the First of Her Name",
                    "HeightM": 1.65,
                })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Viserys I Targaryen, King of the Andals", "HeightM": 1.82 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {Name: {_nlike: "Daenerys Stormborn of House Targaryen, the First of Her Name"}}) {
                                            Name
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "Users": [{ "Name": "Viserys I Targaryen, King of the Andals" }] }),
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
#[mantra_macros::req_test("go.query.simple.filter.pattern.nlike")]
fn query_simple_with_not_like_string_contains_filter_block_has_both() {
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
                doc_map: Some(json!({
                    "Name": "Daenerys Stormborn of House Targaryen, the First of Her Name",
                    "HeightM": 1.65,
                })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Viserys I Targaryen, King of the Andals", "HeightM": 1.82 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {_and: [{Name: {_nlike: "%Baratheon%"}}, {Name: {_nlike: "%Stormborn%"}}]}) {
                                            Name
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "Users": [{ "Name": "Viserys I Targaryen, King of the Andals" }] }),
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
#[mantra_macros::req_test("go.query.simple.filter.pattern.nlike")]
fn query_simple_with_not_like_string_contains_filter_block_has_either() {
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
                doc_map: Some(json!({
                    "Name": "Daenerys Stormborn of House Targaryen, the First of Her Name",
                    "HeightM": 1.65,
                })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Viserys I Targaryen, King of the Andals", "HeightM": 1.82 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {_or: [{Name: {_nlike: "%Baratheon%"}}, {Name: {_nlike: "%Stormborn%"}}]}) {
                                            Name
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "Users": [
                        {
                            "Name": "Daenerys Stormborn of House Targaryen, the First of Her Name",
                        },
                        { "Name": "Viserys I Targaryen, King of the Andals" },
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

#[test]
#[mantra_macros::req_test("go.query.simple.filter.pattern.nlike")]
fn query_simple_with_not_like_string_contains_filter_block_has_start_and_end() {
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
                doc_map: Some(json!({
                    "Name": "Daenerys Stormborn of House Targaryen, the First of Her Name",
                    "HeightM": 1.65,
                })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "Name": "Viserys I Targaryen, King of the Andals", "HeightM": 1.82 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {Name: {_nlike: "Daenerys%Name"}}) {
                                            Name
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "Users": [{ "Name": "Viserys I Targaryen, King of the Andals" }] }),
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
#[mantra_macros::req_test("go.query.simple.filter.pattern.nlike")]
fn query_simple_with_not_like_string_contains_filter_block_prop_not_set() {
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
                doc_map: Some(json!({
                    "Name": "Daenerys Stormborn of House Targaryen, the First of Her Name",
                    "HeightM": 1.65,
                })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(
                    json!({ "Name": "Viserys I Targaryen, King of the Andals", "HeightM": 1.82 }),
                ),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Users".into(),
                doc_map: Some(json!({ "HeightM": 1.92 })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Users(filter: {Name: {_nlike: "%King%"}}) {
                                            Name
                                        }
                                    }
                "#
                .into(),
                expected_data: json!({
                    "Users": [
                        { "Name": null },
                        {
                            "Name": "Daenerys Stormborn of House Targaryen, the First of Her Name",
                        },
                    ],
                }),
                common: Common {
                    compare: Some(true),
                    unordered: vec!["/data/Users".into()],
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
