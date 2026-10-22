//! Two Rust peers edit one Json field while apart, then replicate: both end
//! at the same value, which keeps every edit no concurrent one beat at its
//! path, and both list the edits that lost in `_jsonConflicts`.

use std::time::{Duration, Instant};

use integration_test::{DefraClient, TestCluster};
use serde_json::{json, Value};

const SCHEMA: &str = "type Note { meta: JSON @crdt(type: json) }";

fn address(info: Value) -> String {
    info.as_array()
        .and_then(|addrs| addrs.first())
        .and_then(|addr| addr.as_str())
        .expect("node has a P2P address")
        .to_string()
}

/// Two connected peers, a Note created on the first with `meta` and
/// replicated to the second, and replication stopped so they edit apart.
struct Apart {
    cluster: TestCluster,
    doc_id: String,
    alice_addr: String,
    bob_addr: String,
}

impl Apart {
    async fn new(meta: &str) -> Self {
        let cluster = TestCluster::builder()
            .rust_nodes(2)
            .with_p2p()
            .build()
            .await
            .unwrap();
        for node in [0, 1] {
            cluster
                .wait_for_log(node, "p2p_listening", Duration::from_secs(15))
                .await
                .expect("P2P listener started");
        }
        let alice = cluster.client(0);
        let bob = cluster.client(1);
        let alice_addr = address(alice.p2p_info().unwrap());
        let bob_addr = address(bob.p2p_info().unwrap());
        alice.schema_add(SCHEMA).unwrap();
        bob.schema_add(SCHEMA).unwrap();
        alice.p2p_connect(&[bob_addr.as_str()]).unwrap();
        let created = alice
            .query(&format!(
                r#"mutation {{ add_Note(input: {{meta: {meta}}}) {{ _docID meta }} }}"#
            ))
            .unwrap();
        let doc_id = created["add_Note"][0]["_docID"]
            .as_str()
            .expect("created Note has a _docID")
            .to_string();
        let expected = created["add_Note"][0]["meta"].clone();
        alice.p2p_replicator_set(&["Note"], &bob_addr).unwrap();
        wait_for(&bob, |meta| *meta == expected).await;
        alice
            .p2p_replicator_delete(&["Note"], Some(bob_addr.as_str()))
            .expect("stop replicating while the peers edit apart");
        Self {
            cluster,
            doc_id,
            alice_addr,
            bob_addr,
        }
    }

    fn peer(&self, node: usize) -> DefraClient {
        self.cluster.client(node)
    }

    fn update(&self, node: usize, input: &str) {
        self.peer(node)
            .query(&format!(
                r#"mutation {{ update_Note(docID: "{}", input: {input}) {{ meta }} }}"#,
                self.doc_id
            ))
            .unwrap_or_else(|error| panic!("update {input} failed: {error}"));
    }

    /// Replicate both ways, and wait until both peers read the same value
    /// and that value passes `done`. Returns it.
    async fn converge<F>(&self, done: F) -> Value
    where
        F: Fn(&Value) -> bool,
    {
        let (alice, bob) = (self.peer(0), self.peer(1));
        alice.p2p_replicator_set(&["Note"], &self.bob_addr).unwrap();
        bob.p2p_replicator_set(&["Note"], &self.alice_addr).unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut last = (Value::Null, Value::Null);
        while Instant::now() < deadline {
            last = (meta(&alice), meta(&bob));
            if last.0 == last.1 && done(&last.0) {
                return last.0;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        panic!("peers did not converge in time; last values {last:?}");
    }

    fn conflicts(&self, node: usize) -> Value {
        self.peer(node)
            .query(&format!(
                r#"query {{ _jsonConflicts(collection: "Note", docID: "{}", field: "meta") {{ path cid height value deleted }} }}"#,
                self.doc_id
            ))
            .unwrap()["_jsonConflicts"]
            .clone()
    }
}

fn meta(node: &DefraClient) -> Value {
    node.query("query { Note { meta } }").unwrap()["Note"][0]["meta"].clone()
}

/// Poll `node` until its one Note's meta passes `done`.
async fn wait_for<F>(node: &DefraClient, done: F)
where
    F: Fn(&Value) -> bool,
{
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut last = Value::Null;
    while Instant::now() < deadline {
        last = meta(node);
        if done(&last) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    panic!("meta did not arrive in time; last {last:?}");
}

#[tokio::test]
async fn peers_editing_different_keys_keep_both_edits() {
    let apart = Apart::new(r#"{a: 1, b: 1, nested: {x: 1}}"#).await;
    apart.update(0, r#"{set: {meta: {a: 2}}}"#);
    apart.update(1, r#"{set: {meta: {nested: {y: 3}}}}"#);
    let value = apart.converge(|meta| meta["nested"]["y"] == 3).await;
    assert_eq!(value, json!({"a": 2, "b": 1, "nested": {"x": 1, "y": 3}}));
    assert_eq!(apart.conflicts(0), json!([]));
    assert_eq!(apart.conflicts(1), json!([]));
}

#[tokio::test]
async fn a_whole_value_write_only_changes_the_keys_it_changes() {
    let apart = Apart::new(r#"{a: 1, b: 1}"#).await;
    apart.update(0, r#"{meta: {a: 1, b: 2}}"#);
    apart.update(1, r#"{meta: {a: 5, b: 1, c: true}}"#);
    let value = apart.converge(|meta| meta.get("c").is_some()).await;
    assert_eq!(value, json!({"a": 5, "b": 2, "c": true}));
}

#[tokio::test]
async fn an_update_below_beats_a_concurrent_delete() {
    let apart = Apart::new(r#"{a: {b: 1, c: 1}, d: 1}"#).await;
    apart.update(0, r#"{delete: {meta: ["a"]}}"#);
    apart.update(1, r#"{set: {meta: {a: {b: 2}}}}"#);
    let value = apart.converge(|meta| meta["a"]["b"] == 2).await;
    assert_eq!(value, json!({"a": {"b": 2}, "d": 1}));
    for node in [0, 1] {
        let conflicts = apart.conflicts(node);
        assert_eq!(conflicts.as_array().map(Vec::len), Some(1), "{conflicts}");
        assert_eq!(conflicts[0]["path"], json!(["a"]));
        assert_eq!(conflicts[0]["deleted"], json!(true));
    }
}

#[tokio::test]
async fn a_type_clash_resolves_one_way_on_both_peers() {
    let apart = Apart::new(r#"{a: {b: 1}}"#).await;
    apart.update(0, r#"{set: {meta: {a: 5}}}"#);
    apart.update(1, r#"{set: {meta: {a: {c: 2}}}}"#);
    // Setting `a` replaced its map, `b` included, so a winning map holds
    // only what the other peer set below it.
    let value = apart
        .converge(|meta| *meta == json!({"a": 5}) || *meta == json!({"a": {"c": 2}}))
        .await;
    let conflicts = apart.conflicts(0);
    assert_eq!(conflicts, apart.conflicts(1));
    assert_eq!(conflicts.as_array().map(Vec::len), Some(1), "{conflicts}");
    assert_eq!(conflicts[0]["path"], json!(["a"]));
    let lost_map = value == json!({"a": 5});
    assert_eq!(conflicts[0]["value"] == json!({}), lost_map, "{conflicts}");
}

#[tokio::test]
async fn racing_writes_of_one_path_list_the_loser_with_its_block() {
    let apart = Apart::new(r#"{a: 1, b: 1}"#).await;
    apart.update(0, r#"{set: {meta: {a: "alice"}}}"#);
    apart.update(1, r#"{set: {meta: {a: "bob"}}}"#);
    let value = apart.converge(|meta| meta["a"] != 1).await;
    let winner = value["a"].as_str().expect("a is a string").to_string();
    let loser = if winner == "alice" { "bob" } else { "alice" };
    for node in [0, 1] {
        let conflicts = apart.conflicts(node);
        assert_eq!(conflicts.as_array().map(Vec::len), Some(1), "{conflicts}");
        assert_eq!(conflicts[0]["path"], json!(["a"]));
        assert_eq!(conflicts[0]["value"], json!(loser));
        assert_eq!(conflicts[0]["height"], json!(2));
        assert_eq!(conflicts[0]["deleted"], json!(false));
        let cid = conflicts[0]["cid"]
            .as_str()
            .expect("the loser's block is named");
        let commit = apart
            .peer(node)
            .query(&format!(
                r#"query {{ _commits(cid: "{cid}") {{ fieldName }} }}"#
            ))
            .unwrap();
        assert_eq!(commit["_commits"][0]["fieldName"], json!("meta"));
    }
}
