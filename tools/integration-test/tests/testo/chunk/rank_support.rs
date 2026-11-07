//! Shared helpers for ranking articles by their best chunk.

use integration_test::actions::{Actions, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::Common;
use serde_json::{json, Value};

use super::embed_support::paragraph;

/// The vector index `body_v` gets on the chunk collection.
pub const CHUNK_VECTOR_INDEX: &str = "Article__body_chunk_body_v_ASC";

/// The query vector: the direction of a paragraph about rivers.
pub const RIVERS: [f64; 3] = [1.0, 0.0, 0.0];

/// A body with one paragraph per topic, one chunk each.
pub fn body_about(topics: &[&str]) -> String {
    topics
        .iter()
        .enumerate()
        .map(|(i, topic)| paragraph(i, topic, i + 1 == topics.len()))
        .collect()
}

/// Adds an article titled `title` with a paragraph per topic, as `identity`.
pub fn add_article_about(title: &str, topics: &[&str], identity: Option<u64>) -> Actions {
    AddDoc {
        collection: "Article".into(),
        doc_map: Some(json!({ "title": title, "body": body_about(topics) })),
        common: Common {
            identity_seed: identity,
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

/// The score of a chunk whose text counts `rivers`, `engines` and `music` as given.
pub fn score(counts: [f64; 3]) -> Value {
    m::similarity_score("COSINE", json!(counts), json!(RIVERS))
}

/// The articles nearest to rivers, best first, under `arguments`.
pub fn ranked(arguments: &str, expected: Value) -> Request {
    ranked_as(arguments, expected, None)
}

pub fn ranked_as(arguments: &str, expected: Value, identity: Option<u64>) -> Request {
    Request {
        request: format!(
            "query {{ Article(order: {{_alias: {{sim: DESC}}}}{arguments}) {{
                title
                sim: SIMILARITY(body_v: {{vector: [1, 0, 0]}})
            }} }}"
        )
        .into(),
        expected_data: json!({ "Article": expected }),
        common: Common {
            identity_seed: identity,
            ..Default::default()
        },
        ..Default::default()
    }
}

/// The same query under `@explain(type: execute)`, whose parent scan reads `scan`.
pub fn explained(arguments: &str, scan: Value) -> Request {
    Request {
        request: format!(
            "query @explain(type: execute) {{ Article(order: {{_alias: {{sim: DESC}}}}{arguments}) {{
                title
                sim: SIMILARITY(body_v: {{vector: [1, 0, 0]}})
            }} }}"
        )
        .into(),
        expected_data: m::explain_target("scanNode", false, 0, scan),
        ..Default::default()
    }
}
