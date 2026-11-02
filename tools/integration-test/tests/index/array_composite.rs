//! Generated from the conformance scenarios of tests/integration/index/array_composite_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, DeleteDoc, Request, UpdateDoc};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.index.array.composite")]
fn array_composite_index_delete_with2_consecutive_array_fields_succeed() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "name"}, {field: "numbers"}, {field: "hobbies"}]) {
                        name: String 
                        numbers: [Int!] 
                        hobbies: [String!] 
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "numbers": [0, 30, 20], "hobbies": ["sports", "books"] })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({
                    "name": "Shahzad",
                    "numbers": [30, 40, 30, 50],
                    "hobbies": ["sports", "books", "sports", "movies"],
                })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            DeleteDoc {
                collection: "User".into(),
                doc_id: Text::reference("doc0_1"),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User(filter: {numbers: {_any: {_eq: 30}}}) {
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
                    query {
                                        User(filter: {numbers: {_any: {_gt: 0}}, hobbies: {_any: {_eq: "sports"}}}) {
                                            name
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "User": [{ "name": "John" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.array.composite")]
fn array_composite_index_update_with2_array_fields_succeed() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "name"}, {field: "numbers"}, {field: "hobbies"}]) {
                        name: String 
                        numbers: [Int!] 
                        hobbies: [String!] 
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({
                    "name": "John",
                    "numbers": [0, 30, 20, 40],
                    "hobbies": ["sports", "books"],
                })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({
                    "name": "Shahzad",
                    "numbers": [30, 40, 30],
                    "hobbies": ["sports", "books"],
                })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "User".into(),
                doc_id: Text::reference("doc0_1"),
                doc_map: json!({
                    "name": "Shahzad",
                    "numbers": [40, 50, 50],
                    "hobbies": ["books", "movies", "books", "movies"],
                }),
                expected_data: json!({}),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User(filter: {numbers: {_any: {_eq: 30}}}) {
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
                    query {
                                        User(filter: {numbers: {_any: {_eq: 40}}}) {
                                            name
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "User": [{ "name": "John" }, { "name": "Shahzad" }] }),
                common: Common { unordered_documents: vec!["/data/User".into()], ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User(filter: {numbers: {_any: {_eq: 50}}}) {
                                            name
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "User": [{ "name": "Shahzad" }] }),
                common: Common { unordered_documents: vec!["/data/User".into()], ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User(filter: {numbers: {_any: {_gt: 0}}, hobbies: {_any: {_eq: "sports"}}}) {
                                            name
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "User": [{ "name": "John" }] }),
                common: Common { unordered_documents: vec!["/data/User".into()], ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User(filter: {numbers: {_any: {_gt: 0}}, hobbies: {_any: {_eq: "books"}}}) {
                                            name
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "User": [{ "name": "John" }, { "name": "Shahzad" }] }),
                common: Common { unordered_documents: vec!["/data/User".into()], ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User(filter: {numbers: {_any: {_gt: 0}}, hobbies: {_any: {_eq: "movies"}}}) {
                                            name
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "User": [{ "name": "Shahzad" }] }),
                common: Common { unordered_documents: vec!["/data/User".into()], ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.array.composite")]
fn array_composite_index_with_any_none_all_succeed() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "numbers1"}, {field: "numbers2"}, {field: "numbers3"}]) {
                        name: String 
                        numbers1: [Int!] 
                        numbers2: [Int!] 
                        numbers3: [Int!] 
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({
                    "name": "John",
                    "numbers1": [1, 2, 3],
                    "numbers2": [10, 20, 30],
                    "numbers3": [100, 200, 300],
                })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({
                    "name": "Shahzad",
                    "numbers1": [2, 3, 4],
                    "numbers2": [20, 30, 40],
                    "numbers3": [200, 300, 400],
                })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({
                    "name": "Keenan",
                    "numbers1": [0, 1],
                    "numbers2": [90],
                    "numbers3": [900],
                })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({
                    "name": "Islam",
                    "numbers1": [6, 7, 8],
                    "numbers2": [10, 70, 80],
                    "numbers3": [100, 700, 800],
                })),
                common: Common { capture: captures([("doc0_3", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({
                    "name": "Fred",
                    "numbers1": [1, 4, 5, 8],
                    "numbers2": [60, 80],
                    "numbers3": [600, 800],
                })),
                common: Common { capture: captures([("doc0_4", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(filter: {
                                numbers1: {_all: {_gt: 0}}, 
                                numbers2: {_none: {_eq: 40}}, 
                                numbers3: {_any: {_leq: 200}}
                            }) {
                                name
                            }
                        }
                "#.into(),
                expected_data: json!({ "User": [{ "name": "John" }, { "name": "Islam" }] }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.array.composite")]
fn array_composite_index_with_filter_on_indexed_array_using_all_should_use_index() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "name"}, {field: "numbers"}, {field: "age"}]) {
                        name: String 
                        numbers: [Int!] 
                        age: Int
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "numbers": [0, 30, 20], "age": 30 })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Shahzad", "numbers": [30, 40], "age": 30 })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Shahzad", "numbers": [50], "age": 60 })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Shahzad", "numbers": [1, 2], "age": 30 })),
                common: Common { capture: captures([("doc0_3", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(filter: {name: {_eq: "Shahzad"}, numbers: {_all: {_gt: 1}}, age: {_eq: 30}}) {
                                _docID
                                numbers
                            }
                        }
                "#.into(),
                expected_data: json!({ "User": [{ "_docID": m::r("doc0_1"), "numbers": [30, 40] }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            User(filter: {name: {_eq: "Shahzad"}, numbers: {_all: {_gt: 1}}, age: {_eq: 30}}) {
                                _docID
                                numbers
                            }
                        }
                "#.into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 5 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.array.composite")]
fn array_composite_index_with_filter_on_indexed_array_using_any_should_use_index() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "name"}, {field: "numbers"}, {field: "age"}]) {
                        name: String 
                        numbers: [Int!] 
                        age: Int
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "numbers": [0, 30, 20], "age": 30 })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Shahzad", "numbers": [30, 40, 50, 30], "age": 30 })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Shahzad", "numbers": [40, 50, 30], "age": 60 })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Shahzad", "numbers": [1, 2, 3], "age": 30 })),
                common: Common { capture: captures([("doc0_3", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(filter: {name: {_eq: "Shahzad"}, numbers: {_any: {_eq: 30}}, age: {_eq: 30}}) {
                                _docID
                                numbers
                            }
                        }
                "#.into(),
                expected_data: json!({ "User": [{ "_docID": m::r("doc0_1"), "numbers": [30, 40, 50, 30] }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            User(filter: {name: {_eq: "Shahzad"}, numbers: {_any: {_eq: 30}}, age: {_eq: 30}}) {
                                _docID
                                numbers
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
#[mantra_macros::req_test("go.index.array.composite")]
fn array_composite_index_with_filter_on_indexed_array_using_none_should_use_index() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "name"}, {field: "numbers"}, {field: "age"}]) {
                        name: String 
                        numbers: [Int!] 
                        age: Int
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "numbers": [0, 30, 20], "age": 30 })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Shahzad", "numbers": [30, 40], "age": 30 })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Shahzad", "numbers": [50], "age": 60 })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Shahzad", "numbers": [2, 3], "age": 30 })),
                common: Common { capture: captures([("doc0_3", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(filter: {name: {_eq: "Shahzad"}, numbers: {_none: {_eq: 3}}, age: {_eq: 30}}) {
                                _docID
                                numbers
                            }
                        }
                "#.into(),
                expected_data: json!({ "User": [{ "_docID": m::r("doc0_1"), "numbers": [30, 40] }] }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            User(filter: {name: {_eq: "Shahzad"}, numbers: {_none: {_eq: 3}}, age: {_eq: 30}}) {
                                _docID
                                numbers
                            }
                        }
                "#.into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 5 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.array.composite")]
fn array_composite_index_with2_consecutive_array_fields_succeed() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "name"}, {field: "numbers"}, {field: "hobbies"}, {field: "age"}]) {
                        name: String 
                        numbers: [Int!] 
                        hobbies: [String!] 
                        age: Int
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({
                    "name": "John",
                    "numbers": [0, 30, 20],
                    "hobbies": ["sports", "books"],
                    "age": 30,
                })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({
                    "name": "Shahzad",
                    "numbers": [30, 40],
                    "hobbies": ["sports", "books"],
                    "age": 30,
                })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({
                    "name": "Shahzad",
                    "numbers": [50],
                    "hobbies": ["books", "movies"],
                    "age": 60,
                })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({
                    "name": "Shahzad",
                    "numbers": [2, 3],
                    "hobbies": ["sports", "movies", "books"],
                    "age": 30,
                })),
                common: Common { capture: captures([("doc0_3", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(filter: {name: {_eq: "Shahzad"}, numbers: {_any: {_eq: 30}}, hobbies: {_any: {_eq: "sports"}} age: {_eq: 30}}) {
                                _docID
                                numbers
                                hobbies
                            }
                        }
                "#.into(),
                expected_data: json!({
                    "User": [
                        {
                            "_docID": m::r("doc0_1"),
                            "hobbies": ["sports", "books"],
                            "numbers": [30, 40],
                        },
                    ],
                }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            User(filter: {name: {_eq: "Shahzad"}, numbers: {_any: {_eq: 30}}, hobbies: {_any: {_eq: "sports"}} age: {_eq: 30}}) {
                                _docID
                                numbers
                                hobbies
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
#[mantra_macros::req_test("go.index.array.composite")]
fn array_composite_index_with2_separate_array_fields_succeed() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(includes: [{field: "numbers"}, {field: "name"}, {field: "age"}, {field: "hobbies"}]) {
                        name: String 
                        numbers: [Int!] 
                        hobbies: [String!] 
                        age: Int
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({
                    "name": "John",
                    "numbers": [0, 30, 20],
                    "hobbies": ["sports", "books"],
                    "age": 30,
                })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({
                    "name": "Shahzad",
                    "numbers": [30, 40],
                    "hobbies": ["sports", "books"],
                    "age": 30,
                })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({
                    "name": "Shahzad",
                    "numbers": [50],
                    "hobbies": ["books", "movies"],
                    "age": 60,
                })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({
                    "name": "Shahzad",
                    "numbers": [2, 3],
                    "hobbies": ["sports", "movies", "books"],
                    "age": 30,
                })),
                common: Common { capture: captures([("doc0_3", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(filter: {name: {_eq: "Shahzad"}, numbers: {_any: {_eq: 30}}, hobbies: {_any: {_eq: "sports"}} age: {_eq: 30}}) {
                                _docID
                                numbers
                                hobbies
                            }
                        }
                "#.into(),
                expected_data: json!({
                    "User": [
                        {
                            "_docID": m::r("doc0_1"),
                            "hobbies": ["sports", "books"],
                            "numbers": [30, 40],
                        },
                    ],
                }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute)  {
                            User(filter: {name: {_eq: "Shahzad"}, numbers: {_any: {_eq: 30}}, hobbies: {_any: {_eq: "sports"}} age: {_eq: 30}}) {
                                _docID
                                numbers
                                hobbies
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
