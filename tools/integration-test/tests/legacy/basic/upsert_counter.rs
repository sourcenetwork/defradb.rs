//! An upsert that finds its document applies counter values as increments,
//! like an update.

use integration_test::{DefraClient, TestCluster};
use serde_json::Value;

const SDL: &str = "type Score {
    name: String
    points: Int @crdt(type: pcounter)
    delta: Int @crdt(type: pncounter)
}";

fn row(node: &DefraClient) -> Option<Value> {
    let data = node.query("query { Score { name points delta } }").ok()?;
    match data["Score"].as_array()?.as_slice() {
        [row] => Some(row.clone()),
        _ => None,
    }
}

fn sums(node: &DefraClient) -> Option<(i64, i64)> {
    let row = row(node)?;
    Some((row["points"].as_i64()?, row["delta"].as_i64()?))
}

fn upsert(node: &DefraClient, input: &str) -> Result<Value, String> {
    node.query(&format!(
        r#"mutation {{ upsert_Score(filter: {{name: {{_eq: "a"}}}}, add: {{name: "a", points: 5, delta: 5}}, update: {input}) {{ _docID }} }}"#
    ))
    .map_err(|e| e.to_string())
}

#[nuclei::test]
async fn upsert_adds_to_counters_and_refuses_a_negative_pcounter() {
    edge::compat(async move {
        let cluster = TestCluster::builder().rust_nodes(1).build().await.unwrap();
        let node = cluster.client(0);
        node.schema_add(SDL).expect("schema add");

        upsert(&node, "{points: 10, delta: 10}").expect("first upsert creates");
        assert_eq!(sums(&node), Some((5, 5)), "the add input creates");

        upsert(&node, "{points: 10, delta: -3}").expect("upsert updates");
        assert_eq!(sums(&node), Some((15, 2)));

        let error = upsert(&node, "{points: -1}").expect_err("negative P-counter");
        assert!(error.contains("value cannot be negative"), "{error}");
        assert_eq!(sums(&node), Some((15, 2)));
    })
    .await;
}
