//! Generated from the conformance scenarios of tests/integration/query/simple/with_similarity_vector_index_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, DeleteDoc, Request, UpdateDoc};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.query.simple.similarity.vector-index-fallback")]
fn query_simple_with_similarity_on_vector_index_ascending_order_full_scans() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                                        name: String
                                        vector: [Float32!] @index(vector: {dimensions: 3, hnsw: {metric: COSINE}})
                                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "near", "vector": [1.0, 0.0, 0.0] })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "mid", "vector": [0.8, 0.6, 0.0] })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "far", "vector": [0.1, 1.0, 0.0] })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "opp", "vector": [-1.0, 0.2, 0.0] })),
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
                            "name": "opp",
                            "sim": m::similarity_score("COSINE", json!([-1, 0.2, 0]), json!([1, 0, 0])),
                        },
                        {
                            "name": "far",
                            "sim": m::similarity_score("COSINE", json!([0.1, 1, 0]), json!([1, 0, 0])),
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
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute) {
                                        User(order: {_alias: {sim: ASC}}, limit: 2){
                                            sim: SIMILARITY(vector: {vector: [1, 0, 0]})
                                        }
                                    }
                "#.into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 0, "docFetches": 4 }))),
                expected_extensions: json!(m::object(json!({
                    "warnings": [
                        m::object(json!({
                            "code": "VECTOR_INDEX_UNUSED",
                            "detail": { "field": "vector", "reason": "notOrderedBySimilarityDesc" },
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
#[mantra_macros::req_test("go.query.simple.similarity.vector-index")]
fn query_simple_with_similarity_on_vector_index_excludes_deleted_doc() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                                        name: String
                                        vector: [Float32!] @index(vector: {dimensions: 3, hnsw: {metric: COSINE}})
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
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "xy", "vector": [0.9, 0.4, 0] })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            DeleteDoc {
                collection: "User".into(),
                doc_id: Text::reference("doc0_0"),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User(order: {_alias: {sim: DESC}}, limit: 1){
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
#[mantra_macros::req_test("go.query.simple.similarity.vector-index")]
fn query_simple_with_similarity_on_vector_index_is_magnitude_invariant() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                                        name: String
                                        vector: [Float32!] @index(vector: {dimensions: 3, hnsw: {metric: COSINE}})
                                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "unit", "vector": [1, 0, 0] })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "long", "vector": [10, 0, 0] })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "off", "vector": [0, 1, 0] })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User(order: {_alias: {sim: DESC}}, limit: 2){
                                            sim: SIMILARITY(vector: {vector: [1, 0, 0]})
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "User": [
                        {
                            "sim": m::similarity_score("COSINE", json!([1, 0, 0]), json!([1, 0, 0])),
                        },
                        {
                            "sim": m::similarity_score("COSINE", json!([10, 0, 0]), json!([1, 0, 0])),
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
#[mantra_macros::req_test("go.query.simple.similarity.vector-index-fallback")]
fn query_simple_with_similarity_on_vector_index_no_order_does_not_use_index() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                                        name: String
                                        vector: [Float32!] @index(vector: {dimensions: 3, hnsw: {metric: COSINE}})
                                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "x", "vector": [1.0, 0.0, 0.0] })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "y", "vector": [0.0, 1.0, 0.0] })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "xy", "vector": [0.9, 0.4, 0.0] })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "z", "vector": [0.0, 0.0, 1.0] })),
                common: Common { capture: captures([("doc0_3", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query @explain(type: execute) {
                                        User(limit: 2){
                                            sim: SIMILARITY(vector: {vector: [1, 0, 0]})
                                        }
                                    }
                "#.into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 0 }))),
                expected_extensions: json!(m::object(json!({
                    "warnings": [
                        m::object(json!({
                            "code": "VECTOR_INDEX_UNUSED",
                            "detail": { "field": "vector", "reason": "notOrderedBySimilarityDesc" },
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
#[mantra_macros::req_test("go.query.simple.similarity.vector-index")]
fn query_simple_with_similarity_on_vector_index_reflects_updated_vector() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                                        name: String
                                        vector: [Float32!] @index(vector: {dimensions: 3, hnsw: {metric: COSINE}})
                                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "a", "vector": [0.6, 0.8, 0] })),
                common: Common { capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "b", "vector": [0, 1, 0] })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "User".into(),
                doc_id: Text::reference("doc0_1"),
                doc_map: json!({ "vector": [1, 0, 0] }),
                expected_data: json!({}),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User(order: {_alias: {sim: DESC}}, limit: 1){
                                            name
                                            sim: SIMILARITY(vector: {vector: [1, 0, 0]})
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "User": [
                        {
                            "name": "b",
                            "sim": m::similarity_score("COSINE", json!([1, 0, 0]), json!([1, 0, 0])),
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
#[mantra_macros::req_test("go.query.simple.similarity.vector-index")]
fn query_simple_with_similarity_on_vector_index_respects_offset() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                                        name: String
                                        vector: [Float32!] @index(vector: {dimensions: 3, hnsw: {metric: COSINE}})
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
                doc_map: Some(json!({ "name": "a", "vector": [0.9, 0.4, 0] })),
                common: Common { capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "b", "vector": [0.6, 0.8, 0] })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "c", "vector": [0, 1, 0] })),
                common: Common { capture: captures([("doc0_3", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User(order: {_alias: {sim: DESC}}, limit: 2, offset: 1){
                                            name
                                            sim: SIMILARITY(vector: {vector: [1, 0, 0]})
                                        }
                                    }
                "#.into(),
                expected_data: json!({
                    "User": [
                        {
                            "name": "a",
                            "sim": m::similarity_score("COSINE", json!([0.9, 0.4, 0]), json!([1, 0, 0])),
                        },
                        {
                            "name": "b",
                            "sim": m::similarity_score("COSINE", json!([0.6, 0.8, 0]), json!([1, 0, 0])),
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
#[mantra_macros::req_test("go.query.simple.similarity.vector-index")]
fn query_simple_with_similarity_on_vector_index_returns_k_nearest() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                                        name: String
                                        vector: [Float32!] @index(vector: {dimensions: 3, hnsw: {metric: COSINE}})
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
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "xy", "vector": [0.9, 0.4, 0] })),
                common: Common { capture: captures([("doc0_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "z", "vector": [0, 0, 1] })),
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
            Request {
                request: r#"
                    query @explain(type: execute) {
                                        User(order: {_alias: {sim: DESC}}, limit: 2){
                                            name
                                            sim: SIMILARITY(vector: {vector: [1, 0, 0]})
                                        }
                                    }
                "#.into(),
                expected_data: json!(m::explain_scan(json!({ "indexFetches": 1, "docFetches": 2 }))),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.query.simple.similarity.vector-index-fallback")]
fn query_simple_with_similarity_on_vector_index_wrong_length_query_errors() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type User {
                                        name: String
                                        vector: [Float32!] @index(vector: {dimensions: 3, hnsw: {metric: COSINE}})
                                    }
                "#.into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "x", "vector": [1, 0, 0] })),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "y", "vector": [0, 1, 0] })),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        User(order: {_alias: {sim: DESC}}, limit: 1){
                                            name
                                            sim: SIMILARITY(vector: {vector: [1, 0]})
                                        }
                                    }
                "#.into(),
                expected_error: Some("source and vector must be of the same length".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
