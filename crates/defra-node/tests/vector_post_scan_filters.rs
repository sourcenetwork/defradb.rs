use defra_node::EmbeddedNode;
use serde_json::{json, Value};

async fn data(node: &EmbeddedNode, query: &str) -> Value {
    let response = node.execute(query).await;
    assert!(!response.has_errors(), "{:?}", response.errors);
    response.data.unwrap()
}

async fn fixture() -> EmbeddedNode {
    let node = EmbeddedNode::builder().build().await.unwrap();
    node.add_schema(
        "type Note { tag: String embedding: [Float32!] @index(vector: {dimensions: 1, hnsw: {metric: DOT}}) }",
    ).await.unwrap();
    for (tag, score) in [
        ("hot", 100),
        ("hot", 99),
        ("hot", 98),
        ("cold", 10),
        ("cold", 9),
        ("cold", 8),
    ] {
        data(&node, &format!(
            r#"mutation {{ create_Note(input: {{tag: "{tag}", embedding: [{score}]}}) {{ _docID }} }}"#,
        )).await;
    }
    node
}

fn uses_vector_index(value: &Value) -> bool {
    match value {
        Value::Object(fields) => {
            fields.get("vectorIndex").is_some_and(Value::is_string)
                || fields.values().any(uses_vector_index)
        }
        Value::Array(values) => values.iter().any(uses_vector_index),
        _ => false,
    }
}

#[tokio::test]
async fn computed_similarity_filter_keeps_a_full_page_after_offset() {
    let node = fixture().await;
    let query = r#"{ Note(filter: {_alias: {sim: {_lt: 50}}}, order: {_alias: {sim: DESC}}, limit: 2, offset: 1) {
        tag sim: SIMILARITY(embedding: {vector: [1]})
    } }"#;
    assert_eq!(
        data(&node, query).await["Note"],
        json!([
            {"tag": "cold", "sim": 9.0}, {"tag": "cold", "sim": 8.0}
        ])
    );
    let explain = data(&node, &format!("query @explain(type: execute) {query}")).await;
    assert!(!uses_vector_index(&explain), "{explain}");
}

#[tokio::test]
async fn group_limit_is_not_used_as_a_document_candidate_limit() {
    let node = fixture().await;
    let query = r#"{ Note(groupBy: [tag], order: {_alias: {sim: DESC}}, limit: 2) {
        tag sim: SIMILARITY(embedding: {vector: [1]})
    } }"#;
    let result = data(&node, query).await;
    let mut tags: Vec<_> = result["Note"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["tag"].as_str().unwrap())
        .collect();
    tags.sort_unstable();
    assert_eq!(tags, ["cold", "hot"]);
    let explain = data(&node, &format!("query @explain(type: execute) {query}")).await;
    assert!(!uses_vector_index(&explain), "{explain}");
}

#[tokio::test]
async fn scalar_filter_keeps_vector_routing() {
    let node = fixture().await;
    let query = r#"{ Note(filter: {tag: {_eq: "cold"}}, order: {_alias: {sim: DESC}}, limit: 2) {
        tag sim: SIMILARITY(embedding: {vector: [1]})
    } }"#;
    assert_eq!(
        data(&node, query).await["Note"],
        json!([
            {"tag": "cold", "sim": 10.0}, {"tag": "cold", "sim": 9.0}
        ])
    );
    let explain = data(&node, &format!("query @explain(type: execute) {query}")).await;
    assert!(uses_vector_index(&explain), "{explain}");
}
