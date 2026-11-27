//! What a local write costs as replicators are added: commits per second,
//! write latency, device syncs and conflicts per write.
//!
//! One on-disk sender, fsyncing every commit, replicates `User` to 0, 1 and 4
//! live in-memory receivers; 1, 4, 16 and 64 writers create documents on it at
//! once. Every replicator registers a durable retry marker before a write
//! returns, and its acknowledgement clears that marker, so each row shows what
//! the marker writes add to a write. Device syncs are the sender engine's
//! `regolith.wal.sync_count`: during the writes, and once every receiver holds
//! every document and the acknowledgements have settled.
//!
//! ```text
//! cargo test -p defra-node --features p2p replicator_writes -- --ignored --nocapture
//! ```
//!
//! Override with DEFRA_REPLICATOR_BENCH_WRITERS, DEFRA_REPLICATOR_BENCH_REPLICATORS
//! (comma-separated) and DEFRA_REPLICATOR_BENCH_WRITES (writes per row).
//!
//! DEFRA_REPLICATOR_BENCH_DOWN=<n> stops the first `n` receivers of each row
//! before the writes, and gives the sender a one-second retry ladder, so its
//! retry pass replays and reschedules those peers' markers every sweep while
//! the writes register and acknowledge theirs.

use std::sync::Arc;
use std::time::{Duration, Instant};

use regolith::{Statistics, StatsLevel, Ticker};

use super::{await_connected_peer, await_user_count, bench_p2p_config, listen_addr, BENCH_SDL};
use crate::Node;

/// How long acknowledgements get to clear their markers after the receivers
/// hold every document.
const SETTLE: Duration = Duration::from_secs(2);

fn counts(name: &str, default: &str) -> Vec<usize> {
    std::env::var(name)
        .unwrap_or_else(|_| default.into())
        .split(',')
        .map(|value| value.trim().parse().expect(name))
        .collect()
}

struct Engine {
    syncs: u64,
    commits: u64,
    conflicts: u64,
}

fn engine(statistics: &Statistics) -> Engine {
    Engine {
        syncs: statistics.get_ticker(Ticker::WalSyncCount),
        commits: statistics.get_ticker(Ticker::CommitCount),
        conflicts: statistics.get_ticker(Ticker::CommitConflicts),
    }
}

async fn build_sender(dir: &std::path::Path, statistics: &Arc<Statistics>, down: usize) -> Node {
    let mut options = storage::RegolithStoreOptions::new();
    options.engine = options.engine.statistics(Some(Arc::clone(statistics)));
    let mut config = bench_p2p_config();
    if down > 0 {
        config.sync.replicator_retry_intervals = Some(vec![1]);
    }
    let node = Node::builder()
        .data_path(dir)
        .with_regolith_options(options)
        .with_p2p(config)
        .build()
        .await
        .expect("build the sender");
    node.add_collection(&defra_core::RequestContext::ambient(), BENCH_SDL)
        .await
        .expect("add the bench schema");
    node
}

async fn build_receiver() -> Node {
    let node = Node::builder()
        .with_p2p(bench_p2p_config())
        .build()
        .await
        .expect("build a receiver");
    node.add_collection(&defra_core::RequestContext::ambient(), BENCH_SDL)
        .await
        .expect("add the bench schema");
    node
}

/// Makes `receiver` a replicator of the sender's `User` writes.
async fn replicate_to(sender: &Node, sender_addr: &str, receiver: &Node) {
    let receiver_addr = listen_addr(receiver).await;
    let sender_p2p = sender.p2p().expect("sender P2P");
    let receiver_p2p = receiver.p2p().expect("receiver P2P");
    sender_p2p
        .connect_peer(&receiver_addr)
        .await
        .expect("connect sender to receiver");
    await_connected_peer(receiver).await;
    let collections = vec!["User".to_string()];
    receiver_p2p
        .add_collections(collections.clone())
        .await
        .expect("subscribe receiver to User");
    receiver_p2p
        .add_replicator(
            collections.clone(),
            Some(sender_addr),
            Default::default(),
            Vec::new(),
            None,
        )
        .await
        .expect("authorize sender on receiver");
    sender_p2p
        .add_replicator(
            collections,
            Some(&receiver_addr),
            Default::default(),
            Vec::new(),
            None,
        )
        .await
        .expect("set replicator from sender to receiver");
}

async fn write(node: &Node, writer: usize, index: usize) -> Duration {
    let start = Instant::now();
    let response = node
        .execute(&format!(
            r#"mutation {{ add_User(input: {{name: "W{writer}-{index}", age: {}}}) {{ _docID }} }}"#,
            index % 100
        ))
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    start.elapsed()
}

fn percentile(sorted: &[Duration], p: f64) -> f64 {
    let rank = ((sorted.len() as f64 * p).ceil() as usize).clamp(1, sorted.len());
    sorted[rank - 1].as_secs_f64() * 1e3
}

async fn measure(replicators: usize, writers: usize, writes: usize, down: usize) {
    let down = down.min(replicators);
    let dir = tempfile::tempdir().expect("a temporary directory");
    let statistics = Arc::new(Statistics::with_level(StatsLevel::ExceptHistogramOrTimers));
    let sender = build_sender(dir.path(), &statistics, down).await;
    sender
        .p2p()
        .expect("sender P2P")
        .add_collections(vec!["User".to_string()])
        .await
        .expect("subscribe sender to User");
    let sender_addr = listen_addr(&sender).await;
    let mut receivers = Vec::with_capacity(replicators);
    for _ in 0..replicators {
        let receiver = build_receiver().await;
        replicate_to(&sender, &sender_addr, &receiver).await;
        receivers.push(receiver);
    }
    for receiver in receivers.drain(..down) {
        receiver.shutdown().await.unwrap();
    }

    let per_writer = writes.div_ceil(writers);
    let total = per_writer * writers;
    let before = engine(&statistics);
    let start = Instant::now();
    let mut latencies: Vec<Duration> = futures::future::join_all((0..writers).map(|writer| {
        let sender = &sender;
        async move {
            let mut latencies = Vec::with_capacity(per_writer);
            for index in 0..per_writer {
                latencies.push(write(sender, writer, index).await);
            }
            latencies
        }
    }))
    .await
    .into_iter()
    .flatten()
    .collect();
    let elapsed = start.elapsed();
    let written = engine(&statistics);

    for receiver in &receivers {
        await_user_count(receiver, total).await;
    }
    edge::testing::edge().sleep(SETTLE).await;
    let settled = engine(&statistics);

    latencies.sort_unstable();
    let per_write = |count: u64| count as f64 / total as f64;
    println!(
        "{{\"replicators\":{replicators},\"down\":{down},\"writers\":{writers},\"writes\":{total},\
         \"commits_per_sec\":{:.1},\"p50_ms\":{:.2},\"p99_ms\":{:.2},\
         \"syncs_per_write\":{:.2},\"settled_syncs_per_write\":{:.2},\
         \"engine_commits_per_write\":{:.2},\"conflicts_per_commit\":{:.4}}}",
        total as f64 / elapsed.as_secs_f64(),
        percentile(&latencies, 0.50),
        percentile(&latencies, 0.99),
        per_write(written.syncs - before.syncs),
        per_write(settled.syncs - before.syncs),
        per_write(settled.commits - before.commits),
        (settled.conflicts - before.conflicts) as f64
            / (settled.commits - before.commits).max(1) as f64,
    );

    sender.shutdown().await.unwrap();
    for receiver in &receivers {
        receiver.shutdown().await.unwrap();
    }
}

#[test]
#[ignore = "benchmark: spins up a sender and its replicators per row"]
fn bench_replicator_writes() {
    let writers = counts("DEFRA_REPLICATOR_BENCH_WRITERS", "1,4,16,64");
    let replicators = counts("DEFRA_REPLICATOR_BENCH_REPLICATORS", "0,1,4");
    let writes = counts("DEFRA_REPLICATOR_BENCH_WRITES", "256")[0];
    let down = counts("DEFRA_REPLICATOR_BENCH_DOWN", "0")[0];
    edge::testing::edge()
        .block_on(async {
            for &replicators in &replicators {
                for &writers in &writers {
                    measure(replicators, writers, writes, down).await;
                }
            }
        })
        .unwrap()
}
