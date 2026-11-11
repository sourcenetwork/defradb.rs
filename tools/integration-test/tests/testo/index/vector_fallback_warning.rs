//! Generated from the conformance scenarios of tests/integration/index/vector_fallback_warning_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.index.vector.fallback-warning")]
fn vector_index_warning_multiple_similarity_fields_reports_warning() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                                    name: String
                                    age: Int
                                    vector: [Float32!] @index(vector: {dimensions: 3, hnsw: {metric: COSINE}})
                                }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 10, "name": "x", "vector": [1.0, 0.0, 0.0] })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 20, "name": "y", "vector": [0.0, 1.0, 0.0] })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 30, "name": "xy", "vector": [0.9, 0.4, 0.0] })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 40, "name": "z", "vector": [0.0, 0.0, 1.0] })),
                common: Common { capture: captures([("doc0_3", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User(order: {_alias: {sim: DESC}}, limit: 2){
                                            name
                                            sim: SIMILARITY(vector: {vector: [1, 0, 0]})
                                            other: SIMILARITY(vector: {vector: [0, 1, 0]})
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "User": [
                        {
                            "name": "x",
                            "other": m::similarity_score("COSINE", json!([1, 0, 0]), json!([0, 1, 0])),
                            "sim": m::similarity_score("COSINE", json!([1, 0, 0]), json!([1, 0, 0])),
                        },
                        {
                            "name": "xy",
                            "other": m::similarity_score("COSINE", json!([0.9, 0.4, 0]), json!([0, 1, 0])),
                            "sim": m::similarity_score("COSINE", json!([0.9, 0.4, 0]), json!([1, 0, 0])),
                        },
                    ],
                }),
                expected_extensions: json!(m::object(json!({
                    "warnings": [
                        m::object(json!({
                            "code": "VECTOR_INDEX_UNUSED",
                            "detail": { "field": "vector", "reason": "multipleSimilarityFields" },
                            "message": m::nonempty_string(),
                        })),
                    ],
                }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.vector.fallback-warning")]
fn vector_index_warning_no_limit_reports_warning() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                                    name: String
                                    age: Int
                                    vector: [Float32!] @index(vector: {dimensions: 3, hnsw: {metric: COSINE}})
                                }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 10, "name": "x", "vector": [1.0, 0.0, 0.0] })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 20, "name": "y", "vector": [0.0, 1.0, 0.0] })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 30, "name": "xy", "vector": [0.9, 0.4, 0.0] })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 40, "name": "z", "vector": [0.0, 0.0, 1.0] })),
                common: Common { capture: captures([("doc0_3", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User(order: {_alias: {sim: DESC}}){
                                            name
                                            sim: SIMILARITY(vector: {vector: [1, 0, 0]})
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "User": [
                        {
                            "name": "x",
                            "sim": m::similarity_score("COSINE", json!([1, 0, 0]), json!([1, 0, 0])),
                        },
                        {
                            "name": "xy",
                            "sim": m::similarity_score("COSINE", json!([0.9, 0.4, 0]), json!([1, 0, 0])),
                        },
                        {
                            "name": "y",
                            "sim": m::similarity_score("COSINE", json!([0, 1, 0]), json!([1, 0, 0])),
                        },
                        {
                            "name": "z",
                            "sim": m::similarity_score("COSINE", json!([0, 0, 1]), json!([1, 0, 0])),
                        },
                    ],
                }),
                expected_extensions: json!(m::object(json!({
                    "warnings": [
                        m::object(json!({
                            "code": "VECTOR_INDEX_UNUSED",
                            "detail": { "field": "vector", "reason": "noLimit" },
                            "message": m::nonempty_string(),
                        })),
                    ],
                }))),
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
#[mantra_macros::req_test("go.index.vector.fallback-warning")]
fn vector_index_warning_no_vector_index_on_field_reports_no_warning() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                                        name: String
                                        vector: [Float32!]
                                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "x", "vector": [1, 0, 0] })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "y", "vector": [0, 1, 0] })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User(order: {_alias: {sim: ASC}}, limit: 1){
                                            name
                                            sim: SIMILARITY(vector: {vector: [1, 0, 0]})
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "User": [
                        {
                            "name": "y",
                            "sim": m::similarity_score("COSINE", json!([0, 1, 0]), json!([1, 0, 0])),
                        },
                    ],
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.vector.fallback-warning")]
fn vector_index_warning_ordered_ascending_reports_warning() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                                    name: String
                                    age: Int
                                    vector: [Float32!] @index(vector: {dimensions: 3, hnsw: {metric: COSINE}})
                                }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 10, "name": "x", "vector": [1.0, 0.0, 0.0] })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 20, "name": "y", "vector": [0.0, 1.0, 0.0] })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 30, "name": "xy", "vector": [0.9, 0.4, 0.0] })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 40, "name": "z", "vector": [0.0, 0.0, 1.0] })),
                common: Common { capture: captures([("doc0_3", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User(order: {_alias: {sim: ASC}}, limit: 2){
                                            name
                                            sim: SIMILARITY(vector: {vector: [1, 0, 0]})
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "User": [
                        {
                            "name": "z",
                            "sim": m::similarity_score("COSINE", json!([0, 0, 1]), json!([1, 0, 0])),
                        },
                        {
                            "name": "y",
                            "sim": m::similarity_score("COSINE", json!([0, 1, 0]), json!([1, 0, 0])),
                        },
                    ],
                }),
                expected_extensions: json!(m::object(json!({
                    "warnings": [
                        m::object(json!({
                            "code": "VECTOR_INDEX_UNUSED",
                            "detail": { "field": "vector", "reason": "notOrderedBySimilarityDesc" },
                            "message": m::nonempty_string(),
                        })),
                    ],
                }))),
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
#[mantra_macros::req_test("go.index.vector.fallback-warning")]
fn vector_index_warning_query_uses_index_reports_no_warning() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                                    name: String
                                    age: Int
                                    vector: [Float32!] @index(vector: {dimensions: 3, hnsw: {metric: COSINE}})
                                }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 10, "name": "x", "vector": [1.0, 0.0, 0.0] })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 20, "name": "y", "vector": [0.0, 1.0, 0.0] })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 30, "name": "xy", "vector": [0.9, 0.4, 0.0] })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 40, "name": "z", "vector": [0.0, 0.0, 1.0] })),
                common: Common { capture: captures([("doc0_3", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User(order: {_alias: {sim: DESC}}, limit: 2){
                                            name
                                            sim: SIMILARITY(vector: {vector: [1, 0, 0]})
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "User": [
                        {
                            "name": "x",
                            "sim": m::similarity_score("COSINE", json!([1, 0, 0]), json!([1, 0, 0])),
                        },
                        {
                            "name": "xy",
                            "sim": m::similarity_score("COSINE", json!([0.9, 0.4, 0]), json!([1, 0, 0])),
                        },
                    ],
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.vector.fallback-warning")]
fn vector_index_warning_selective_filter_still_returns_matching_docs() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                                    name: String
                                    age: Int
                                    vector: [Float32!] @index(vector: {dimensions: 3, hnsw: {metric: COSINE}})
                                }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 10, "name": "x", "vector": [1.0, 0.0, 0.0] })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 20, "name": "y", "vector": [0.0, 1.0, 0.0] })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 30, "name": "xy", "vector": [0.9, 0.4, 0.0] })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 40, "name": "z", "vector": [0.0, 0.0, 1.0] })),
                common: Common { capture: captures([("doc0_3", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User(filter: {age: {_gt: 35}}, order: {_alias: {sim: DESC}}, limit: 1){
                                            name
                                            sim: SIMILARITY(vector: {vector: [1, 0, 0]})
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "User": [
                        {
                            "name": "z",
                            "sim": m::similarity_score("COSINE", json!([0, 0, 1]), json!([1, 0, 0])),
                        },
                    ],
                }),
                expected_extensions: json!(m::object(json!({
                    "warnings": [
                        m::object(json!({
                            "code": "VECTOR_INDEX_UNUSED",
                            "detail": { "field": "vector", "reason": "filter" },
                            "message": m::nonempty_string(),
                        })),
                    ],
                }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.index.vector.fallback-warning")]
fn vector_index_warning_with_filter_reports_warning() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                                    name: String
                                    age: Int
                                    vector: [Float32!] @index(vector: {dimensions: 3, hnsw: {metric: COSINE}})
                                }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 10, "name": "x", "vector": [1.0, 0.0, 0.0] })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 20, "name": "y", "vector": [0.0, 1.0, 0.0] })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 30, "name": "xy", "vector": [0.9, 0.4, 0.0] })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "age": 40, "name": "z", "vector": [0.0, 0.0, 1.0] })),
                common: Common { capture: captures([("doc0_3", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User(filter: {age: {_gt: 15}}, order: {_alias: {sim: DESC}}, limit: 2){
                                            name
                                            sim: SIMILARITY(vector: {vector: [1, 0, 0]})
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "User": [
                        {
                            "name": "xy",
                            "sim": m::similarity_score("COSINE", json!([0.9, 0.4, 0]), json!([1, 0, 0])),
                        },
                        {
                            "name": "y",
                            "sim": m::similarity_score("COSINE", json!([0, 1, 0]), json!([1, 0, 0])),
                        },
                    ],
                }),
                expected_extensions: json!(m::object(json!({
                    "warnings": [
                        m::object(json!({
                            "code": "VECTOR_INDEX_UNUSED",
                            "detail": { "field": "vector", "reason": "filter" },
                            "message": m::nonempty_string(),
                        })),
                    ],
                }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
