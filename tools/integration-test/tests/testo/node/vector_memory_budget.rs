//! A node's vector memory budget decides what its vector indexes keep in
//! memory between searches, never what a search returns: a node with no room
//! at all still ranks the exact nearest documents.

use integration_test::actions::{AddCollection, NewNode, Request};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::json;

/// Past DiskANN's training threshold, so searches walk the trained graph by
/// its resident codes.
const NOTES: usize = 1_124;

#[test]
#[mantra_macros::req_test("defra.node.vector_memory_budget")]
fn a_node_with_no_vector_memory_returns_the_nearest_documents() {
    let notes = (0..NOTES)
        .map(|i| format!("{{title: \"n{i}\", embedding: [{i}, 1, 0, 0]}}"))
        .collect::<Vec<_>>()
        .join(", ");
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                vector_memory_budget: Some(0),
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type Note {
                        title: String
                        embedding: [Float32!] @index(vector: {dimensions: 4, diskann: {metric: COSINE, R: 8, m: 2, residentCodes: true}})
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: format!("mutation {{ add_Note(input: [{notes}]) {{ title }} }}").into(),
                expected_data: json!({ "add_Note": (0..NOTES).map(|i| json!({ "title": format!("n{i}") })).collect::<Vec<_>>() }),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Note(limit: 5, order: {_alias: {sim: DESC}}) {
                            title
                            sim: SIMILARITY(embedding: {vector: [0, 1, 0, 0]})
                        }
                    }
                "#
                .into(),
                expected_data: json!({
                    "Note": (0..5).map(|i| json!({
                        "title": format!("n{i}"),
                        "sim": m::similarity_score("COSINE", json!([i, 1, 0, 0]), json!([0, 1, 0, 0])),
                    })).collect::<Vec<_>>(),
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
