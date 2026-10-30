//! An index on a field whose value is merged rather than written follows the
//! merged value, after local edits and after peers' edits replicate. An index
//! on a counter catches up after the write, and reads in between still see
//! the current total.

use std::time::{Duration, Instant};

use integration_test::{DefraClient, TestCluster};
use serde_json::Value;

fn address(info: Value) -> String {
    info.as_array()
        .and_then(|addrs| addrs.first())
        .and_then(|addr| addr.as_str())
        .expect("node has a P2P address")
        .to_string()
}

fn index_fetches(value: &Value) -> u64 {
    match value {
        Value::Array(values) => values.iter().map(index_fetches).sum(),
        Value::Object(values) => values
            .iter()
            .map(|(key, value)| match (key.as_str(), value.as_u64()) {
                ("indexFetches", Some(count)) => count,
                _ => index_fetches(value),
            })
            .sum(),
        _ => 0,
    }
}

/// The Notes `filter` finds, and how many index entries the query read. A
/// stale entry shows up as a fetch even when the document read back no
/// longer matches.
fn find(node: &DefraClient, filter: &str) -> (usize, u64) {
    let query = format!("query {{ Note(filter: {filter}) {{ _docID }} }}");
    let found = node.query(&query).unwrap()["Note"]
        .as_array()
        .map_or(0, Vec::len);
    let explain = node
        .query(&query.replacen("query", "query @explain(type: execute)", 1))
        .unwrap();
    (found, index_fetches(&explain))
}

fn update(node: &DefraClient, doc_id: &str, input: &str) {
    node.query(&format!(
        r#"mutation {{ update_Note(docID: "{doc_id}", input: {input}) {{ _docID }} }}"#
    ))
    .unwrap();
}

/// Poll `node` until the filter finds its one Note.
async fn wait_for(node: &DefraClient, filter: &str) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        let found = node.query(&format!("query {{ Note(filter: {filter}) {{ _docID }} }}"));
        if found.is_ok_and(|found| {
            found["Note"]
                .as_array()
                .is_some_and(|notes| notes.len() == 1)
        }) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    let field = filter
        .trim_start_matches('{')
        .split(':')
        .next()
        .unwrap_or("_docID");
    let all = node
        .query(&format!("query {{ Note {{ {field} }} }}"))
        .map_or_else(|error| error.to_string(), |all| all.to_string());
    panic!("no Note matched {filter} in time; Notes: {all}");
}

/// Two connected peers that both have `schema`, a Note created on the first
/// and replicated to the second, and replication stopped so they edit apart.
async fn apart(schema: &str, input: &str, created: &str) -> (TestCluster, String, String, String) {
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
    alice.schema_add(schema).unwrap();
    bob.schema_add(schema).unwrap();
    alice.p2p_connect(&[bob_addr.as_str()]).unwrap();
    let doc_id = alice
        .query(&format!(
            "mutation {{ add_Note(input: {input}) {{ _docID }} }}"
        ))
        .unwrap()["add_Note"][0]["_docID"]
        .as_str()
        .expect("created Note has a _docID")
        .to_string();
    alice.p2p_replicator_set(&["Note"], &bob_addr).unwrap();
    wait_for(&bob, created).await;
    alice
        .p2p_replicator_delete(&["Note"], Some(bob_addr.as_str()))
        .expect("stop replicating while the peers edit apart");
    (cluster, doc_id, alice_addr, bob_addr)
}

async fn rejoin(cluster: &TestCluster, alice_addr: &str, bob_addr: &str) {
    cluster
        .client(0)
        .p2p_replicator_set(&["Note"], bob_addr)
        .unwrap();
    cluster
        .client(1)
        .p2p_replicator_set(&["Note"], alice_addr)
        .unwrap();
}

#[tokio::test]
async fn a_text_index_follows_local_edits() {
    let cluster = TestCluster::builder().rust_nodes(1).build().await.unwrap();
    let node = cluster.client(0);
    node.schema_add("type Note { body: String @crdt(type: text) @index }")
        .unwrap();
    let doc_id = node
        .query(r#"mutation { add_Note(input: {body: "hello world"}) { _docID } }"#)
        .unwrap()["add_Note"][0]["_docID"]
        .as_str()
        .unwrap()
        .to_string();
    update(&node, &doc_id, r#"{body: "hello brave world"}"#);

    let (found, fetches) = find(&node, r#"{body: {_eq: "hello brave world"}}"#);
    assert_eq!((found, fetches), (1, 1), "the edited text is indexed");
    let (found, fetches) = find(&node, r#"{body: {_eq: "hello world"}}"#);
    assert_eq!((found, fetches), (0, 0), "the old text left the index");
}

#[tokio::test]
async fn a_text_index_follows_merged_edits() {
    let (cluster, doc_id, alice_addr, bob_addr) = apart(
        "type Note { body: String @crdt(type: text) @index }",
        r#"{body: "hello world"}"#,
        r#"{body: {_eq: "hello world"}}"#,
    )
    .await;
    let (alice, bob) = (cluster.client(0), cluster.client(1));
    update(&alice, &doc_id, r#"{body: "hello brave world"}"#);
    update(&bob, &doc_id, r#"{body: "hello world!"}"#);
    rejoin(&cluster, &alice_addr, &bob_addr).await;

    let merged = r#"{body: {_eq: "hello brave world!"}}"#;
    for (name, node) in [("alice", &alice), ("bob", &bob)] {
        wait_for(node, merged).await;
        assert_eq!(
            find(node, merged),
            (1, 1),
            "{name}: the merged text is indexed"
        );
        for stale in ["hello world", "hello brave world", "hello world!"] {
            let filter = format!(r#"{{body: {{_eq: "{stale}"}}}}"#);
            assert_eq!(
                find(node, &filter),
                (0, 0),
                "{name}: {stale:?} left the index"
            );
        }
    }
}

#[tokio::test]
async fn a_json_index_follows_merged_edits() {
    let (cluster, doc_id, alice_addr, bob_addr) = apart(
        "type Note { meta: JSON @crdt(type: json) @index }",
        r#"{meta: {a: 1, b: 1}}"#,
        "{meta: {a: {_eq: 1}}}",
    )
    .await;
    let (alice, bob) = (cluster.client(0), cluster.client(1));
    update(&alice, &doc_id, "{meta: {a: 2, b: 1}}");
    update(&bob, &doc_id, "{meta: {a: 1, b: 2}}");
    rejoin(&cluster, &alice_addr, &bob_addr).await;

    let merged = "{meta: {a: {_eq: 2}, b: {_eq: 2}}}";
    for (name, node) in [("alice", &alice), ("bob", &bob)] {
        wait_for(node, merged).await;
        for path in ["a", "b"] {
            let filter = format!("{{meta: {{{path}: {{_eq: 2}}}}}}");
            assert_eq!(
                find(node, &filter),
                (1, 1),
                "{name}: merged {path} is indexed"
            );
            let filter = format!("{{meta: {{{path}: {{_eq: 1}}}}}}");
            assert_eq!(
                find(node, &filter),
                (0, 0),
                "{name}: old {path} left the index"
            );
        }
    }
}

#[tokio::test]
async fn a_counter_index_follows_local_increments() {
    let cluster = TestCluster::builder().rust_nodes(1).build().await.unwrap();
    let node = cluster.client(0);
    node.schema_add("type Note { count: Int @crdt(type: pcounter) @index }")
        .unwrap();
    let doc_id = node
        .query("mutation { add_Note(input: {count: 1}) { _docID } }")
        .unwrap()["add_Note"][0]["_docID"]
        .as_str()
        .unwrap()
        .to_string();
    update(&node, &doc_id, "{count: 2}");
    update(&node, &doc_id, "{count: 3}");

    assert_eq!(
        find(&node, "{count: {_eq: 6}}"),
        (1, 1),
        "the total is indexed"
    );
    assert_eq!(
        find(&node, "{count: {_gt: 4}}").0,
        1,
        "a range finds the total"
    );
    for stale in [1, 3] {
        let filter = format!("{{count: {{_eq: {stale}}}}}");
        assert_eq!(find(&node, &filter), (0, 0), "{stale} left the index");
    }
}

#[tokio::test]
async fn a_counter_index_follows_merged_increments() {
    let (cluster, doc_id, alice_addr, bob_addr) = apart(
        "type Note { count: Int @crdt(type: pcounter) @index }",
        "{count: 1}",
        "{count: {_eq: 1}}",
    )
    .await;
    let (alice, bob) = (cluster.client(0), cluster.client(1));
    update(&alice, &doc_id, "{count: 2}");
    update(&bob, &doc_id, "{count: 3}");
    rejoin(&cluster, &alice_addr, &bob_addr).await;

    let merged = "{count: {_eq: 6}}";
    for (name, node) in [("alice", &alice), ("bob", &bob)] {
        wait_for(node, merged).await;
        assert_eq!(
            find(node, merged),
            (1, 1),
            "{name}: the merged total is indexed"
        );
        for stale in [1, 3, 4] {
            let filter = format!("{{count: {{_eq: {stale}}}}}");
            assert_eq!(
                find(node, &filter),
                (0, 0),
                "{name}: {stale} left the index"
            );
        }
    }
}
