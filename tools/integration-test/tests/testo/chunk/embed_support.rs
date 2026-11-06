//! Shared helpers for the chunk embedding tests: a schema whose chunks are
//! embedded by the test's own model.

use integration_test::actions::{
    embedding_of, Actions, AddCollection, EmbeddingModel, Request, EMBEDDING_CAPTURE,
};
use integration_test::scenario::{Common, Text};
use serde_json::{json, Value};

use super::support::{on, CHUNKS};

/// `Article` with its body chunked and each chunk embedded, under `policy`
/// when one is given.
pub fn embedded_schema(policy: Option<&str>) -> Text {
    let directive = match policy {
        Some(capture) => Text::concat([
            Text::from(r#"@policy(id: ""#),
            Text::reference(capture),
            Text::from(r#"", resource: "users")"#),
        ]),
        None => Text::from(""),
    };
    Text::concat([
        Text::from("type Article "),
        directive,
        Text::from(
            r#" {
                title: String
                body: String @chunk(maxChars: 64)
                body_v: [Float32!] @embedding(fields: ["body"], "#,
        ),
        Text::reference(EMBEDDING_CAPTURE),
        Text::from(
            r#") @index(vector: {dimensions: 3})
            }"#,
        ),
    ])
}

pub fn add_embedded_collection(node: Option<&str>) -> Actions {
    AddCollection {
        sdl: embedded_schema(None),
        common: on(node),
        ..Default::default()
    }
    .into()
}

/// Starts the test's model, or reads what it embedded since the last call:
/// exactly `texts`, in any order.
pub fn model_embedded(texts: &[String]) -> Actions {
    EmbeddingModel {
        expected_data: json!({ "embedded": texts }),
        common: Common {
            unordered: vec!["/embedded".into()],
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

/// The text the model is asked to embed for a chunk: its text and a newline.
pub fn embedding_input(chunk: &str) -> String {
    format!("{chunk}\n")
}

/// Paragraph `i` about `topic`, as a chunk holds it: every paragraph but the
/// last keeps its blank line.
pub fn paragraph(i: usize, topic: &str, last: bool) -> String {
    let text = format!("Paragraph {i} is about {topic} and nothing else.");
    match last {
        true => text,
        false => format!("{text}\n\n"),
    }
}

/// The chunks of `super::support::body(n, topic)`, as stored.
pub fn chunks_of(n: usize, topic: &str) -> Vec<String> {
    (0..n).map(|i| paragraph(i, topic, i + 1 == n)).collect()
}

/// Every chunk holds exactly `chunks`, each with the model's vector of its text.
pub fn chunks_have_vectors(chunks: &[String], node: Option<&str>) -> Actions {
    let expected: Vec<Value> = chunks
        .iter()
        .map(|text| json!({ "text": text, "body_v": embedding_of(&embedding_input(text)) }))
        .collect();
    let mut common = on(node);
    common.unordered = vec![format!("/data/{CHUNKS}")];
    Request {
        request: format!("query {{ {CHUNKS} {{ text body_v }} }}").into(),
        expected_data: json!({ CHUNKS: expected }),
        common,
        ..Default::default()
    }
    .into()
}
