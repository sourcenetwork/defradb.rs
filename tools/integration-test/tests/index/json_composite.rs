//! Generated from the conformance scenarios of tests/integration/index/json_composite_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.index.json.composite")]
fn json_array_composite_index_json_array_with_array_field_should_fetch_using_index_unique_combination_non_unique_custom_numbers_element(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "custom"}, {field: "tags"}]) {
                        name: String 
                        custom: JSON 
                        tags: [String]
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({
                    "custom": { "numbers": [3, 4] },
                    "name": "John",
                    "tags": ["colleague", "mentor", "unique"],
                })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({
                    "custom": { "numbers": [3, 15] },
                    "name": "Islam",
                    "tags": ["friend", "mentor"],
                })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({
                    "custom": { "numbers": [4, 6] },
                    "name": "Shahzad",
                    "tags": ["colleague"],
                })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": [5, 7] }, "name": "Keenan", "tags": ["family"] })),
                common: Common { capture: captures([("doc0_3", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": [1, 5, 8] }, "name": "Addo", "tags": ["family"] })),
                common: Common { capture: captures([("doc0_4", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": [6, 9] }, "name": "Bruno", "tags": ["dude"] })),
                common: Common { capture: captures([("doc0_5", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": [] }, "name": "Andy", "tags": ["friend"] })),
                common: Common { capture: captures([("doc0_6", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({
                    "custom": { "numbers": [7, 10] },
                    "name": "Chris",
                    "tags": ["colleague"],
                })),
                common: Common { capture: captures([("doc0_7", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                    User(filter: {
                                        custom: {numbers: {_any: {_eq: 3}}},
                                        tags: {_any: {_eq: "unique"}}
                                    }) {
                                        name
                                    }
                                }
                "#.into(),
                expected_data: json!({ "User": [{ "name": "John" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                                    User(filter: {
                                        custom: {numbers: {_any: {_eq: 3}}},
                                        tags: {_any: {_eq: "unique"}}
                                    }) {
                                        name
                                    }
                                }
                "#.into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 1 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.json.composite")]
fn json_array_composite_index_json_array_with_array_field_should_fetch_using_index_unique_combination_non_unique_tags(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "custom"}, {field: "tags"}]) {
                        name: String 
                        custom: JSON 
                        tags: [String]
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({
                    "custom": { "numbers": [3, 4] },
                    "name": "John",
                    "tags": ["colleague", "mentor", "unique"],
                })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({
                    "custom": { "numbers": [3, 15] },
                    "name": "Islam",
                    "tags": ["friend", "mentor"],
                })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({
                    "custom": { "numbers": [4, 6] },
                    "name": "Shahzad",
                    "tags": ["colleague"],
                })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": [5, 7] }, "name": "Keenan", "tags": ["family"] })),
                common: Common { capture: captures([("doc0_3", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": [1, 5, 8] }, "name": "Addo", "tags": ["family"] })),
                common: Common { capture: captures([("doc0_4", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": [6, 9] }, "name": "Bruno", "tags": ["dude"] })),
                common: Common { capture: captures([("doc0_5", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": [] }, "name": "Andy", "tags": ["friend"] })),
                common: Common { capture: captures([("doc0_6", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({
                    "custom": { "numbers": [7, 10] },
                    "name": "Chris",
                    "tags": ["colleague"],
                })),
                common: Common { capture: captures([("doc0_7", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                    User(filter: {
                                        custom: {numbers: {_any: {_eq: 15}}},
                                        tags: {_any: {_eq: "mentor"}}
                                    }) {
                                        name
                                    }
                                }
                "#.into(),
                expected_data: json!({ "User": [{ "name": "Islam" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                                    User(filter: {
                                        custom: {numbers: {_any: {_eq: 15}}},
                                        tags: {_any: {_eq: "mentor"}}
                                    }) {
                                        name
                                    }
                                }
                "#.into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 1 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.json.composite")]
fn json_array_composite_index_json_array_with_array_field_should_fetch_using_index_match_first_part_of_the_composite_index(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "custom"}, {field: "tags"}]) {
                        name: String 
                        custom: JSON 
                        tags: [String]
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({
                    "custom": { "numbers": [3, 4] },
                    "name": "John",
                    "tags": ["colleague", "mentor", "unique"],
                })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({
                    "custom": { "numbers": [3, 15] },
                    "name": "Islam",
                    "tags": ["friend", "mentor"],
                })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({
                    "custom": { "numbers": [4, 6] },
                    "name": "Shahzad",
                    "tags": ["colleague"],
                })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": [5, 7] }, "name": "Keenan", "tags": ["family"] })),
                common: Common { capture: captures([("doc0_3", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": [1, 5, 8] }, "name": "Addo", "tags": ["family"] })),
                common: Common { capture: captures([("doc0_4", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": [6, 9] }, "name": "Bruno", "tags": ["dude"] })),
                common: Common { capture: captures([("doc0_5", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": [] }, "name": "Andy", "tags": ["friend"] })),
                common: Common { capture: captures([("doc0_6", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({
                    "custom": { "numbers": [7, 10] },
                    "name": "Chris",
                    "tags": ["colleague"],
                })),
                common: Common { capture: captures([("doc0_7", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                    User(filter: {custom: {numbers: {_any: {_eq: 5}}}}) {
                                        name
                                    }
                                }
                "#.into(),
                expected_data: json!({ "User": [{ "name": "Keenan" }, { "name": "Addo" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                                    User(filter: {custom: {numbers: {_any: {_eq: 5}}}}) {
                                        name
                                    }
                                }
                "#.into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 2 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.json.composite")]
fn json_array_composite_index_json_array_with_array_field_should_fetch_using_index_non_unique_combination(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "custom"}, {field: "tags"}]) {
                        name: String 
                        custom: JSON 
                        tags: [String]
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({
                    "custom": { "numbers": [3, 4] },
                    "name": "John",
                    "tags": ["colleague", "mentor", "unique"],
                })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({
                    "custom": { "numbers": [3, 15] },
                    "name": "Islam",
                    "tags": ["friend", "mentor"],
                })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({
                    "custom": { "numbers": [4, 6] },
                    "name": "Shahzad",
                    "tags": ["colleague"],
                })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": [5, 7] }, "name": "Keenan", "tags": ["family"] })),
                common: Common { capture: captures([("doc0_3", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": [1, 5, 8] }, "name": "Addo", "tags": ["family"] })),
                common: Common { capture: captures([("doc0_4", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": [6, 9] }, "name": "Bruno", "tags": ["dude"] })),
                common: Common { capture: captures([("doc0_5", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": [] }, "name": "Andy", "tags": ["friend"] })),
                common: Common { capture: captures([("doc0_6", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({
                    "custom": { "numbers": [7, 10] },
                    "name": "Chris",
                    "tags": ["colleague"],
                })),
                common: Common { capture: captures([("doc0_7", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                    User(filter: {
                                        custom: {numbers: {_any: {_eq: 5}}},
                                        tags: {_any: {_eq: "family"}}
                                    }) {
                                        name
                                    }
                                }
                "#.into(),
                expected_data: json!({ "User": [{ "name": "Keenan" }, { "name": "Addo" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                                    User(filter: {
                                        custom: {numbers: {_any: {_eq: 5}}},
                                        tags: {_any: {_eq: "family"}}
                                    }) {
                                        name
                                    }
                                }
                "#.into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 2 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.json.composite")]
fn json_array_composite_index_json_array_with_array_field_should_fetch_using_index_match_second_part_of_the_composite_index(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "custom"}, {field: "tags"}]) {
                        name: String 
                        custom: JSON 
                        tags: [String]
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({
                    "custom": { "numbers": [3, 4] },
                    "name": "John",
                    "tags": ["colleague", "mentor", "unique"],
                })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({
                    "custom": { "numbers": [3, 15] },
                    "name": "Islam",
                    "tags": ["friend", "mentor"],
                })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({
                    "custom": { "numbers": [4, 6] },
                    "name": "Shahzad",
                    "tags": ["colleague"],
                })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": [5, 7] }, "name": "Keenan", "tags": ["family"] })),
                common: Common { capture: captures([("doc0_3", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": [1, 5, 8] }, "name": "Addo", "tags": ["family"] })),
                common: Common { capture: captures([("doc0_4", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": [6, 9] }, "name": "Bruno", "tags": ["dude"] })),
                common: Common { capture: captures([("doc0_5", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "custom": { "numbers": [] }, "name": "Andy", "tags": ["friend"] })),
                common: Common { capture: captures([("doc0_6", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({
                    "custom": { "numbers": [7, 10] },
                    "name": "Chris",
                    "tags": ["colleague"],
                })),
                common: Common { capture: captures([("doc0_7", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                    User(filter: {tags: {_any: {_eq: "dude"}}}) {
                                        name
                                    }
                                }
                "#.into(),
                expected_data: json!({ "User": [{ "name": "Bruno" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                                    User(filter: {tags: {_any: {_eq: "dude"}}}) {
                                        name
                                    }
                                }
                "#.into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 0 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.json.composite")]
fn json_array_composite_index_json_array_with_scalar_should_fetch_using_index_unique_combination_non_unique_custom_numbers_element(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "custom"}, {field: "age"}]) {
                        name: String 
                        custom: JSON 
                        age: Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(
                    json!({ "age": 30, "custom": { "numbers": [3, 4] }, "name": "John" }),
                ),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(
                    json!({ "age": 25, "custom": { "numbers": [3, 5] }, "name": "Islam" }),
                ),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(
                    json!({ "age": 30, "custom": { "numbers": [4, 6] }, "name": "Shahzad" }),
                ),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(
                    json!({ "age": 35, "custom": { "numbers": [5, 7] }, "name": "Keenan" }),
                ),
                common: Common {
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(
                    json!({ "age": 35, "custom": { "numbers": [1, 5, 8] }, "name": "Addo" }),
                ),
                common: Common {
                    capture: captures([("doc0_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(
                    json!({ "age": 40, "custom": { "numbers": [6, 9] }, "name": "Bruno" }),
                ),
                common: Common {
                    capture: captures([("doc0_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 35, "custom": { "numbers": [] }, "name": "Andy" })),
                common: Common {
                    capture: captures([("doc0_6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(
                    json!({ "age": null, "custom": { "numbers": [7, 10] }, "name": "Chris" }),
                ),
                common: Common {
                    capture: captures([("doc0_7", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                    User(filter: {
                                        custom: {numbers: {_any: {_eq: 3}}}, 
                                        age: {_eq: 25}
                                    }) {
                                        name
                                    }
                                }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": "Islam" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                                    User(filter: {
                                        custom: {numbers: {_any: {_eq: 3}}}, 
                                        age: {_eq: 25}
                                    }) {
                                        name
                                    }
                                }
                "#
                .into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 1 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.json.composite")]
fn json_array_composite_index_json_array_with_scalar_should_fetch_using_index_unique_combination_non_unique_age(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "custom"}, {field: "age"}]) {
                        name: String 
                        custom: JSON 
                        age: Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(
                    json!({ "age": 30, "custom": { "numbers": [3, 4] }, "name": "John" }),
                ),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(
                    json!({ "age": 25, "custom": { "numbers": [3, 5] }, "name": "Islam" }),
                ),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(
                    json!({ "age": 30, "custom": { "numbers": [4, 6] }, "name": "Shahzad" }),
                ),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(
                    json!({ "age": 35, "custom": { "numbers": [5, 7] }, "name": "Keenan" }),
                ),
                common: Common {
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(
                    json!({ "age": 35, "custom": { "numbers": [1, 5, 8] }, "name": "Addo" }),
                ),
                common: Common {
                    capture: captures([("doc0_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(
                    json!({ "age": 40, "custom": { "numbers": [6, 9] }, "name": "Bruno" }),
                ),
                common: Common {
                    capture: captures([("doc0_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 35, "custom": { "numbers": [] }, "name": "Andy" })),
                common: Common {
                    capture: captures([("doc0_6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(
                    json!({ "age": null, "custom": { "numbers": [7, 10] }, "name": "Chris" }),
                ),
                common: Common {
                    capture: captures([("doc0_7", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                    User(filter: {
                                        custom: {numbers: {_any: {_eq: 3}}}, 
                                        age: {_eq: 30}
                                    }) {
                                        name
                                    }
                                }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": "John" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                                    User(filter: {
                                        custom: {numbers: {_any: {_eq: 3}}}, 
                                        age: {_eq: 30}
                                    }) {
                                        name
                                    }
                                }
                "#
                .into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 1 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.json.composite")]
fn json_array_composite_index_json_array_with_scalar_should_fetch_using_index_match_first_part_of_the_composite_index(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "custom"}, {field: "age"}]) {
                        name: String 
                        custom: JSON 
                        age: Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(
                    json!({ "age": 30, "custom": { "numbers": [3, 4] }, "name": "John" }),
                ),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(
                    json!({ "age": 25, "custom": { "numbers": [3, 5] }, "name": "Islam" }),
                ),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(
                    json!({ "age": 30, "custom": { "numbers": [4, 6] }, "name": "Shahzad" }),
                ),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(
                    json!({ "age": 35, "custom": { "numbers": [5, 7] }, "name": "Keenan" }),
                ),
                common: Common {
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(
                    json!({ "age": 35, "custom": { "numbers": [1, 5, 8] }, "name": "Addo" }),
                ),
                common: Common {
                    capture: captures([("doc0_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(
                    json!({ "age": 40, "custom": { "numbers": [6, 9] }, "name": "Bruno" }),
                ),
                common: Common {
                    capture: captures([("doc0_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 35, "custom": { "numbers": [] }, "name": "Andy" })),
                common: Common {
                    capture: captures([("doc0_6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(
                    json!({ "age": null, "custom": { "numbers": [7, 10] }, "name": "Chris" }),
                ),
                common: Common {
                    capture: captures([("doc0_7", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                    User(filter: {custom: {numbers: {_any: {_eq: 3}}}}) {
                                        name
                                    }
                                }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": "Islam" }, { "name": "John" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                                    User(filter: {custom: {numbers: {_any: {_eq: 3}}}}) {
                                        name
                                    }
                                }
                "#
                .into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 2 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.json.composite")]
fn json_array_composite_index_json_array_with_scalar_should_fetch_using_index_non_unique_combination(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "custom"}, {field: "age"}]) {
                        name: String 
                        custom: JSON 
                        age: Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(
                    json!({ "age": 30, "custom": { "numbers": [3, 4] }, "name": "John" }),
                ),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(
                    json!({ "age": 25, "custom": { "numbers": [3, 5] }, "name": "Islam" }),
                ),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(
                    json!({ "age": 30, "custom": { "numbers": [4, 6] }, "name": "Shahzad" }),
                ),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(
                    json!({ "age": 35, "custom": { "numbers": [5, 7] }, "name": "Keenan" }),
                ),
                common: Common {
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(
                    json!({ "age": 35, "custom": { "numbers": [1, 5, 8] }, "name": "Addo" }),
                ),
                common: Common {
                    capture: captures([("doc0_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(
                    json!({ "age": 40, "custom": { "numbers": [6, 9] }, "name": "Bruno" }),
                ),
                common: Common {
                    capture: captures([("doc0_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 35, "custom": { "numbers": [] }, "name": "Andy" })),
                common: Common {
                    capture: captures([("doc0_6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(
                    json!({ "age": null, "custom": { "numbers": [7, 10] }, "name": "Chris" }),
                ),
                common: Common {
                    capture: captures([("doc0_7", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                    User(filter: {
                                        custom: {numbers: {_any: {_eq: 5}}}, 
                                        age: {_eq: 35}
                                    }) {
                                        name
                                    }
                                }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": "Keenan" }, { "name": "Addo" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                                    User(filter: {
                                        custom: {numbers: {_any: {_eq: 5}}}, 
                                        age: {_eq: 35}
                                    }) {
                                        name
                                    }
                                }
                "#
                .into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 2 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.json.composite")]
fn json_array_composite_index_json_array_with_scalar_should_fetch_using_index_match_second_part_of_the_composite_index(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "custom"}, {field: "age"}]) {
                        name: String 
                        custom: JSON 
                        age: Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(
                    json!({ "age": 30, "custom": { "numbers": [3, 4] }, "name": "John" }),
                ),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(
                    json!({ "age": 25, "custom": { "numbers": [3, 5] }, "name": "Islam" }),
                ),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(
                    json!({ "age": 30, "custom": { "numbers": [4, 6] }, "name": "Shahzad" }),
                ),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(
                    json!({ "age": 35, "custom": { "numbers": [5, 7] }, "name": "Keenan" }),
                ),
                common: Common {
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(
                    json!({ "age": 35, "custom": { "numbers": [1, 5, 8] }, "name": "Addo" }),
                ),
                common: Common {
                    capture: captures([("doc0_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(
                    json!({ "age": 40, "custom": { "numbers": [6, 9] }, "name": "Bruno" }),
                ),
                common: Common {
                    capture: captures([("doc0_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 35, "custom": { "numbers": [] }, "name": "Andy" })),
                common: Common {
                    capture: captures([("doc0_6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(
                    json!({ "age": null, "custom": { "numbers": [7, 10] }, "name": "Chris" }),
                ),
                common: Common {
                    capture: captures([("doc0_7", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                    User(filter: {age: {_eq: 40}}) {
                                        name
                                    }
                                }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": "Bruno" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                                    User(filter: {age: {_eq: 40}}) {
                                        name
                                    }
                                }
                "#
                .into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 0 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.json.composite")]
fn json_composite_index_json_with_scalar_with_eq_filter_should_fetch_using_index_unique_combination_non_unique_custom_val(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "custom"}, {field: "age"}]) {
                        name: String 
                        custom: JSON 
                        age: Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 30, "custom": { "val": 3 }, "name": "John" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 25, "custom": { "val": 3 }, "name": "Islam" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 25, "custom": { "val": 4 }, "name": "Shahzad" })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 35, "custom": { "val": 5 }, "name": "Keenan" })),
                common: Common {
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 35, "custom": { "val": 5 }, "name": "Addo" })),
                common: Common {
                    capture: captures([("doc0_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 40, "custom": { "val": 6 }, "name": "Bruno" })),
                common: Common {
                    capture: captures([("doc0_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 50, "custom": { "val": null }, "name": "Andy" })),
                common: Common {
                    capture: captures([("doc0_6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": null, "custom": { "val": 7 }, "name": "Chris" })),
                common: Common {
                    capture: captures([("doc0_7", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                    User(filter: {
                                        custom: {val: {_eq: 3}}, 
                                        age: {_eq: 25}
                                    }) {
                                        name
                                    }
                                }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": "Islam" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                                    User(filter: {
                                        custom: {val: {_eq: 3}}, 
                                        age: {_eq: 25}
                                    }) {
                                        name
                                    }
                                }
                "#
                .into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 1 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.json.composite")]
fn json_composite_index_json_with_scalar_with_eq_filter_should_fetch_using_index_unique_combination_non_unique_age(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "custom"}, {field: "age"}]) {
                        name: String 
                        custom: JSON 
                        age: Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 30, "custom": { "val": 3 }, "name": "John" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 25, "custom": { "val": 3 }, "name": "Islam" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 25, "custom": { "val": 4 }, "name": "Shahzad" })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 35, "custom": { "val": 5 }, "name": "Keenan" })),
                common: Common {
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 35, "custom": { "val": 5 }, "name": "Addo" })),
                common: Common {
                    capture: captures([("doc0_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 40, "custom": { "val": 6 }, "name": "Bruno" })),
                common: Common {
                    capture: captures([("doc0_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 50, "custom": { "val": null }, "name": "Andy" })),
                common: Common {
                    capture: captures([("doc0_6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": null, "custom": { "val": 7 }, "name": "Chris" })),
                common: Common {
                    capture: captures([("doc0_7", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                    User(filter: {
                                        custom: {val: {_eq: 3}}, 
                                        age: {_eq: 30}
                                    }) {
                                        name
                                    }
                                }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": "John" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                                    User(filter: {
                                        custom: {val: {_eq: 3}}, 
                                        age: {_eq: 30}
                                    }) {
                                        name
                                    }
                                }
                "#
                .into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 1 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.json.composite")]
fn json_composite_index_json_with_scalar_with_eq_filter_should_fetch_using_index_match_first_part_of_the_composite_index(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "custom"}, {field: "age"}]) {
                        name: String 
                        custom: JSON 
                        age: Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 30, "custom": { "val": 3 }, "name": "John" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 25, "custom": { "val": 3 }, "name": "Islam" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 25, "custom": { "val": 4 }, "name": "Shahzad" })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 35, "custom": { "val": 5 }, "name": "Keenan" })),
                common: Common {
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 35, "custom": { "val": 5 }, "name": "Addo" })),
                common: Common {
                    capture: captures([("doc0_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 40, "custom": { "val": 6 }, "name": "Bruno" })),
                common: Common {
                    capture: captures([("doc0_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 50, "custom": { "val": null }, "name": "Andy" })),
                common: Common {
                    capture: captures([("doc0_6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": null, "custom": { "val": 7 }, "name": "Chris" })),
                common: Common {
                    capture: captures([("doc0_7", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                    User(filter: {custom: {val: {_eq: 3}}}) {
                                        name
                                    }
                                }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": "Islam" }, { "name": "John" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                                    User(filter: {custom: {val: {_eq: 3}}}) {
                                        name
                                    }
                                }
                "#
                .into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 2 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.json.composite")]
fn json_composite_index_json_with_scalar_with_eq_filter_should_fetch_using_index_non_unique_combination(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "custom"}, {field: "age"}]) {
                        name: String 
                        custom: JSON 
                        age: Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 30, "custom": { "val": 3 }, "name": "John" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 25, "custom": { "val": 3 }, "name": "Islam" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 25, "custom": { "val": 4 }, "name": "Shahzad" })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 35, "custom": { "val": 5 }, "name": "Keenan" })),
                common: Common {
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 35, "custom": { "val": 5 }, "name": "Addo" })),
                common: Common {
                    capture: captures([("doc0_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 40, "custom": { "val": 6 }, "name": "Bruno" })),
                common: Common {
                    capture: captures([("doc0_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 50, "custom": { "val": null }, "name": "Andy" })),
                common: Common {
                    capture: captures([("doc0_6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": null, "custom": { "val": 7 }, "name": "Chris" })),
                common: Common {
                    capture: captures([("doc0_7", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                    User(filter: {
                                        custom: {val: {_eq: 5}},
                                        age: {_eq: 35},
                                    }) {
                                        name
                                    }
                                }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": "Keenan" }, { "name": "Addo" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                                    User(filter: {
                                        custom: {val: {_eq: 5}},
                                        age: {_eq: 35},
                                    }) {
                                        name
                                    }
                                }
                "#
                .into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 2 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.json.composite")]
fn json_composite_index_json_with_scalar_with_eq_filter_should_fetch_using_index_match_second_part_of_the_composite_index(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "custom"}, {field: "age"}]) {
                        name: String 
                        custom: JSON 
                        age: Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 30, "custom": { "val": 3 }, "name": "John" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 25, "custom": { "val": 3 }, "name": "Islam" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 25, "custom": { "val": 4 }, "name": "Shahzad" })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 35, "custom": { "val": 5 }, "name": "Keenan" })),
                common: Common {
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 35, "custom": { "val": 5 }, "name": "Addo" })),
                common: Common {
                    capture: captures([("doc0_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 40, "custom": { "val": 6 }, "name": "Bruno" })),
                common: Common {
                    capture: captures([("doc0_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 50, "custom": { "val": null }, "name": "Andy" })),
                common: Common {
                    capture: captures([("doc0_6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": null, "custom": { "val": 7 }, "name": "Chris" })),
                common: Common {
                    capture: captures([("doc0_7", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                    User(filter: {age: {_eq: 40}}) {
                                        name
                                    }
                                }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": "Bruno" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                                    User(filter: {age: {_eq: 40}}) {
                                        name
                                    }
                                }
                "#
                .into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 0 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.json.composite")]
fn json_composite_index_json_with_scalar_with_other_filters_should_fetch_using_index2_with_le_and_gt_filters(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "age"}, {field: "custom"}]) {
                        name: String 
                        custom: JSON 
                        age: Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 30, "custom": { "val": 3 }, "name": "John" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 25, "custom": { "val": 3 }, "name": "Islam" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 25, "custom": { "val": 4 }, "name": "Shahzad" })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 35, "custom": { "val": 5 }, "name": "Keenan" })),
                common: Common {
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 35, "custom": { "val": 6 }, "name": "Addo" })),
                common: Common {
                    capture: captures([("doc0_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 40, "custom": { "val": 6 }, "name": "Bruno" })),
                common: Common {
                    capture: captures([("doc0_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 50, "custom": { "val": null }, "name": "Andy" })),
                common: Common {
                    capture: captures([("doc0_6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": null, "custom": { "val": 7 }, "name": "Chris" })),
                common: Common {
                    capture: captures([("doc0_7", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                    User(filter: {
                                        age: {_leq: 35},
                                        custom: {val: {_gt: 4}}
                                    }) {
                                        name
                                    }
                                }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": "Keenan" }, { "name": "Addo" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                                    User(filter: {
                                        age: {_leq: 35},
                                        custom: {val: {_gt: 4}}
                                    }) {
                                        name
                                    }
                                }
                "#
                .into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 6 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.json.composite")]
fn json_composite_index_json_with_scalar_with_other_filters_should_fetch_using_index2_with_lt_and_eq_filters(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "age"}, {field: "custom"}]) {
                        name: String 
                        custom: JSON 
                        age: Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 30, "custom": { "val": 3 }, "name": "John" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 25, "custom": { "val": 3 }, "name": "Islam" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 25, "custom": { "val": 4 }, "name": "Shahzad" })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 35, "custom": { "val": 5 }, "name": "Keenan" })),
                common: Common {
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 35, "custom": { "val": 6 }, "name": "Addo" })),
                common: Common {
                    capture: captures([("doc0_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 40, "custom": { "val": 6 }, "name": "Bruno" })),
                common: Common {
                    capture: captures([("doc0_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 50, "custom": { "val": null }, "name": "Andy" })),
                common: Common {
                    capture: captures([("doc0_6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": null, "custom": { "val": 7 }, "name": "Chris" })),
                common: Common {
                    capture: captures([("doc0_7", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                    User(filter: {
                                        age: {_lt: 100},
                                        custom: {val: {_eq: null}}
                                    }) {
                                        name
                                    }
                                }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": "Andy" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                                    User(filter: {
                                        age: {_lt: 100},
                                        custom: {val: {_eq: null}}
                                    }) {
                                        name
                                    }
                                }
                "#
                .into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 8 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.json.composite")]
fn json_composite_index_json_with_scalar_with_other_filters_should_fetch_using_index2_with_ne_and_ge_filters(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "age"}, {field: "custom"}]) {
                        name: String 
                        custom: JSON 
                        age: Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 30, "custom": { "val": 3 }, "name": "John" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 25, "custom": { "val": 3 }, "name": "Islam" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 25, "custom": { "val": 4 }, "name": "Shahzad" })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 35, "custom": { "val": 5 }, "name": "Keenan" })),
                common: Common {
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 35, "custom": { "val": 6 }, "name": "Addo" })),
                common: Common {
                    capture: captures([("doc0_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 40, "custom": { "val": 6 }, "name": "Bruno" })),
                common: Common {
                    capture: captures([("doc0_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 50, "custom": { "val": null }, "name": "Andy" })),
                common: Common {
                    capture: captures([("doc0_6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": null, "custom": { "val": 7 }, "name": "Chris" })),
                common: Common {
                    capture: captures([("doc0_7", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                    User(filter: {
                                        _and: [{ age: {_neq: 35} }, { age: {_neq: 40} }],
                                        custom: {val: {_geq: 5}} 
                                    }) {
                                        name
                                    }
                                }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": "Chris" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                                    User(filter: {
                                        _and: [{ age: {_neq: 35} }, { age: {_neq: 40} }],
                                        custom: {val: {_geq: 5}} 
                                    }) {
                                        name
                                    }
                                }
                "#
                .into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 8 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.json.composite")]
fn json_composite_index_scalar_with_json_should_fetch_using_index_unique_combination_non_unique_custom_val(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "age"}, {field: "custom"}]) {
                        name: String 
                        custom: JSON 
                        age: Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 30, "custom": { "val": 3 }, "name": "John" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 25, "custom": { "val": 3 }, "name": "Islam" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 25, "custom": { "val": 4 }, "name": "Shahzad" })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 35, "custom": { "val": 5 }, "name": "Keenan" })),
                common: Common {
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 35, "custom": { "val": 5 }, "name": "Addo" })),
                common: Common {
                    capture: captures([("doc0_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 40, "custom": { "val": 6 }, "name": "Bruno" })),
                common: Common {
                    capture: captures([("doc0_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 50, "custom": { "val": null }, "name": "Andy" })),
                common: Common {
                    capture: captures([("doc0_6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": null, "custom": { "val": 7 }, "name": "Chris" })),
                common: Common {
                    capture: captures([("doc0_7", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                    User(filter: {
                                        age: {_eq: 25}, 
                                        custom: {val: {_eq: 3}}
                                    }) {
                                        name
                                    }
                                }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": "Islam" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                                    User(filter: {
                                        age: {_eq: 25}, 
                                        custom: {val: {_eq: 3}}
                                    }) {
                                        name
                                    }
                                }
                "#
                .into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 1 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.json.composite")]
fn json_composite_index_scalar_with_json_should_fetch_using_index_unique_combination_non_unique_age(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "age"}, {field: "custom"}]) {
                        name: String 
                        custom: JSON 
                        age: Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 30, "custom": { "val": 3 }, "name": "John" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 25, "custom": { "val": 3 }, "name": "Islam" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 25, "custom": { "val": 4 }, "name": "Shahzad" })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 35, "custom": { "val": 5 }, "name": "Keenan" })),
                common: Common {
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 35, "custom": { "val": 5 }, "name": "Addo" })),
                common: Common {
                    capture: captures([("doc0_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 40, "custom": { "val": 6 }, "name": "Bruno" })),
                common: Common {
                    capture: captures([("doc0_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 50, "custom": { "val": null }, "name": "Andy" })),
                common: Common {
                    capture: captures([("doc0_6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": null, "custom": { "val": 7 }, "name": "Chris" })),
                common: Common {
                    capture: captures([("doc0_7", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                    User(filter: {
                                        age: {_eq: 30}, 
                                        custom: {val: {_eq: 3}}
                                    }) {
                                        name
                                    }
                                }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": "John" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                                    User(filter: {
                                        age: {_eq: 30}, 
                                        custom: {val: {_eq: 3}}
                                    }) {
                                        name
                                    }
                                }
                "#
                .into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 1 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.json.composite")]
fn json_composite_index_scalar_with_json_should_fetch_using_index_match_first_part_of_the_composite_index(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "age"}, {field: "custom"}]) {
                        name: String 
                        custom: JSON 
                        age: Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 30, "custom": { "val": 3 }, "name": "John" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 25, "custom": { "val": 3 }, "name": "Islam" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 25, "custom": { "val": 4 }, "name": "Shahzad" })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 35, "custom": { "val": 5 }, "name": "Keenan" })),
                common: Common {
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 35, "custom": { "val": 5 }, "name": "Addo" })),
                common: Common {
                    capture: captures([("doc0_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 40, "custom": { "val": 6 }, "name": "Bruno" })),
                common: Common {
                    capture: captures([("doc0_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 50, "custom": { "val": null }, "name": "Andy" })),
                common: Common {
                    capture: captures([("doc0_6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": null, "custom": { "val": 7 }, "name": "Chris" })),
                common: Common {
                    capture: captures([("doc0_7", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                    User(filter: {age: {_eq: 25}}) {
                                        name
                                    }
                                }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": "Islam" }, { "name": "Shahzad" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                                    User(filter: {age: {_eq: 25}}) {
                                        name
                                    }
                                }
                "#
                .into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 2 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.json.composite")]
fn json_composite_index_scalar_with_json_should_fetch_using_index_non_unique_combination() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "age"}, {field: "custom"}]) {
                        name: String 
                        custom: JSON 
                        age: Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 30, "custom": { "val": 3 }, "name": "John" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 25, "custom": { "val": 3 }, "name": "Islam" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 25, "custom": { "val": 4 }, "name": "Shahzad" })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 35, "custom": { "val": 5 }, "name": "Keenan" })),
                common: Common {
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 35, "custom": { "val": 5 }, "name": "Addo" })),
                common: Common {
                    capture: captures([("doc0_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 40, "custom": { "val": 6 }, "name": "Bruno" })),
                common: Common {
                    capture: captures([("doc0_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 50, "custom": { "val": null }, "name": "Andy" })),
                common: Common {
                    capture: captures([("doc0_6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": null, "custom": { "val": 7 }, "name": "Chris" })),
                common: Common {
                    capture: captures([("doc0_7", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                    User(filter: {
                                        age: {_eq: 35}, 
                                        custom: {val: {_eq: 5}}
                                    }) {
                                        name
                                    }
                                }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": "Keenan" }, { "name": "Addo" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                                    User(filter: {
                                        age: {_eq: 35}, 
                                        custom: {val: {_eq: 5}}
                                    }) {
                                        name
                                    }
                                }
                "#
                .into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 2 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.json.composite")]
fn json_composite_index_scalar_with_json_should_fetch_using_index_match_second_part_of_the_composite_index(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "age"}, {field: "custom"}]) {
                        name: String 
                        custom: JSON 
                        age: Int
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 30, "custom": { "val": 3 }, "name": "John" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 25, "custom": { "val": 3 }, "name": "Islam" })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 25, "custom": { "val": 4 }, "name": "Shahzad" })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 35, "custom": { "val": 5 }, "name": "Keenan" })),
                common: Common {
                    capture: captures([("doc0_3", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 35, "custom": { "val": 5 }, "name": "Addo" })),
                common: Common {
                    capture: captures([("doc0_4", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 40, "custom": { "val": 6 }, "name": "Bruno" })),
                common: Common {
                    capture: captures([("doc0_5", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 50, "custom": { "val": null }, "name": "Andy" })),
                common: Common {
                    capture: captures([("doc0_6", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": null, "custom": { "val": 7 }, "name": "Chris" })),
                common: Common {
                    capture: captures([("doc0_7", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                    User(filter: {custom: {val: {_eq: 6}}}) {
                                        name
                                    }
                                }
                "#
                .into(),
                expected_data: json!({ "User": [{ "name": "Bruno" }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                                    User(filter: {custom: {val: {_eq: 6}}}) {
                                        name
                                    }
                                }
                "#
                .into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 0 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
