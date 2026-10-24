//! A replicated write shows up in both peers' `sync/status` summaries and
//! `peers/{peer_id}` details, on both transports.

use std::time::Duration;

use integration_test::{poll_until, TestCluster, TestClusterBuilder};
use serde_json::Value;
use serial_test::serial;

const SCHEMA: &str = "type Users { name: String  age: Int }";
const TIMEOUT: Duration = Duration::from_secs(30);
const POLL: Duration = Duration::from_millis(200);

async fn get_json(url: String) -> Value {
    reqwest::get(&url)
        .await
        .unwrap_or_else(|error| panic!("GET {url}: {error}"))
        .json()
        .await
        .unwrap_or_else(|error| panic!("GET {url} json: {error}"))
}

async fn connected_peer(cluster: &TestCluster, node: usize) -> String {
    let url = format!("{}/api/v0/p2p/sync/status", cluster.api_url(node));
    let deadline = tokio::time::Instant::now() + TIMEOUT;
    loop {
        let status = get_json(url.clone()).await;
        if let Some(peer) = status["peers"]
            .as_array()
            .and_then(|peers| peers.iter().find(|peer| peer["connected"] == true))
        {
            return peer["peer_id"].as_str().unwrap().to_string();
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "node{node} never summarized a connected peer: {status}"
        );
        tokio::time::sleep(POLL).await;
    }
}

async fn replicated_write_is_visible_per_peer(builder: TestClusterBuilder) {
    let cluster = builder.rust_nodes(2).build().await.unwrap();
    for node in 0..2 {
        cluster
            .wait_for_log(node, "p2p_listening", TIMEOUT)
            .await
            .unwrap_or_else(|_| panic!("node{node} P2P listener did not start"));
        cluster.client(node).schema_add(SCHEMA).expect("add schema");
        cluster
            .client(node)
            .p2p_collection_add(&["Users"])
            .expect("collection add");
    }
    let addr1 = integration_test::extract_p2p_addr(&cluster, 1);
    cluster.client(0).p2p_connect(&[&addr1]).expect("connect");
    cluster
        .client(0)
        .p2p_replicator_set(&["Users"], &addr1)
        .expect("replicator 0->1");
    cluster
        .client(0)
        .query(r#"mutation { add_Users(input: {name: "John", age: 21}) { _docID } }"#)
        .expect("create on node0");

    let node1 = cluster.client(1);
    poll_until(
        || {
            node1.query("query { Users { name } }").unwrap_or_default()["Users"]
                .as_array()
                .is_some_and(|users| !users.is_empty())
        },
        TIMEOUT,
        POLL,
        "doc did not reach node1",
    )
    .await;

    let receiver = connected_peer(&cluster, 0).await;
    let sender = connected_peer(&cluster, 1).await;
    let pushed_url = format!("{}/api/v0/p2p/peers/{receiver}", cluster.api_url(0));
    let deadline = tokio::time::Instant::now() + TIMEOUT;
    let pushed = loop {
        let details = get_json(pushed_url.clone()).await;
        if details["heads_pushed"]["succeeded"].as_u64() >= Some(1) {
            break details;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "node0 never recorded an acked head for node1: {details}"
        );
        tokio::time::sleep(POLL).await;
    };
    assert_eq!(pushed["peer_id"], receiver.as_str());
    assert_eq!(pushed["connected"], true);
    assert_eq!(pushed["connect_count"], 1);
    assert_eq!(pushed["connection"]["path"], "direct");
    assert!(pushed["connection"]["open_connections"].as_u64() >= Some(1));
    assert!(pushed["heads_pushed"]["last_succeeded_at"].is_string());
    assert_eq!(
        pushed["replicated_collections"].as_array().unwrap().len(),
        1
    );
    assert!(
        pushed["source_errors"].as_array().unwrap().is_empty(),
        "{pushed}"
    );

    let status = get_json(format!("{}/api/v0/p2p/sync/status", cluster.api_url(0))).await;
    let summary = status["peers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|peer| peer["peer_id"] == receiver.as_str())
        .expect("node1 is summarized on node0");
    assert_eq!(
        summary["last_head_acked_at"],
        pushed["heads_pushed"]["last_succeeded_at"]
    );

    let received = get_json(format!("{}/api/v0/p2p/peers/{sender}", cluster.api_url(1))).await;
    assert!(
        received["blocks_received"]["succeeded"].as_u64() >= Some(1),
        "{received}"
    );
    assert!(received["blocks_received"]["last_succeeded_at"].is_string());
    assert_eq!(received["blocks_received"]["failed"], 0);
    assert_eq!(received["pending_fetch_count"], 0);
}

#[tokio::test]
#[serial]
async fn libp2p_replicated_write_is_visible_per_peer() {
    replicated_write_is_visible_per_peer(TestCluster::builder().with_p2p()).await;
}

#[tokio::test]
#[serial]
async fn iroh_replicated_write_is_visible_per_peer() {
    replicated_write_is_visible_per_peer(TestCluster::builder().with_iroh_transport()).await;
}
