//! #1154 at-scale repro: every success-acked document must merge on the
//! restarted hub. Four persistent pushers commit at least 500 documents while
//! source-side CAR serving is disabled. All-matching filtered replication in
//! Controlled mode also denies legacy Bitswap fallback, holding a durable
//! crash window without racing the receiver's recovery speed.
//!
//! Own binary: injects process-wide node settings inherited by every spawned
//! node.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use integration_test::TestCluster;

const SCHEMA: &str = "type User { name: String  age: Int @immutable }";
const PUSHERS: usize = 4;
const MIN_DOCS: usize = 500;

#[path = "issue1154_repro/support.rs"]
mod support;
use support::{log_field, pending_dags, registered_doc_ids, sender_retry_snapshot, sync_status};

/// Pushers write into a 1-slot hub with source-side CAR serving disabled.
/// A real success acknowledgment and an unmerged durable registration must
/// coexist before the hub is hard-killed and respawned on its rootdir.
///
/// The restart contract under test (PendingDagRestart.tla INV_AckBacked): the
/// success ack can discharge the sender's retry record. Require both durable
/// restoration and exact registered-document recovery: merging alone cannot
/// prove restoration because restarted sources may also replay their state.
#[tokio::test]
async fn hub_restart_recovers_success_acked_pending_dags() {
    std::env::set_var("DEFRA_P2P_MAX_PENDING_DAGS", "1");
    std::env::set_var("DEFRA_P2P_RATE_LIMIT_BURST", "500");
    std::env::set_var("RUST_LOG", "info,p2p::sync::restart_recovery=debug");

    // Every node must survive a restart with identity and state intact: the
    // harness defaults (memory store, no keyring => ephemeral peer key) would
    // make the respawned hub an empty stranger the pushers cannot dial.
    let mut cluster = TestCluster::builder()
        .rust_nodes(1 + PUSHERS)
        .with_store("regolith")
        .with_keyring()
        .with_p2p()
        .with_acp_local()
        .build()
        .await
        .expect("cluster start");

    let startup_timeout = Duration::from_secs(30);
    for node in 0..=PUSHERS {
        cluster
            .wait_for_log(node, "p2p_listening", startup_timeout)
            .await
            .unwrap_or_else(|e| panic!("node{node} P2P listener did not start: {e}"));
    }

    let hub = cluster.client(0);
    let hub_info = hub.p2p_info().expect("hub p2p info");
    let hub_addr = hub_info
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|v| v.as_str())
        .expect("hub has no P2P address")
        .to_string();

    hub.schema_add(SCHEMA).expect("hub schema");
    for pusher in 1..=PUSHERS {
        let client = cluster.client(pusher);
        client.schema_add(SCHEMA).expect("pusher schema");
        cluster.nodes[pusher].process.kill();
        std::env::set_var("DEFRA_P2P_RATE_LIMIT_BURST", "0");
        cluster
            .restart_node(pusher, Duration::from_secs(60))
            .await
            .expect("restart source with CAR serving disabled");
        std::env::set_var("DEFRA_P2P_RATE_LIMIT_BURST", "500");
        let client = cluster.client(pusher);
        client.p2p_connect(&[&hub_addr]).expect("connect to hub");
        // Filtered replicas recover through rooted CAR; Controlled mode
        // denies their legacy Bitswap data-block fallback as well.
        let added = std::process::Command::new(client.binary_path())
            .arg("--url")
            .arg(cluster.api_url(pusher).strip_prefix("http://").unwrap())
            .args([
                "client",
                "p2p",
                "replicator",
                "add",
                "-c",
                "User",
                "--filter",
                r#"{"age":{"_gte":0}}"#,
                &hub_addr,
            ])
            .output()
            .expect("add all-matching filtered replicator");
        assert!(
            added.status.success(),
            "filtered replicator: {}",
            String::from_utf8_lossy(&added.stderr)
        );
    }

    // Continuous head-only write load: every live push has missing field
    // links on the hub, so the single pending slot keeps being occupied by a
    // success-acked registration while the writers run.
    let stop_writers = Arc::new(AtomicBool::new(false));
    let doc_ids = Arc::new(Mutex::new(Vec::<String>::new()));
    let writer_handles: Vec<_> = (1..=PUSHERS)
        .map(|pusher| {
            let client = cluster.client(pusher);
            let stop = Arc::clone(&stop_writers);
            let doc_ids = Arc::clone(&doc_ids);
            std::thread::spawn(move || {
                let mut doc = 0usize;
                while !stop.load(Ordering::Relaxed) {
                    let mutation = format!(
                        r#"mutation {{ add_User(input: {{name: "p{pusher}-d{doc}", age: {doc}}}) {{ _docID }} }}"#
                    );
                    let data = client.query(&mutation).expect("create doc on pusher");
                    let doc_id = data["add_User"][0]["_docID"]
                        .as_str()
                        .expect("missing _docID")
                        .to_string();
                    doc_ids.lock().unwrap().push(doc_id);
                    doc += 1;
                    std::thread::sleep(Duration::from_millis(25));
                }
            })
        })
        .collect();

    // Keep the at-scale backlog while holding the missing-link fetch boundary.
    let load_deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let produced = doc_ids.lock().unwrap().len();
        if produced >= MIN_DOCS {
            break;
        }
        assert!(
            Instant::now() < load_deadline,
            "writers only produced {produced} docs before the load deadline"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    stop_writers.store(true, Ordering::Relaxed);
    for handle in writer_handles {
        handle.join().expect("writer thread panicked");
    }
    let expected_doc_ids = Arc::try_unwrap(doc_ids)
        .expect("writers joined")
        .into_inner()
        .unwrap();
    assert!(expected_doc_ids.len() >= MIN_DOCS);

    let hub_api = cluster.api_url(0).to_string();
    let hub_log = cluster.nodes[0]
        .rootdir
        .parent()
        .expect("hub rootdir has a parent")
        .join("logs/stdout.log");
    let registration_deadline = Instant::now() + Duration::from_secs(30);
    let registered = loop {
        let registered = registered_doc_ids(&hub_log);
        if !registered.is_empty() {
            break registered;
        }
        assert!(
            Instant::now() < registration_deadline,
            "hub never durably registered a pending DAG before the kill"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    eprintln!(
        "issue1154_repro: {} durably registered documents at kill time",
        registered.len()
    );

    let registration = std::fs::read_to_string(&hub_log).expect("hub log");
    let registration = registration
        .lines()
        .rev()
        .find(|line| line.contains("Persisted pending DAG registration"))
        .expect("pending registration");
    let pending_cid = log_field(registration, "cid=").expect("registration CID");
    let pending_doc = log_field(registration, "doc_id=").expect("registration doc ID");
    let ack_deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let acknowledged = (1..=PUSHERS).any(|pusher| {
            let log = cluster.nodes[pusher]
                .rootdir
                .parent()
                .unwrap()
                .join("logs/stdout.log");
            std::fs::read_to_string(log).is_ok_and(|log| {
                log.lines().any(|line| {
                    line.contains("PushLog head hint accepted by replicator")
                        && log_field(line, "cid=") == Some(pending_cid)
                })
            })
        });
        let status = sync_status(&cluster, 0).await;
        let (sender_markers, _) = sender_retry_snapshot(&cluster).await;
        if acknowledged
            && status["pending_dag_capacity_shed"].as_u64().unwrap_or(0) > 0
            && sender_markers >= MIN_DOCS - 1
        {
            break;
        }
        assert!(
            Instant::now() < ack_deadline,
            "missing success ack or at-scale retry backlog"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    // Hold beyond the ten-second CAR-first fallback window. A CAR-only gate
    // would allow libp2p Bitswap to discharge the supposedly frozen root.
    tokio::time::sleep(Duration::from_secs(12)).await;
    assert_eq!(pending_dags(&hub_api).await, 1);
    assert_eq!(sync_status(&cluster, 0).await["persisted_pending_dags"], 1);
    let before_crash = hub.query("query { User { _docID } }").expect("hub query");
    assert!(before_crash["User"]
        .as_array()
        .unwrap()
        .iter()
        .all(|row| row["_docID"] != pending_doc));

    cluster.nodes[0].process.kill();

    cluster
        .restart_node(0, Duration::from_secs(60))
        .await
        .expect("restart hub on its rootdir");

    eprintln!(
        "issue1154_repro: {} committed documents expected on restarted hub",
        expected_doc_ids.len()
    );

    // Anti-vacuity for the recovery path: durable registrations must have
    // survived the kill and been re-driven. Without persistence this log
    // (emitted only when records were loaded) never appears.
    let restore_deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let log = std::fs::read_to_string(&hub_log).unwrap_or_default();
        if log.contains("restored persisted pending DAG registrations") {
            break;
        }
        assert!(
            Instant::now() < restore_deadline,
            "hub restart never restored persisted pending DAG registrations"
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    }

    // Restore proof precedes re-enabling source-side CAR serving.
    for pusher in 1..=PUSHERS {
        cluster.nodes[pusher].process.kill();
        cluster
            .restart_node(pusher, Duration::from_secs(60))
            .await
            .expect("restart source with normal request intake");
        cluster
            .client(pusher)
            .p2p_connect(&[&hub_addr])
            .expect("reconnect source");
    }

    // Every document the hub durably registered must merge on the restarted
    // hub. These roots may have been success-acked, discharging the sender's
    // retry record; losing their receiver registration is the #1154 failure.
    // The roots that were actionably nacked instead keep sender
    // markers on the Go-compatible 30s..32m ladder; with a one-slot receiver,
    // requiring hundreds of those to traverse the ladder inside this test's
    // four-minute bound would test wall-clock tuning rather than crash
    // durability, so they are not required to arrive here.
    //
    // The aggregate below is a coverage smoke check only. It cannot carry the
    // per-document claim: `merged` is a set of document ids, but the receiver
    // and sender terms are opaque counts (root-CID cardinality and a
    // `/rep/retry/doc/` prefix scan), so one unit of overlap would buy one
    // unit of slack and hide exactly one orphan. Overlap is real here: the
    // receiver commits its registration before acknowledging the push, so a
    // hub killed in between leaves a document owned twice, by the restored
    // obligation and by the sender marker its unacknowledged push retained.
    let hub = cluster.client(0);
    let converge_start = Instant::now();
    let deadline = Instant::now() + Duration::from_secs(240);
    let mut next_progress_log = Instant::now() + Duration::from_secs(30);
    loop {
        // Bracket the receiver observation with sender snapshots. Marker
        // ownership only decreases after the writers stop, so equal endpoint
        // samples prove that no sender-to-receiver transfer crossed this
        // observation. Likewise, stable receiver terminal/pending counters
        // prove that the document query did not straddle pending-to-merged
        // discharge. A single unbracketed pass can otherwise count one
        // obligation twice (or not at all) across these independent HTTP
        // surfaces even though the protocol conserved it exactly.
        let (sender_markers_before, sender_jobs_before) = sender_retry_snapshot(&cluster).await;
        let hub_status_before = sync_status(&cluster, 0).await;
        let present: std::collections::HashSet<String> = hub
            .query("query { User { _docID } }")
            .ok()
            .and_then(|result| {
                result["User"].as_array().map(|rows| {
                    rows.iter()
                        .filter_map(|row| row["_docID"].as_str().map(str::to_string))
                        .collect()
                })
            })
            .unwrap_or_default();
        let hub_status_after = sync_status(&cluster, 0).await;
        let (sender_markers_after, sender_jobs_after) = sender_retry_snapshot(&cluster).await;

        let receiver_obligations = hub_status_after["persisted_pending_dags"]
            .as_u64()
            .expect("persisted pending count") as usize;
        let receiver_terminal_merges = hub_status_after["pending_dag_terminal_merged"]
            .as_u64()
            .expect("terminal merge count");
        let receiver_stable = hub_status_before["persisted_pending_dags"]
            == hub_status_after["persisted_pending_dags"]
            && hub_status_before["pending_dag_terminal_merged"]
                == hub_status_after["pending_dag_terminal_merged"];
        let sender_stable = sender_markers_before == sender_markers_after
            && sender_jobs_before == 0
            && sender_jobs_after == 0;

        let merged = expected_doc_ids
            .iter()
            .filter(|id| present.contains(id.as_str()))
            .count();
        let orphaned: Vec<&str> = registered
            .iter()
            .filter(|doc_id| !present.contains(doc_id.as_str()))
            .map(String::as_str)
            .collect();

        // A dropped obligation must not pass as a merge: both of these retire
        // a root without one, and no amount of further waiting recovers it.
        let fetch_exhausted = hub_status_after["pending_dag_fetch_exhausted"]
            .as_u64()
            .expect("fetch exhausted count");
        let quarantined = hub_status_after["pending_dag_terminal_quarantined"]
            .as_u64()
            .expect("terminal quarantine count");
        assert_eq!(
            (fetch_exhausted, quarantined),
            (0, 0),
            "restored registration retired without merging: exhausted={fetch_exhausted}, \
             quarantined={quarantined}, orphaned={}/{}",
            orphaned.len(),
            registered.len()
        );

        let covered =
            merged + receiver_obligations + sender_markers_after >= expected_doc_ids.len();
        if orphaned.is_empty() && receiver_terminal_merges > 0 && sender_stable && receiver_stable {
            assert!(
                covered,
                "committed documents lost their owner across the restart: merged={merged}, \
                 receiver={receiver_obligations}, sender={sender_markers_after}, expected={}",
                expected_doc_ids.len()
            );
            eprintln!(
                "issue1154_repro: all {} durable registrations merged; \
                 merged={merged}, receiver={receiver_obligations}, sender={sender_markers_after} \
                 {:.1}s after restart",
                registered.len(),
                converge_start.elapsed().as_secs_f64()
            );
            break;
        }
        if Instant::now() >= next_progress_log {
            eprintln!(
                "issue1154_repro: orphaned={}/{}, merged={merged}, \
                 receiver={receiver_obligations}, sender={sender_markers_after}, \
                 terminal_merges={receiver_terminal_merges}, \
                 stable={sender_stable}/{receiver_stable} after {:.1}s",
                orphaned.len(),
                registered.len(),
                converge_start.elapsed().as_secs_f64()
            );
            next_progress_log += Duration::from_secs(30);
        }
        assert!(
            Instant::now() < deadline,
            "durably registered documents never merged on the restarted hub: \
             orphaned={}/{} {:?}, merged={merged}, receiver={receiver_obligations}, \
             sender={sender_markers_after}, expected={}, \
             terminal_merges={receiver_terminal_merges}, \
             stable={sender_stable}/{receiver_stable}",
            orphaned.len(),
            registered.len(),
            orphaned.iter().take(8).collect::<Vec<_>>(),
            expected_doc_ids.len()
        );
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}
