//! Shared helpers for the chunk tests.
#![allow(dead_code)]

use integration_test::DefraClient;
use serde_json::Value;

pub const SCHEMA: &str = r#"type Article {
    title: String
    body: String @chunk(maxChars: 64)
}"#;

pub const CHUNKS: &str = "Article__body_chunk";

/// A body of `n` short paragraphs about `topic`, escaped for a GraphQL string.
pub fn body(n: usize, topic: &str) -> String {
    (0..n)
        .map(|i| format!("Paragraph {i} is about {topic} and nothing else."))
        .collect::<Vec<_>>()
        .join("\\n\\n")
}

pub fn create(client: &DefraClient, body: &str) -> String {
    let data = client
        .query(&format!(
            r#"mutation {{ add_Article(input: {{title: "t", body: "{body}"}}) {{ _docID }} }}"#
        ))
        .expect("create article");
    data["add_Article"][0]["_docID"]
        .as_str()
        .expect("docID")
        .to_string()
}

pub fn update(client: &DefraClient, doc_id: &str, body: &str) {
    client
        .query(&format!(
            r#"mutation {{ update_Article(docID: "{doc_id}", input: {{body: "{body}"}}) {{ _docID }} }}"#
        ))
        .expect("update article");
}

/// The parent's chunk list: entry 0 is the text's hash, then the chunk hashes.
pub fn list_of(client: &DefraClient, doc_id: &str, identity: Option<&str>) -> Vec<String> {
    let query = format!(r#"query {{ Article(docID: "{doc_id}") {{ _body_chunks }} }}"#);
    let data = match identity {
        Some(key) => client.query_with_identity(&query, key),
        None => client.query(&query),
    }
    .expect("read list");
    data["Article"][0]["_body_chunks"]
        .as_array()
        .map(|list| {
            list.iter()
                .map(|h| h.as_str().unwrap().to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// The distinct chunk hashes a parent's list names, sorted.
pub fn listed(client: &DefraClient, doc_id: &str, identity: Option<&str>) -> Vec<String> {
    let mut hashes: Vec<String> = list_of(client, doc_id, identity)
        .into_iter()
        .skip(1)
        .collect();
    hashes.sort();
    hashes.dedup();
    hashes
}

/// The visible chunks as (docID, hash), sorted by hash.
pub fn visible(client: &DefraClient, identity: Option<&str>) -> Vec<(String, String)> {
    let query = format!("query {{ {CHUNKS} {{ _docID hash }} }}");
    let data = match identity {
        Some(key) => client.query_with_identity(&query, key),
        None => client.query(&query),
    }
    .expect("read chunks");
    let mut chunks: Vec<(String, String)> = data[CHUNKS]
        .as_array()
        .expect("chunks array")
        .iter()
        .map(|c| {
            (
                c["_docID"].as_str().unwrap().to_string(),
                c["hash"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    chunks.sort_by(|a, b| a.1.cmp(&b.1));
    chunks
}

pub fn visible_hashes(client: &DefraClient, identity: Option<&str>) -> Vec<String> {
    visible(client, identity)
        .into_iter()
        .map(|(_, h)| h)
        .collect()
}

pub fn first_doc_id(data: &Value, key: &str) -> String {
    data[key][0]["_docID"].as_str().expect("docID").to_string()
}
