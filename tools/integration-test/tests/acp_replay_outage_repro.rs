//! Spike L2 repro: replay into a restarted peer under Local document ACP.
//!
//! Two Rust nodes, local document ACP, node0 replicates `User` to node1.
//! node1 is stopped, node0 writes N protected (owned by Alice) and N public
//! (anonymous) documents, node1 is restarted. Every document must arrive.
//!
//! Characterises `resolve_push_creator` (`crates/db/src/merge/push_docs_creator.rs:59`),
//! which the replay and retry paths call and which has no fallback creator once
//! the collection carries a `@policy`.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use integration_test::{
    generate_identity, poll_until, users_schema_with_policy, TestCluster, USER_ACP_POLICY,
};

const N: usize = 3;
const ARRIVAL_BUDGET: Duration = Duration::from_secs(180);

fn log_dir(cluster: &TestCluster, index: usize) -> PathBuf {
    cluster.nodes[index]
        .rootdir
        .parent()
        .expect("node dir")
        .join("logs")
}

fn log_lines_matching(cluster: &TestCluster, index: usize, needle: &str) -> Vec<String> {
    let dir = log_dir(cluster, index);
    let mut hits = Vec::new();
    for file in ["stdout.log", "stderr.log"] {
        let Ok(content) = std::fs::read_to_string(dir.join(file)) else {
            continue;
        };
        hits.extend(
            content
                .lines()
                .filter(|line| line.contains(needle))
                .map(str::to_string),
        );
    }
    hits
}

fn names_on(cluster: &TestCluster, index: usize, key: &str) -> Vec<String> {
    cluster
        .client(index)
        .query_with_identity("query { User { name } }", key)
        .ok()
        .and_then(|v| {
            v["User"].as_array().map(|arr| {
                arr.iter()
                    .filter_map(|u| u["name"].as_str().map(str::to_string))
                    .collect()
            })
        })
        .unwrap_or_default()
}

#[tokio::test]
async fn protected_and_public_docs_replay_into_a_restarted_peer() {
    let mut cluster = TestCluster::builder()
        .rust_nodes(2)
        .with_store("regolith")
        .with_keyring()
        .with_p2p()
        .with_acp_local()
        // With a keyring the node signs anonymous writes with its own DID, and
        // the receiver's explicit replay authorization check then rejects them
        // because the replicator's authorizer is Alice. That is a separate
        // defect on main; this test is about the retry path, so it runs unsigned.
        .no_signing_multiplier()
        .build()
        .await
        .expect("cluster start");

    let startup = Duration::from_secs(30);
    for node in 0..2 {
        cluster
            .wait_for_log(node, "p2p_listening", startup)
            .await
            .unwrap_or_else(|e| panic!("node{node} P2P listener did not start: {e}"));
    }

    let node0 = cluster.client(0);
    let alice = generate_identity(node0.binary_path()).expect("alice identity");
    let key = alice.private_key_hex.clone();

    let addr1 = cluster
        .client(1)
        .p2p_info()
        .expect("node1 p2p info")
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|v| v.as_str())
        .expect("node1 p2p address")
        .to_string();

    let policy_id = node0
        .acp_policy_add(USER_ACP_POLICY, &key)
        .expect("policy on node0")["PolicyID"]
        .as_str()
        .expect("PolicyID")
        .to_string();
    cluster
        .client(1)
        .acp_policy_add(USER_ACP_POLICY, &key)
        .expect("policy on node1");

    let schema = users_schema_with_policy(&policy_id);
    node0
        .schema_add_with_identity(&schema, &key)
        .expect("schema on node0");
    cluster
        .client(1)
        .schema_add_with_identity(&schema, &key)
        .expect("schema on node1");

    node0.p2p_connect(&[&addr1]).expect("connect");
    node0
        .p2p_collection_add(&["User"])
        .expect("subscribe node0");
    cluster
        .client(1)
        .p2p_collection_add(&["User"])
        .expect("subscribe node1");
    node0
        .p2p_replicator_set_with_identity(&["User"], &addr1, &key)
        .expect("replicator");

    // Sanity: the live path works while the peer is up.
    node0
        .query_with_identity(
            r#"mutation { add_User(input: {name: "LiveProtected", age: 1}) { _docID } }"#,
            &key,
        )
        .expect("live protected write");
    node0
        .query(r#"mutation { add_User(input: {name: "LivePublic", age: 2}) { _docID } }"#)
        .expect("live public write");
    let live_key = key.clone();
    poll_until(
        || {
            cluster
                .client(1)
                .query_with_identity("query { User { name } }", &live_key)
                .ok()
                .and_then(|v| v["User"].as_array().map(|arr| arr.len() >= 2))
                .unwrap_or(false)
        },
        Duration::from_secs(30),
        Duration::from_millis(200),
        "live documents did not replicate before the outage",
    )
    .await;

    let stopped = cluster.stop_node(1).await.expect("stop node1");

    let mut expected: Vec<String> = vec!["LiveProtected".into(), "LivePublic".into()];
    for i in 0..N {
        let protected = format!("OutageProtected{i}");
        node0
            .query_with_identity(
                &format!(
                    r#"mutation {{ add_User(input: {{name: "{protected}", age: {}}}) {{ _docID }} }}"#,
                    100 + i
                ),
                &key,
            )
            .unwrap_or_else(|e| panic!("protected write {i}: {e}"));
        expected.push(protected);

        let public = format!("OutagePublic{i}");
        node0
            .query(&format!(
                r#"mutation {{ add_User(input: {{name: "{public}", age: {}}}) {{ _docID }} }}"#,
                200 + i
            ))
            .unwrap_or_else(|e| panic!("public write {i}: {e}"));
        expected.push(public);
    }

    cluster
        .start_stopped_node(stopped, Duration::from_secs(60))
        .await
        .expect("restart node1");

    let deadline = Instant::now() + ARRIVAL_BUDGET;
    let mut arrived = Vec::new();
    while Instant::now() < deadline {
        arrived = names_on(&cluster, 1, &key);
        if arrived.len() >= expected.len() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    let missing: Vec<&String> = expected.iter().filter(|n| !arrived.contains(n)).collect();
    if !missing.is_empty() {
        let owner_missing = log_lines_matching(&cluster, 0, "ACP owner is missing");
        let skipped = log_lines_matching(&cluster, 0, "Skipping existing document replay");
        let retry_failed = log_lines_matching(&cluster, 0, "retry push failed");
        panic!(
            "missing on node1 after {}s: {missing:?}\narrived: {arrived:?}\n\
             node0 'ACP owner is missing' lines: {}\n{}\n\
             node0 'Skipping existing document replay' lines: {}\n{}\n\
             node0 'retry push failed' lines: {}\n{}",
            ARRIVAL_BUDGET.as_secs(),
            owner_missing.len(),
            owner_missing.join("\n"),
            skipped.len(),
            skipped.join("\n"),
            retry_failed.len(),
            retry_failed.join("\n"),
        );
    }
}
