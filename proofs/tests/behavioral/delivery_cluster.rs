//! The cluster, writes and reads the delivery-liveness scenarios share.
//!
//! Every node is a Rust iroh node on a regolith store with a file keyring, so
//! a restart keeps its documents, its subscriptions and its peer id.

use crate::support;
use defra_harness::{extract_peer_id, DefraClient, TestCluster, TestClusterBuilder};
use serde_json::Value;
use std::collections::BTreeSet;
use std::time::{Duration, Instant};

pub const COLLECTION: &str = "AgentRequest";
const SCHEMA: &str = "type AgentRequest { name: String status: String note: String }";
pub const START_TIMEOUT: Duration = Duration::from_secs(60);
const POLL: Duration = Duration::from_millis(250);
const UPDATES_PER_REQUEST: usize = 50;

/// `nodes` nodes, each with the schema and subscribed to the collection.
/// `configure` adds per-scenario flags before the cluster starts.
pub async fn start<F>(nodes: usize, configure: F) -> TestCluster
where
    F: FnOnce(TestClusterBuilder) -> TestClusterBuilder,
{
    let builder = TestCluster::builder()
        .rust_nodes(nodes)
        .with_iroh_transport()
        .with_store("regolith")
        .with_file_keyring()
        .with_rust_binary(support::iroh_binary());
    let cluster = configure(builder)
        .build()
        .await
        .expect("start the delivery cluster");
    for node in 0..nodes {
        cluster
            .wait_for_log(node, "p2p_listening", START_TIMEOUT)
            .await
            .unwrap_or_else(|e| panic!("node{node} iroh transport did not start: {e}"));
        cluster.client(node).schema_add(SCHEMA).expect("schema");
        cluster
            .client(node)
            .p2p_collection_add(&[COLLECTION])
            .expect("subscribe");
    }
    cluster
}

/// The address peers dial: the endpoint ticket, which carries every direct
/// address of the node, so a dial never depends on which one routes.
pub fn addr(cluster: &TestCluster, node: usize) -> String {
    let info = cluster.client(node).p2p_info().expect("p2p info");
    let addrs: Vec<&str> = info
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    addrs
        .iter()
        .find(|a| a.starts_with("endpoint"))
        .or_else(|| addrs.first())
        .unwrap_or_else(|| panic!("node{node} has no P2P address"))
        .to_string()
}

pub fn peer_id(cluster: &TestCluster, node: usize) -> String {
    extract_peer_id(cluster, node)
}

pub fn connect(cluster: &TestCluster, from: usize, to: usize) {
    cluster
        .client(from)
        .p2p_connect(&[addr(cluster, to).as_str()])
        .unwrap_or_else(|e| panic!("node{from} connect node{to}: {e}"));
}

pub fn replicate(cluster: &TestCluster, from: usize, to: usize) {
    cluster
        .client(from)
        .p2p_replicator_set(&[COLLECTION], &addr(cluster, to))
        .unwrap_or_else(|e| panic!("replicator node{from} -> node{to}: {e}"));
}

/// A new document: one composite over three field commits.
pub fn create(node: &DefraClient, name: &str) -> String {
    let created = node
        .query(&format!(
            r#"mutation {{ add_AgentRequest(input: {{name: "{name}", status: "new", note: "new"}}) {{ _docID }} }}"#
        ))
        .expect("create AgentRequest");
    created["add_AgentRequest"][0]["_docID"]
        .as_str()
        .expect("_docID")
        .to_string()
}

pub fn set(node: &DefraClient, doc_id: &str, field: &str, value: &str) {
    node.query(&format!(
        r#"mutation {{ update_AgentRequest(docID: "{doc_id}", input: {{{field}: "{value}"}}) {{ _docID }} }}"#
    ))
    .unwrap_or_else(|e| panic!("set {field}={value} on {doc_id}: {e}"));
}

/// Grow the document's history by `updates` commits, one per update, in
/// requests of `UPDATES_PER_REQUEST` aliased updates.
pub fn churn(node: &DefraClient, doc_id: &str, updates: usize) {
    for start in (0..updates).step_by(UPDATES_PER_REQUEST) {
        let body: String = (start..(start + UPDATES_PER_REQUEST).min(updates))
            .map(|n| {
                format!(
                    r#"u{n}: update_AgentRequest(docID: "{doc_id}", input: {{note: "{n}"}}) {{ _docID }} "#
                )
            })
            .collect();
        node.query(&format!("mutation {{ {body}}}"))
            .unwrap_or_else(|e| panic!("churn {doc_id}: {e}"));
    }
}

/// The document's row, or null when the node does not have it.
pub fn row(node: &DefraClient, doc_id: &str) -> Value {
    node.query(&format!(
        r#"query {{ AgentRequest(docID: "{doc_id}") {{ _docID name status note }} }}"#
    ))
    .unwrap_or_default()["AgentRequest"][0]
        .clone()
}

/// The document's composite heads: its `_C` commits at depth 1.
pub fn heads(node: &DefraClient, doc_id: &str) -> BTreeSet<String> {
    let commits = node
        .query(&format!(
            r#"query {{ _commits(docID: "{doc_id}", depth: 1) {{ cid fieldName }} }}"#
        ))
        .unwrap_or_default();
    commits["_commits"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|c| c["fieldName"].as_str() == Some("_C"))
                .filter_map(|c| c["cid"].as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// `node` holds exactly the commit DAG `source` holds for the document.
pub fn holds(node: &DefraClient, source: &DefraClient, doc_id: &str) -> bool {
    let want = support::commit_cids(source, doc_id);
    !want.is_empty() && support::commit_cids(node, doc_id) == want
}

pub async fn sync_status(cluster: &TestCluster, node: usize) -> Value {
    cluster
        .p2p_sync_status(node)
        .await
        .unwrap_or_else(|e| panic!("node{node} sync status: {e}"))
}

pub fn count(value: &Value, field: &str) -> u64 {
    value[field].as_u64().unwrap_or(0)
}

/// Durable push-retry markers `node` holds for `peer`: heads it still owes.
pub async fn markers_for(cluster: &TestCluster, node: usize, peer: &str) -> u64 {
    let details = cluster
        .p2p_peer_details(node, peer)
        .await
        .unwrap_or_else(|e| panic!("node{node} details of {peer}: {e}"));
    count(&details["push_retries"], "documents") + count(&details["push_retries"], "collections")
}

/// Heads `node` has pushed to `peer` and seen acknowledged.
pub async fn heads_acked_by(cluster: &TestCluster, node: usize, peer: &str) -> u64 {
    let details = cluster
        .p2p_peer_details(node, peer)
        .await
        .unwrap_or_else(|e| panic!("node{node} details of {peer}: {e}"));
    count(&details["heads_pushed"], "succeeded")
}

/// DAG fetches `node` has in flight to `peer`.
pub async fn fetches_in_flight_to(cluster: &TestCluster, node: usize, peer: &str) -> u64 {
    let status = sync_status(cluster, node).await;
    status["peer_fetches"]
        .as_array()
        .and_then(|peers| peers.iter().find(|p| p["peer"].as_str() == Some(peer)))
        .map(|p| count(p, "in_flight"))
        .unwrap_or(0)
}

/// Pending roots `node` holds whose source is `peer`.
pub async fn pending_from(cluster: &TestCluster, node: usize, peer: &str) -> u64 {
    let details = cluster
        .p2p_peer_details(node, peer)
        .await
        .unwrap_or_else(|e| panic!("node{node} details of {peer}: {e}"));
    count(&details, "pending_fetch_count")
}

/// Poll `check` until it holds or `timeout` passes; whether it held.
pub async fn eventually<F>(timeout: Duration, mut check: F) -> bool
where
    F: FnMut() -> bool,
{
    let deadline = Instant::now() + timeout;
    loop {
        if check() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        nuclei::time::sleep(POLL).await;
    }
}
