//! REST document writes are signed by the node identity, as Go signs them.

use integration_test::{generate_identity, rust_binary, TestCluster};
use reqwest::{Client, Method, StatusCode};
use serde_json::json;
use serial_test::serial;

use super::rest_transactions::request;

#[tokio::test]
#[serial]
async fn rust_rest_writes_are_signed() {
    let identity = generate_identity(&rust_binary()).expect("generate node identity");
    let cluster = TestCluster::builder()
        .rust_nodes(1)
        .with_identity(&identity.private_key_hex)
        .with_signing()
        .build()
        .await
        .expect("cluster starts");
    let node = cluster.client(0);
    node.schema_add("type Users { name: String }")
        .expect("schema add");

    let api = cluster.api_url(0);
    let client = Client::new();
    request(
        &client,
        api,
        Method::POST,
        "/collections/Users",
        None,
        Some(json!({"name": "Alice"})),
        StatusCode::OK,
    )
    .await;
    let transaction = request(
        &client,
        api,
        Method::POST,
        "/tx",
        None,
        None,
        StatusCode::OK,
    )
    .await["id"]
        .to_string();
    request(
        &client,
        api,
        Method::POST,
        "/collections/Users",
        Some(&transaction),
        Some(json!({"name": "Bob"})),
        StatusCode::OK,
    )
    .await;
    request(
        &client,
        api,
        Method::POST,
        &format!("/tx/{transaction}"),
        None,
        None,
        StatusCode::OK,
    )
    .await;

    let commits = node
        .query(r#"query { _commits { fieldName signature { type } } }"#)
        .expect("_commits query");
    let commits = commits["_commits"].as_array().expect("_commits array");
    let unsigned: Vec<_> = commits
        .iter()
        .filter(|commit| commit["signature"].is_null())
        .collect();
    assert!(!commits.is_empty(), "expected commits for the REST writes");
    assert!(
        unsigned.is_empty(),
        "{} of {} REST commits are unsigned: {unsigned:?}",
        unsigned.len(),
        commits.len()
    );
}
