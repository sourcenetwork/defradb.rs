//! An execute explain of a query routed to a vector index counts the index
//! entries the search read, for every algorithm.

use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

const VECTORS: [[f64; 4]; 8] = [
    [1.0, 0.0, 0.0, 0.0],
    [0.9, 0.4, 0.0, 0.0],
    [0.6, 0.8, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.6, 0.8],
    [0.0, 0.0, 0.0, 1.0],
    [-1.0, 0.2, 0.0, 0.0],
];

/// Eight documents and a page of two: the search reads at least the two
/// entries it returns, where Go counts the whole search as one fetch.
fn routed_search_counts_its_reads(id: &'static str, algorithm: &str) {
    let mut actions = vec![AddCollection {
        sdl: format!(
            "type User {{ name: String vector: [Float32!] @index(vector: {{dimensions: 4, {algorithm}}}) }}"
        )
        .into(),
        ..Default::default()
    }
    .into()];
    for (i, vector) in VECTORS.iter().enumerate() {
        actions.push(
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": format!("u{i}"), "vector": vector })),
                ..Default::default()
            }
            .into(),
        );
    }
    actions.push(
        Request {
            request: r#"
                query @explain(type: execute) {
                    User(order: {_alias: {sim: DESC}}, limit: 2) {
                        name
                        sim: SIMILARITY(vector: {vector: [1, 0, 0, 0]})
                    }
                }
            "#
            .into(),
            expected_data: json!(m::explain_scan(json!({
                "indexFetches": m::at_least(2),
                "docFetches": 2,
            }))),
            ..Default::default()
        }
        .into(),
    );
    TestCase {
        id,
        actions,
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.explain.execute.vector_index_fetches")]
fn hnsw_search_counts_its_reads() {
    routed_search_counts_its_reads(integration_test::scenario_id!(), "hnsw: {metric: COSINE}");
}

#[test]
#[mantra_macros::req_test("defra.explain.execute.vector_index_fetches")]
fn flat_search_counts_its_reads() {
    routed_search_counts_its_reads(integration_test::scenario_id!(), "flat: {metric: COSINE}");
}

#[test]
#[mantra_macros::req_test("defra.explain.execute.vector_index_fetches")]
fn ivfflat_search_counts_its_reads() {
    routed_search_counts_its_reads(
        integration_test::scenario_id!(),
        "ivfflat: {metric: COSINE, nlist: 2, nprobe: 2}",
    );
}

#[test]
#[mantra_macros::req_test("defra.explain.execute.vector_index_fetches")]
fn ivfpq_search_counts_its_reads() {
    routed_search_counts_its_reads(
        integration_test::scenario_id!(),
        "ivfpq: {metric: COSINE, nlist: 2, nprobe: 2, m: 2}",
    );
}

#[test]
#[mantra_macros::req_test("defra.explain.execute.vector_index_fetches")]
fn ssg_search_counts_its_reads() {
    routed_search_counts_its_reads(
        integration_test::scenario_id!(),
        "ssg: {metric: COSINE, R: 4}",
    );
}

#[test]
#[mantra_macros::req_test("defra.explain.execute.vector_index_fetches")]
fn diskann_search_counts_its_reads() {
    routed_search_counts_its_reads(
        integration_test::scenario_id!(),
        "diskann: {metric: COSINE, R: 8, m: 2}",
    );
}
