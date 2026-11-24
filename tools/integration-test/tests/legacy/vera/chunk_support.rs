//! Helpers for the Vera chunk access test.

use integration_test::DefraClient;

const CHUNKS: &str = "Article__body_chunk";

/// A body of `n` short paragraphs about `topic`, escaped for a GraphQL string.
pub fn body(n: usize, topic: &str) -> String {
    (0..n)
        .map(|i| format!("Paragraph {i} is about {topic} and nothing else."))
        .collect::<Vec<_>>()
        .join("\\n\\n")
}

/// The visible chunk hashes, sorted.
pub fn visible_hashes(client: &DefraClient, identity: Option<&str>) -> Vec<String> {
    let query = format!("query {{ {CHUNKS} {{ hash }} }}");
    let data = match identity {
        Some(key) => client.query_with_identity(&query, key),
        None => client.query(&query),
    }
    .expect("read chunks");
    let mut hashes: Vec<String> = data[CHUNKS]
        .as_array()
        .expect("chunks array")
        .iter()
        .map(|c| c["hash"].as_str().unwrap().to_string())
        .collect();
    hashes.sort();
    hashes
}
