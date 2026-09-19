//! CLI schema creation and HTTP vector queries against a persistent node.

use integration_test::TestCluster;
use serde_json::{json, Value};
use std::time::Duration;

async fn graphql(http: &reqwest::Client, api: &str, query: &str) -> Value {
    let response = http
        .post(format!("{api}/api/v0/graphql"))
        .json(&json!({"query": query}))
        .send()
        .await
        .unwrap();
    let status = response.status();
    let body: Value = response.json().await.unwrap();
    assert!(status.is_success(), "{status}: {body}");
    assert!(
        body["errors"].as_array().is_none_or(Vec::is_empty),
        "{body}"
    );
    body["data"].clone()
}

fn routed_scan(value: &Value) -> Option<&Value> {
    match value {
        Value::Object(map) if map.contains_key("vectorIndex") => Some(value),
        Value::Object(map) => map.values().find_map(routed_scan),
        Value::Array(items) => items.iter().find_map(routed_scan),
        _ => None,
    }
}

async fn assert_page(http: &reqwest::Client, api: &str, first: usize) -> u64 {
    let query = "{ Note(limit: 5, order: {_alias: {sim: DESC}}) { title sim: SIMILARITY(embedding: {vector: [0, 1, 0, 0]}) } }";
    let result = graphql(http, api, query).await;
    let titles: Vec<_> = result["Note"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["title"].as_str().unwrap())
        .collect();
    let expected: Vec<_> = (first..first + 5).map(|id| format!("n{id}")).collect();
    assert_eq!(titles, expected);

    let explain = graphql(http, api, &format!("query @explain(type: execute) {query}")).await;
    let scan = routed_scan(&explain).unwrap_or_else(|| panic!("full-scan fallback: {explain:#}"));
    assert!(scan["indexFetches"].as_u64().unwrap() > 0);
    scan["docFetches"].as_u64().unwrap()
}

async fn lifecycle(algorithm: &str) {
    let mut cluster = TestCluster::builder()
        .rust_nodes(1)
        .with_store("regolith")
        .with_extra_rust_args(["--log-level", "info"])
        .build()
        .await
        .unwrap();
    let client_dir = tempfile::tempdir().unwrap();
    let schema = format!(
        "type Note {{ title: String embedding: [Float32!] @index(vector: {{dimensions: 4, {algorithm}}}) }}"
    );
    let output = std::process::Command::new(cluster.client(0).binary_path())
        .arg("--rootdir")
        .arg(client_dir.path())
        .args([
            "--url",
            cluster.api_url(0).strip_prefix("http://").unwrap(),
            "client",
            "collection",
            "add",
            &schema,
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .unwrap();
    // Four lists / four SSG edges need 4 * 39 live vectors to trigger a build.
    let input = (0..156)
        .map(|i| format!("{{title: \"n{i}\", embedding: [{i}, 1, 0, 0]}}"))
        .collect::<Vec<_>>()
        .join(",");
    let created = graphql(
        &http,
        cluster.api_url(0),
        &format!("mutation {{ add_Note(input: [{input}]) {{ _docID title }} }}"),
    )
    .await;
    let rows = created["add_Note"].as_array().unwrap();
    assert_eq!(rows.len(), 156);
    let id = |title: &str| {
        rows.iter().find(|row| row["title"] == title).unwrap()["_docID"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let updated = id("n0");
    let deleted = id("n1");
    assert_eq!(assert_page(&http, cluster.api_url(0), 0).await, 5);
    cluster
        .restart_node(0, Duration::from_secs(60))
        .await
        .unwrap();
    assert_eq!(assert_page(&http, cluster.api_url(0), 0).await, 5);

    graphql(&http, cluster.api_url(0), &format!(
        "mutation {{ update_Note(docID: \"{updated}\", input: {{embedding: [155, 1, 0, 0]}}) {{ _docID }} }}"
    )).await;
    assert_eq!(assert_page(&http, cluster.api_url(0), 1).await, 5);
    graphql(
        &http,
        cluster.api_url(0),
        &format!("mutation {{ delete_Note(docID: \"{deleted}\") {{ _docID }} }}"),
    )
    .await;
    // IVF-PQ can return a short candidate page after skipping tombstones;
    // the planner must still fill the requested page through its fallback.
    assert_page(&http, cluster.api_url(0), 2).await;
    cluster
        .restart_node(0, Duration::from_secs(60))
        .await
        .unwrap();
    assert_page(&http, cluster.api_url(0), 2).await;
}

#[tokio::test]
async fn rust_flat_vector_lifecycle() {
    lifecycle("flat: {metric: EUCLIDEAN}").await;
}

#[tokio::test]
async fn rust_hnsw_vector_lifecycle() {
    lifecycle("hnsw: {metric: EUCLIDEAN}").await;
}

#[tokio::test]
async fn rust_ivfpq_vector_lifecycle() {
    lifecycle("ivfpq: {metric: COSINE, nlist: 4, nprobe: 4, m: 2}").await;
}

#[tokio::test]
async fn rust_ssg_vector_lifecycle() {
    lifecycle("ssg: {metric: EUCLIDEAN, R: 4}").await;
}
