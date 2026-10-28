//! Generated from the conformance scenarios of tests/integration/index/array_unique_composite_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request, UpdateDoc};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.index.unique.array")]
fn array_unique_composite_index_if_doc_is_added_that_violates_uniqueness_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(unique: true, includes: [{field: "nfts1"}, {field: "nfts2"}]) {
                        name: String 
                        nfts1: [Int!] 
                        nfts2: [Int!] 
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "nfts1": [1, 2], "nfts2": [1, 3] })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Shahzad", "nfts1": [1, 2], "nfts2": [2, 4, 3] })),
                expected_error: Some(
                    "can not index a doc's field(s) that violates unique index.".into(),
                ),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Shahzad", "nfts1": [5, 6, 2], "nfts2": [1, 3] })),
                expected_error: Some(
                    "can not index a doc's field(s) that violates unique index.".into(),
                ),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.unique.array")]
fn array_unique_composite_index_if_doc_is_updated_that_violates_uniqueness_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(unique: true, includes: [{field: "nfts1"}, {field: "nfts2"}]) {
                        name: String 
                        nfts1: [Int!] 
                        nfts2: [Int!] 
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "nfts1": [1, 2], "nfts2": [1, 3] })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Shahzad", "nfts1": [1, 2], "nfts2": [2, 4, 5, 6] })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "User".into(),
                doc_id: Text::reference("doc0_1"),
                doc_map: json!({ "name": "Shahzad", "nfts1": [1], "nfts2": [2, 5, 3] }),
                expected_error: Some(
                    "can not index a doc's field(s) that violates unique index.".into(),
                ),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.unique.array")]
fn array_unique_composite_index_if_docs_have_nil_values_succeed() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(unique: true, includes: [{field: "nfts1"}, {field: "nfts2"}]) {
                        name: String 
                        nfts1: [Int] 
                        nfts2: [Int] 
                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "nfts1": [1, null], "nfts2": [null, 1, 3, null] })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({
                    "name": "Shahzad",
                    "nfts1": [1, null, 2],
                    "nfts2": [2, 4, null, 5, 6, null],
                })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                            User(filter: {nfts1: {_any: {_eq: null}}, nfts2: {_any: {_eq: null}}}) {
                                                name
                                            }
                                        }
                "#.into(),
                expected_data: json!({ "User": [{ "name": "John" }, { "name": "Shahzad" }] }),
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
#[mantra_macros::req_test("go.index.unique.array")]
fn array_unique_composite_index_with_unique_combinations_succeed() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User @index(unique: true, includes: [{field: "nfts1"}, {field: "nfts2"}]) {
                        name: String 
                        nfts1: [Int!] 
                        nfts2: [Int!] 
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John", "nfts1": [1, 2], "nfts2": [1, 3] })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Shahzad", "nfts1": [1, 2], "nfts2": [2, 4] })),
                common: Common {
                    capture: captures([("doc0_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Keenan", "nfts1": [3, 4], "nfts2": [1, 3] })),
                common: Common {
                    capture: captures([("doc0_2", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                            User(filter: {nfts1: {_any: {_eq: 2}}, nfts2: {_any: {_eq: 3}}}) {
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
                            User(filter: {nfts1: {_any: {_eq: 2}}, nfts2: {_any: {_eq: 3}}}) {
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
