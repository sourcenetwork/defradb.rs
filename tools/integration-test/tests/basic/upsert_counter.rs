//! An upsert that finds its document applies counter values as increments,
//! like an update, locally and on a replicating peer.

use std::time::Duration;

use integration_test::{extract_p2p_addr, poll_until, DefraClient, TestCluster, P2P_TIMEOUT};
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

#[tokio::test]
async fn upsert_adds_to_counters_and_refuses_a_negative_pcounter() {
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
}

#[tokio::test]
async fn a_peer_converges_on_an_upserted_counter_sum() {
    let cluster = TestCluster::builder()
        .rust_nodes(2)
        .with_iroh_transport()
        .build()
        .await
        .unwrap();
    for index in 0..2 {
        cluster
            .wait_for_log(index, "p2p_listening", P2P_TIMEOUT)
            .await
            .expect("P2P listener did not start");
    }
    let node0 = cluster.client(0);
    let node1 = cluster.client(1);
    let addr0 = extract_p2p_addr(&cluster, 0);
    let addr1 = extract_p2p_addr(&cluster, 1);
    for node in [&node0, &node1] {
        node.schema_add(SDL).expect("schema add");
        node.p2p_collection_add(&["Score"]).expect("collection add");
    }
    node0.p2p_connect(&[&addr1]).expect("connect");
    node0
        .p2p_replicator_set(&["Score"], &addr1)
        .expect("replicator 0 to 1");
    node1
        .p2p_replicator_set(&["Score"], &addr0)
        .expect("replicator 1 to 0");

    upsert(&node0, "{points: 10, delta: 10}").expect("first upsert creates");
    poll_until(
        || sums(&node1) == Some((5, 5)),
        P2P_TIMEOUT,
        Duration::from_millis(200),
        "document did not replicate",
    )
    .await;

    upsert(&node0, "{points: 10, delta: -3}").expect("upsert updates");
    assert_eq!(sums(&node0), Some((15, 2)));
    poll_until(
        || sums(&node1) == Some((15, 2)),
        P2P_TIMEOUT,
        Duration::from_millis(200),
        "peer did not reach the upserted sums",
    )
    .await;
}
