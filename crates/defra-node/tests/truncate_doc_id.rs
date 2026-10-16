//! `truncate_X(docID:)`, ported from Go's `tests/integration/mutation/truncate/docid_test.go`.

use defra_node::EmbeddedNode;
use serde_json::{json, Value};

async fn node_with_users(names: &[&str]) -> (EmbeddedNode, Vec<String>) {
    let node = EmbeddedNode::builder().build().await.unwrap();
    node.add_schema("type User { name: String }").await.unwrap();
    let mut ids = Vec::new();
    for name in names {
        let row = data(
            &node,
            &format!(r#"mutation {{ add_User(input: [{{name: "{name}"}}]) {{ _docID }} }}"#),
        )
        .await;
        ids.push(row["add_User"][0]["_docID"].as_str().unwrap().to_owned());
    }
    (node, ids)
}

async fn data(node: &EmbeddedNode, query: &str) -> Value {
    let response = node.execute(query).await;
    assert!(!response.has_errors(), "{query}: {:?}", response.errors);
    response.data.unwrap()
}

async fn truncate(node: &EmbeddedNode, args: &str) {
    let result = data(node, &format!("mutation {{ truncate_User({args}) }}")).await;
    assert_eq!(result, json!({ "truncate_User": true }));
}

async fn names(node: &EmbeddedNode) -> Vec<String> {
    let rows = data(node, "query { User { name } }").await;
    let mut names: Vec<String> = rows["User"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["name"].as_str().unwrap().to_owned())
        .collect();
    names.sort();
    names
}

#[tokio::test]
async fn truncate_with_doc_id_list() {
    let (node, ids) = node_with_users(&["Alice", "Bob"]).await;
    truncate(&node, &format!(r#"docID: ["{}"]"#, ids[0])).await;
    assert_eq!(names(&node).await, ["Bob"]);
}

#[tokio::test]
async fn truncate_with_scalar_doc_id() {
    let (node, ids) = node_with_users(&["Alice", "Bob"]).await;
    truncate(&node, &format!(r#"docID: "{}""#, ids[0])).await;
    assert_eq!(names(&node).await, ["Bob"]);
}

#[tokio::test]
async fn truncate_with_multiple_doc_ids() {
    let (node, ids) = node_with_users(&["Alice", "Bob", "Carol"]).await;
    truncate(&node, &format!(r#"docID: ["{}", "{}"]"#, ids[0], ids[1])).await;
    assert_eq!(names(&node).await, ["Carol"]);
}

#[tokio::test]
async fn truncate_with_unknown_doc_id_removes_nothing() {
    let (node, _) = node_with_users(&["Alice"]).await;
    truncate(
        &node,
        r#"docID: ["bae-390b4419-fe1c-506b-98bd-20847cdab2d9"]"#,
    )
    .await;
    assert_eq!(names(&node).await, ["Alice"]);
}

#[tokio::test]
async fn truncate_with_doc_id_and_filter_requires_both() {
    let (node, ids) = node_with_users(&["Alice", "Bob"]).await;
    truncate(
        &node,
        &format!(r#"docID: ["{}"], filter: {{name: {{_eq: "Bob"}}}}"#, ids[0]),
    )
    .await;
    assert_eq!(names(&node).await, ["Alice", "Bob"]);
}

#[tokio::test]
async fn truncate_rejects_null_doc_id() {
    let (node, _) = node_with_users(&["Alice"]).await;
    let response = node
        .execute("mutation { truncate_User(docID: null) }")
        .await;
    assert!(
        format!("{:?}", response.errors).contains("truncate docID cannot be null"),
        "{:?}",
        response.errors
    );
    assert_eq!(names(&node).await, ["Alice"]);
}
