//! #2307: three full-mesh replicas must converge after a write burst that
//! fills the receivers' pending-DAG maps and the senders' push backlogs, and
//! again after every node restarts on its data.
//!
//! The caps are scaled down so a laptop fills them: 16 pending DAGs per node
//! (a peer's quarter is 4) and 16 queued pushes. They are injected through
//! `DEFRA_P2P_*` variables that every node this test process spawns inherits.
//! `DEFRA_BURST_STORM_SECS`, `DEFRA_BURST_CONVERGE_SECS`,
//! `DEFRA_BURST_MAX_PENDING_DAGS`, `DEFRA_BURST_PUSH_QUEUE_CAPACITY` and
//! `DEFRA_BURST_LOG` (the nodes' `RUST_LOG`) reshape the run. Point
//! `DEFRA_RUST_BINARY` at a release build to match a real deployment.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use integration_test::{DefraClient, TestCluster};

const NODES: usize = 3;
const WRITERS_PER_NODE: usize = 4;
const SCHEMA: &str = r#"
type Article { title: String  body: String @chunk(maxChars: 64) }
type Note { title: String  rank: Int }
"#;
const COLLECTIONS: &[&str] = &["Article", "Article__body_chunk", "Note"];
const SHARED_DOCS: usize = 6;

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

async fn sync_status(cluster: &TestCluster, node: usize) -> serde_json::Value {
    let Ok(response) = net::http::get(
        format!("{}/api/v0/p2p/sync/status", cluster.api_url(node)),
        edge::testing::edge(),
    )
    .await
    else {
        return serde_json::Value::Null;
    };
    response.json().await.unwrap_or(serde_json::Value::Null)
}

fn p2p_address(client: &DefraClient) -> String {
    client
        .p2p_info()
        .expect("p2p info")
        .as_array()
        .and_then(|values| values.first())
        .and_then(|value| value.as_str())
        .expect("node has no P2P address")
        .to_string()
}

/// Every node's visible state, sorted, so equal snapshots mean converged.
fn snapshot(client: &DefraClient) -> Option<String> {
    let articles = client
        .query("query { Article(order: {_docID: ASC}) { _docID title body } }")
        .ok()?;
    let notes = client
        .query("query { Note(order: {_docID: ASC}) { _docID title rank } }")
        .ok()?;
    Some(format!("{}{}", articles["Article"], notes["Note"]))
}

fn doc_count(snapshot: &str) -> usize {
    snapshot.matches("\"_docID\"").count()
}

struct Lcg(u64);

impl Lcg {
    fn next(&mut self, bound: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 33) % bound as u64) as usize
    }
}

fn body(rng: &mut Lcg, tag: &str) -> String {
    let paragraphs = 1 + rng.next(6);
    (0..paragraphs)
        .map(|i| {
            format!(
                "Paragraph {i} of {tag} talks about item {}.",
                rng.next(1_000)
            )
        })
        .collect::<Vec<_>>()
        .join("\\n\\n")
}

fn create(client: &DefraClient, rng: &mut Lcg, tag: &str) -> Option<(String, &'static str)> {
    if rng.next(2) == 0 {
        let data = client
            .query(&format!(
                r#"mutation {{ add_Article(input: {{title: "{tag}", body: "{}"}}) {{ _docID }} }}"#,
                body(rng, tag)
            ))
            .ok()?;
        Some((
            data["add_Article"][0]["_docID"].as_str()?.to_string(),
            "Article",
        ))
    } else {
        let data = client
            .query(&format!(
                r#"mutation {{ add_Note(input: {{title: "{tag}", rank: {}}}) {{ _docID }} }}"#,
                rng.next(100)
            ))
            .ok()?;
        Some((data["add_Note"][0]["_docID"].as_str()?.to_string(), "Note"))
    }
}

fn update(client: &DefraClient, rng: &mut Lcg, doc_id: &str, collection: &str, tag: &str) -> bool {
    let input = if collection == "Article" {
        format!(r#"{{title: "{tag}", body: "{}"}}"#, body(rng, tag))
    } else {
        format!(r#"{{title: "{tag}", rank: {}}}"#, rng.next(100))
    };
    client
        .query(&format!(
            r#"mutation {{ update_{collection}(docID: "{doc_id}", input: {input}) {{ _docID }} }}"#
        ))
        .is_ok()
}

fn delete(client: &DefraClient, doc_id: &str, collection: &str) -> bool {
    client
        .query(&format!(
            r#"mutation {{ delete_{collection}(docID: "{doc_id}") {{ _docID }} }}"#
        ))
        .is_ok()
}

/// One writer: creates, edits its own and the shared documents, deletes its own.
fn run_writer(
    client: DefraClient,
    writer: usize,
    shared: Vec<(String, &'static str)>,
    stop: Arc<AtomicBool>,
    ops: Arc<AtomicUsize>,
) {
    let mut rng = Lcg(0x2307 + writer as u64 * 7_919);
    let mut own: Vec<(String, &'static str)> = Vec::new();
    let mut seq = 0usize;
    while !stop.load(Ordering::Relaxed) {
        seq += 1;
        let tag = format!("w{writer}-{seq}");
        let roll = rng.next(100);
        let done = if roll < 45 || own.is_empty() {
            create(&client, &mut rng, &tag)
                .map(|doc| own.push(doc))
                .is_some()
        } else if roll < 65 {
            let (doc_id, collection) = &shared[rng.next(shared.len())];
            update(&client, &mut rng, doc_id, collection, &tag)
        } else if roll < 90 {
            let (doc_id, collection) = own[rng.next(own.len())].clone();
            update(&client, &mut rng, &doc_id, collection, &tag)
        } else {
            let (doc_id, collection) = own.swap_remove(rng.next(own.len()));
            delete(&client, &doc_id, collection)
        };
        if done {
            ops.fetch_add(1, Ordering::Relaxed);
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct Backlog {
    pending_dags: u64,
    persisted_pending: u64,
    queued_pushes: u64,
    active_pushes: u64,
    retry_markers: u64,
    capacity_shed: u64,
    push_rejected: u64,
}

async fn backlog(cluster: &TestCluster) -> Backlog {
    let mut total = Backlog::default();
    for node in 0..NODES {
        let status = sync_status(cluster, node).await;
        let read = |pointer: &str| {
            status
                .pointer(pointer)
                .and_then(|v| v.as_u64())
                .unwrap_or(0)
        };
        total.pending_dags += read("/pending_dags");
        total.persisted_pending += read("/persisted_pending_dags");
        total.queued_pushes += read("/push_backlog/queued_items");
        total.active_pushes += read("/push_backlog/active_jobs");
        total.retry_markers += read("/push_retry_markers/document_markers");
        total.capacity_shed += read("/pending_dag_capacity_shed");
        total.push_rejected +=
            read("/push_backlog/rejected_items_total") + read("/push_backlog/rejected_bytes_total");
    }
    total
}

/// Waits until every node shows the same documents, logging how long it took
/// and the backlogs on the way.
async fn converge(cluster: &TestCluster, phase: &str, timeout: Duration) {
    let started = Instant::now();
    let mut next_log = started;
    let mut peak = Backlog::default();
    loop {
        let snapshots: Vec<Option<String>> =
            (0..NODES).map(|n| snapshot(&cluster.client(n))).collect();
        let status = backlog(cluster).await;
        peak.pending_dags = peak.pending_dags.max(status.pending_dags);
        peak.persisted_pending = peak.persisted_pending.max(status.persisted_pending);
        peak.queued_pushes = peak.queued_pushes.max(status.queued_pushes);
        peak.retry_markers = peak.retry_markers.max(status.retry_markers);
        let counts: Vec<usize> = snapshots
            .iter()
            .map(|s| s.as_deref().map_or(0, doc_count))
            .collect();
        let converged = snapshots.iter().all(Option::is_some)
            && snapshots.windows(2).all(|pair| pair[0] == pair[1]);
        if converged {
            eprintln!(
                "BURST_CONVERGENCE phase={phase} converged_after_s={:.1} docs={} peak={peak:?} now={status:?}",
                started.elapsed().as_secs_f64(),
                counts[0],
            );
            return;
        }
        if Instant::now() >= next_log {
            eprintln!(
                "BURST_CONVERGENCE phase={phase} waiting_s={:.0} docs={counts:?} backlog={status:?}",
                started.elapsed().as_secs_f64()
            );
            next_log += Duration::from_secs(15);
        }
        assert!(
            started.elapsed() < timeout,
            "{phase}: replicas did not converge within {timeout:?}: docs per node {counts:?}, \
             backlog {status:?}"
        );
        nuclei::time::sleep(Duration::from_secs(2)).await;
    }
}

fn connect_mesh(cluster: &TestCluster, addrs: &[String]) {
    for node in 0..NODES {
        let client = cluster.client(node);
        for (peer, addr) in addrs.iter().enumerate() {
            if peer != node {
                client.p2p_connect(&[addr]).expect("connect peer");
            }
        }
    }
}

#[nuclei::test]
#[ignore = "manual burst-convergence harness; runs a multi-minute burst across three nodes"]
async fn full_mesh_replicas_converge_after_a_saturating_burst() {
    edge::compat(async {
        std::env::set_var(
            "DEFRA_P2P_MAX_PENDING_DAGS",
            env_u64("DEFRA_BURST_MAX_PENDING_DAGS", 16).to_string(),
        );
        std::env::set_var(
            "DEFRA_P2P_PUSH_QUEUE_CAPACITY",
            env_u64("DEFRA_BURST_PUSH_QUEUE_CAPACITY", 16).to_string(),
        );
        std::env::set_var(
            "RUST_LOG",
            std::env::var("DEFRA_BURST_LOG").unwrap_or_else(|_| "info".into()),
        );
        let storm = Duration::from_secs(env_u64("DEFRA_BURST_STORM_SECS", 30));
        let timeout = Duration::from_secs(env_u64("DEFRA_BURST_CONVERGE_SECS", 600));

        let mut cluster = TestCluster::builder()
            .rust_nodes(NODES)
            .with_store("regolith")
            .with_keyring()
            .with_p2p()
            .build()
            .await
            .expect("cluster start");
        for node in 0..NODES {
            cluster
                .wait_for_log(node, "p2p_listening", Duration::from_secs(30))
                .await
                .unwrap_or_else(|e| panic!("node{node} P2P listener did not start: {e}"));
        }

        let addrs: Vec<String> = (0..NODES)
            .map(|n| p2p_address(&cluster.client(n)))
            .collect();
        for node in 0..NODES {
            cluster.client(node).schema_add(SCHEMA).expect("schema");
        }
        connect_mesh(&cluster, &addrs);
        for node in 0..NODES {
            let client = cluster.client(node);
            client
                .p2p_collection_add(COLLECTIONS)
                .expect("p2p collection add");
            for (peer, addr) in addrs.iter().enumerate() {
                if peer != node {
                    client
                        .p2p_replicator_set(COLLECTIONS, addr)
                        .expect("replicator add");
                }
            }
        }

        let seeder = cluster.client(0);
        let mut rng = Lcg(1);
        let shared: Vec<(String, &'static str)> = (0..SHARED_DOCS)
            .map(|i| create(&seeder, &mut rng, &format!("shared-{i}")).expect("seed doc"))
            .collect();
        converge(&cluster, "seed", timeout).await;

        let stop = Arc::new(AtomicBool::new(false));
        let ops = Arc::new(AtomicUsize::new(0));
        let writers: Vec<_> = (0..NODES * WRITERS_PER_NODE)
            .map(|writer| {
                let client = cluster.client(writer % NODES);
                let shared = shared.clone();
                let stop = Arc::clone(&stop);
                let ops = Arc::clone(&ops);
                std::thread::spawn(move || run_writer(client, writer, shared, stop, ops))
            })
            .collect();
        let storm_started = Instant::now();
        let mut storm_peak = Backlog::default();
        while storm_started.elapsed() < storm {
            nuclei::time::sleep(Duration::from_secs(2)).await;
            let status = backlog(&cluster).await;
            storm_peak.pending_dags = storm_peak.pending_dags.max(status.pending_dags);
            storm_peak.queued_pushes = storm_peak.queued_pushes.max(status.queued_pushes);
            storm_peak.retry_markers = storm_peak.retry_markers.max(status.retry_markers);
            storm_peak.persisted_pending =
                storm_peak.persisted_pending.max(status.persisted_pending);
        }
        stop.store(true, Ordering::Relaxed);
        for writer in writers {
            writer.join().expect("writer panicked");
        }
        let after_storm = backlog(&cluster).await;
        eprintln!(
            "BURST_CONVERGENCE storm ops={} secs={:.0} peak={storm_peak:?} end={after_storm:?}",
            ops.load(Ordering::Relaxed),
            storm_started.elapsed().as_secs_f64()
        );
        assert!(
            after_storm.capacity_shed > 0 || after_storm.push_rejected > 0,
            "the burst filled neither a pending-DAG map nor a push backlog: {after_storm:?}"
        );

        converge(&cluster, "burst", timeout).await;

        let stopped: Vec<_> = {
            let mut stopped = Vec::new();
            for node in 0..NODES {
                stopped.push(cluster.stop_node(node).await.expect("stop node"));
            }
            stopped
        };
        for node in stopped {
            cluster
                .start_stopped_node(node, Duration::from_secs(60))
                .await
                .expect("restart node");
        }
        for node in 0..NODES {
            let replicators = cluster
                .client(node)
                .p2p_replicator_list()
                .expect("replicator list");
            let entries = replicators.as_array().unwrap_or_else(|| {
                panic!("node{node} replicators are not an array: {replicators}")
            });
            for (peer, address) in addrs.iter().enumerate() {
                if peer == node {
                    continue;
                }
                assert!(
                    entries.iter().any(|replicator| {
                        replicator["Addresses"]
                            .as_array()
                            .is_some_and(|addresses| addresses.iter().any(|value| value == address))
                    }),
                    "node{node} lost its replicator for {address} across restart: {replicators}"
                );
            }
            eprintln!("BURST_CONVERGENCE node{node} replicators after restart: {replicators}");
        }
        let mut rng = Lcg(2);
        for node in 0..NODES {
            let client = cluster.client(node);
            for i in 0..4 {
                create(&client, &mut rng, &format!("after-restart-{node}-{i}"))
                    .expect("write after restart");
            }
        }
        converge(&cluster, "restart", timeout).await;
    })
    .await;
}
