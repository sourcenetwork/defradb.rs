//! Spike L2, Go-side verification: does a replicator replay documents written
//! while its peer was down, and does the answer depend on the runtime?
//!
//! Companion to `acp_replay_outage_repro.rs`, which is Rust→Rust only. Here the
//! same outage shape is run across the runtime boundary so the "parity with Go"
//! claim in report 124 rests on measurement rather than on reading
//! `internal/db/p2p/replicator.go:887` (Go sets `Creator: p.host.ID()`
//! unconditionally on every push, including the retry push).
//!
//! Four cases, one per test:
//!   A  go   → go     the pure-Go baseline
//!   B  go   → rust   Go pushes with its host peer id as creator
//!   C  rust → go     the mirror of 124's repro with a Go receiver
//!   D  go   → rust   → rust, the forwarding case (Evidence 3 of report 124)
//!
//! `SUBSCRIBE` controls whether the nodes also `p2p collection add`. Off, the
//! replicator is the only delivery path — 124's repro subscribed both nodes,
//! which its verifier flagged as a second path. It defaults on because a Rust →
//! Go live push hangs without it (see the report); set
//! `DEFRA_L2_NO_SUBSCRIBE=1` to turn it off.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use integration_test::{
    generate_identity, poll_until, users_schema_with_policy, BinarySource, TestCluster,
    USER_ACP_POLICY,
};

/// Both runtimes accept `--replicator-retry-intervals`; the stock ladder starts
/// at 30 s and doubles to 32 min, which no test budget can wait out.
const RETRY_INTERVALS: &str = "5,10,20,40";

const ARRIVAL_BUDGET: Duration = Duration::from_secs(150);
const LIVE_BUDGET: Duration = Duration::from_secs(45);
const STARTUP: Duration = Duration::from_secs(45);

/// Log needles that say what a sender did with a document it could not push.
/// Rust's first three, Go's last four.
const SENDER_NEEDLES: [&str; 7] = [
    "ACP owner is missing",
    "retry push failed",
    "Skipping existing document replay",
    "Failed pushing log",
    "Failed to retry doc",
    "Failed to push doc heads",
    "Failed to handle replicator failure",
];

fn go_binary() -> BinarySource {
    let path = std::env::var("DEFRA_GO_BINARY").unwrap_or_else(|_| {
        format!(
            "{}/.cache/defra-harness/53f0e76a3/defradb",
            std::env::var("HOME").expect("HOME")
        )
    });
    BinarySource::Path(PathBuf::from(path))
}

fn log_lines_matching(cluster: &TestCluster, index: usize, needle: &str) -> Vec<String> {
    let dir = cluster.nodes[index]
        .rootdir
        .parent()
        .expect("node dir")
        .join("logs");
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

fn report_sender_logs(cluster: &TestCluster, index: usize, label: &str) {
    for needle in SENDER_NEEDLES {
        let hits = log_lines_matching(cluster, index, needle);
        if hits.is_empty() {
            continue;
        }
        println!("[{label}] node{index} {:>3} × {needle:?}", hits.len());
        for line in hits.iter().take(4) {
            println!("[{label}]     {line}");
        }
    }
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

async fn await_names(cluster: &TestCluster, index: usize, key: &str, want: usize) -> Vec<String> {
    let deadline = Instant::now() + ARRIVAL_BUDGET;
    let mut arrived = names_on(cluster, index, key);
    while Instant::now() < deadline && arrived.len() < want {
        tokio::time::sleep(Duration::from_millis(500)).await;
        arrived = names_on(cluster, index, key);
    }
    arrived
}

/// Deploy the policy and the policied `User` schema on every node, then wire
/// `from` to replicate `User` to `to`. Returns Alice's private key hex.
fn subscribe_enabled() -> bool {
    std::env::var("DEFRA_L2_NO_SUBSCRIBE").as_deref() != Ok("1")
}

async fn setup(cluster: &TestCluster, links: &[(usize, usize)], label: &str) -> String {
    for index in 0..cluster.len() {
        cluster
            .wait_for_log(index, "p2p_listening", STARTUP)
            .await
            .unwrap_or_else(|e| panic!("[{label}] node{index} P2P listener did not start: {e}"));
    }

    let alice = generate_identity(cluster.client(0).binary_path()).expect("alice identity");
    let key = alice.private_key_hex;

    let policy_id = cluster
        .client(0)
        .acp_policy_add(USER_ACP_POLICY, &key)
        .unwrap_or_else(|e| panic!("[{label}] policy on node0: {e}"))["PolicyID"]
        .as_str()
        .expect("PolicyID")
        .to_string();
    let schema = users_schema_with_policy(&policy_id);

    for index in 0..cluster.len() {
        if index > 0 {
            let id = cluster
                .client(index)
                .acp_policy_add(USER_ACP_POLICY, &key)
                .unwrap_or_else(|e| panic!("[{label}] policy on node{index}: {e}"))["PolicyID"]
                .as_str()
                .expect("PolicyID")
                .to_string();
            assert_eq!(
                id, policy_id,
                "[{label}] node{index} derived a different policy id"
            );
        }
        cluster
            .client(index)
            .schema_add_with_identity(&schema, &key)
            .unwrap_or_else(|e| panic!("[{label}] schema on node{index}: {e}"));
    }

    if subscribe_enabled() {
        for index in 0..cluster.len() {
            cluster
                .client(index)
                .p2p_collection_add(&["User"])
                .unwrap_or_else(|e| panic!("[{label}] subscribe node{index}: {e}"));
        }
    }

    for &(from, to) in links {
        let addr = cluster
            .client(to)
            .p2p_info()
            .unwrap_or_else(|e| panic!("[{label}] node{to} p2p info: {e}"))
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| panic!("[{label}] node{to} has no p2p address"))
            .to_string();
        cluster
            .client(from)
            .p2p_connect(&[&addr])
            .unwrap_or_else(|e| panic!("[{label}] connect node{from}→node{to}: {e}"));
        cluster
            .client(from)
            .p2p_replicator_set_with_identity(&["User"], &addr, &key)
            .unwrap_or_else(|e| panic!("[{label}] replicator node{from}→node{to}: {e}"));
    }

    key
}

fn write_pair(cluster: &TestCluster, writer: usize, suffix: &str, key: &str, label: &str) {
    cluster
        .client(writer)
        .query_with_identity(
            &format!(
                r#"mutation {{ add_User(input: {{name: "{suffix}Protected", age: 1}}) {{ _docID }} }}"#
            ),
            key,
        )
        .unwrap_or_else(|e| panic!("[{label}] {suffix} protected write: {e}"));
    cluster
        .client(writer)
        .query(&format!(
            r#"mutation {{ add_User(input: {{name: "{suffix}Public", age: 2}}) {{ _docID }} }}"#
        ))
        .unwrap_or_else(|e| panic!("[{label}] {suffix} public write: {e}"));
}

/// Prove the live path works, take `receiver` down, write one protected and one
/// public document, bring it back, and report which of the four arrived.
async fn outage_replay(
    cluster: &mut TestCluster,
    writer: usize,
    receiver: usize,
    key: &str,
    label: &str,
) -> Vec<String> {
    write_pair(cluster, writer, "Live", key, label);
    {
        let (c, k) = (&*cluster, key.to_string());
        poll_until(
            || names_on(c, receiver, &k).len() >= 2,
            LIVE_BUDGET,
            Duration::from_millis(200),
            "live documents did not replicate before the outage",
        )
        .await;
    }

    let stopped = cluster
        .stop_node(receiver)
        .await
        .unwrap_or_else(|e| panic!("[{label}] stop node{receiver}: {e}"));
    write_pair(cluster, writer, "Outage", key, label);
    cluster
        .start_stopped_node(stopped, Duration::from_secs(90))
        .await
        .unwrap_or_else(|e| panic!("[{label}] restart node{receiver}: {e}"));

    let expected = [
        "LiveProtected",
        "LivePublic",
        "OutageProtected",
        "OutagePublic",
    ];
    let arrived = await_names(cluster, receiver, key, expected.len()).await;
    let missing: Vec<&str> = expected
        .iter()
        .copied()
        .filter(|n| !arrived.iter().any(|a| a == n))
        .collect();

    println!(
        "[{label}] on node{receiver} after {}s: arrived {arrived:?}",
        ARRIVAL_BUDGET.as_secs()
    );
    println!("[{label}] MISSING: {missing:?}");
    report_sender_logs(cluster, writer, label);
    missing.into_iter().map(str::to_string).collect()
}

/// A: Go writer, Go replicator. The pure-Go baseline the parity claim rests on.
#[tokio::test]
async fn a_go_to_go_outage_replay() {
    let mut cluster = TestCluster::builder()
        .go_nodes(2)
        .with_go_binary(go_binary())
        .with_node_store(0, "badger")
        .with_node_store(1, "badger")
        .with_file_keyring()
        .with_p2p()
        .with_acp_local()
        .with_extra_go_args(["--replicator-retry-intervals", RETRY_INTERVALS])
        .build()
        .await
        .expect("go→go cluster");

    let key = setup(&cluster, &[(0, 1)], "A go→go").await;
    let missing = outage_replay(&mut cluster, 0, 1, &key, "A go→go").await;
    assert!(missing.is_empty(), "A go→go lost {missing:?}");
}

/// B: Go writer, Rust replicator. Go pushes with its host peer id as creator;
/// the Rust receiver must accept and merge both kinds on replay.
#[tokio::test]
async fn b_go_to_rust_outage_replay() {
    let mut cluster = TestCluster::builder()
        .rust_nodes(1)
        .go_nodes(1)
        .with_go_binary(go_binary())
        .with_node_store(0, "regolith")
        .with_node_store(1, "badger")
        .with_file_keyring()
        .with_p2p()
        .with_acp_local()
        .with_extra_rust_args(["--replicator-retry-intervals", RETRY_INTERVALS])
        .with_extra_go_args(["--replicator-retry-intervals", RETRY_INTERVALS])
        .build()
        .await
        .expect("go→rust cluster");

    // node0 is the Rust receiver, node1 the Go writer.
    let key = setup(&cluster, &[(1, 0)], "B go→rust").await;
    let missing = outage_replay(&mut cluster, 1, 0, &key, "B go→rust").await;
    assert!(missing.is_empty(), "B go→rust lost {missing:?}");
}

/// C: Rust writer, Go replicator. 124's repro with a Go receiver: the public
/// document is expected to be refused on the Rust side before it is sent.
#[tokio::test]
async fn c_rust_to_go_outage_replay() {
    let mut cluster = TestCluster::builder()
        .rust_nodes(1)
        .go_nodes(1)
        .with_go_binary(go_binary())
        .with_node_store(0, "regolith")
        .with_node_store(1, "badger")
        .with_file_keyring()
        .with_p2p()
        .with_acp_local()
        .with_extra_rust_args(["--replicator-retry-intervals", RETRY_INTERVALS])
        .with_extra_go_args(["--replicator-retry-intervals", RETRY_INTERVALS])
        .build()
        .await
        .expect("rust→go cluster");

    let key = setup(&cluster, &[(0, 1)], "C rust→go").await;
    let missing = outage_replay(&mut cluster, 0, 1, &key, "C rust→go").await;
    assert!(missing.is_empty(), "C rust→go lost {missing:?}");
}

/// D: the forwarding case. A (Go) writes, B (Rust) forwards to C (Rust), which
/// was down. B holds no ACP owner registration for either document, so
/// `resolve_push_creator` sees the same `Ok(None)` for both.
#[tokio::test]
async fn d_go_to_rust_to_rust_forwarding_outage() {
    let mut cluster = TestCluster::builder()
        .rust_nodes(2)
        .go_nodes(1)
        .with_go_binary(go_binary())
        .with_node_store(0, "regolith")
        .with_node_store(1, "regolith")
        .with_node_store(2, "badger")
        .with_file_keyring()
        .with_p2p()
        .with_acp_local()
        .with_extra_rust_args(["--replicator-retry-intervals", RETRY_INTERVALS])
        .with_extra_go_args(["--replicator-retry-intervals", RETRY_INTERVALS])
        .build()
        .await
        .expect("go→rust→rust cluster");

    // node2 = Go writer A, node0 = Rust forwarder B, node1 = Rust receiver C.
    let label = "D go→rust→rust";
    let key = setup(&cluster, &[(2, 0), (0, 1)], label).await;

    write_pair(&cluster, 2, "Live", &key, label);
    {
        let (c, k) = (&cluster, key.clone());
        poll_until(
            || names_on(c, 1, &k).len() >= 2,
            LIVE_BUDGET,
            Duration::from_millis(200),
            "live documents did not reach C before the outage",
        )
        .await;
    }

    let stopped = cluster.stop_node(1).await.expect("stop C");
    write_pair(&cluster, 2, "Outage", &key, label);
    // Let A's push reach B before C comes back, so the loss under test is B's
    // forwarding replay and not A's.
    {
        let (c, k) = (&cluster, key.clone());
        poll_until(
            || names_on(c, 0, &k).len() >= 4,
            LIVE_BUDGET,
            Duration::from_millis(200),
            "outage documents did not reach the forwarder B",
        )
        .await;
    }
    cluster
        .start_stopped_node(stopped, Duration::from_secs(90))
        .await
        .expect("restart C");

    let expected = [
        "LiveProtected",
        "LivePublic",
        "OutageProtected",
        "OutagePublic",
    ];
    let arrived = await_names(&cluster, 1, &key, expected.len()).await;
    let missing: Vec<&str> = expected
        .iter()
        .copied()
        .filter(|n| !arrived.iter().any(|a| a == n))
        .collect();
    println!(
        "[{label}] on C after {}s: arrived {arrived:?}",
        ARRIVAL_BUDGET.as_secs()
    );
    println!("[{label}] MISSING: {missing:?}");
    report_sender_logs(&cluster, 0, label);
    assert!(missing.is_empty(), "{label} lost {missing:?}");
}
