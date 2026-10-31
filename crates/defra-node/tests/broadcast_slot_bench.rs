//! What bounding committed writes' gossip (#2489) costs a node's writers.
//!
//! A sender gossips to one subscribed receiver. 1, 4, 16 and 64 concurrent
//! writers each create Logs one at a time, alone and while a burst of
//! 100-Log batches crowds the pending gossip. Each row reports commits per
//! second, the p50 and p99 of one create, how many of the written Logs the
//! receiver got, and how many broadcasts the sender rejected.
//!
//! Run with:
//!
//! ```text
//! cargo test -p defra-node --features test-utils --test broadcast_slot_bench -- --ignored --nocapture
//! ```

#[path = "support/clock.rs"]
mod clock;
#[path = "support/outcome.rs"]
mod outcome;

use clock::sleep;
use defra_node::{Node, NodeBuilder};
use outcome::Result;
use std::time::{Duration, Instant};

const SDL: &str = "type Log { address: String blockNumber: Int }";
const WRITERS: [usize; 4] = [1, 4, 16, 64];
/// Creates per row, split evenly across the writers.
const CREATES: usize = 640;
/// Batches in a burst, each of [`BATCH`] Logs.
const BURST_BATCHES: usize = 40;
const BATCH: usize = 100;
/// How long the receiver's count may stall before the row stops waiting.
const DELIVERY_WAIT: Duration = Duration::from_secs(60);

#[test]
#[ignore = "benchmark: run by hand with --ignored --nocapture"]
fn broadcast_slot_bench() -> Result<()> {
    edge::testing::edge()
        .block_on(async {
            for burst in [false, true] {
                for writers in WRITERS {
                    row(writers, burst).await?;
                }
            }
            Ok(())
        })
        .unwrap()
}

async fn row(writers: usize, burst: bool) -> Result<()> {
    let (sender, receiver) = pair().await?;
    let started = Instant::now();
    let per_writer = CREATES / writers;
    let creates = (0..writers).map(|writer| {
        let sender = &sender;
        async move {
            let mut latencies = Vec::with_capacity(per_writer);
            for n in 0..per_writer {
                let at = Instant::now();
                let response = sender
                    .execute(&format!(
                        r#"mutation {{ create_Log(input: {{address: "w{writer}-{n}", blockNumber: 1}}) {{ _docID }} }}"#
                    ))
                    .await;
                assert!(!response.has_errors(), "{:?}", response.errors);
                latencies.push(at.elapsed());
            }
            latencies
        }
    });
    let bursts = async {
        if burst {
            for batch in 0..BURST_BATCHES {
                let inputs: Vec<String> = (0..BATCH)
                    .map(|n| format!(r#"{{address: "b{batch}-{n}", blockNumber: 2}}"#))
                    .collect();
                let response = sender
                    .execute(&format!(
                        "mutation {{ create_Log(input: [{}]) {{ _docID }} }}",
                        inputs.join(", ")
                    ))
                    .await;
                assert!(!response.has_errors(), "{:?}", response.errors);
            }
        }
    };
    let (latencies, ()) = futures::join!(futures::future::join_all(creates), bursts);
    let elapsed = started.elapsed();
    let mut latencies: Vec<Duration> = latencies.into_iter().flatten().collect();
    latencies.sort();
    let at = |q: f64| latencies[((latencies.len() - 1) as f64 * q) as usize];
    let written = latencies.len() + if burst { BURST_BATCHES * BATCH } else { 0 };
    let (received, last_arrival) = received_within(&receiver, written, DELIVERY_WAIT).await;
    let delivered_ms = last_arrival.duration_since(started).as_millis();
    let rejected = sender
        .p2p_system()
        .expect("sender p2p")
        .ops()
        .sync_status()
        .await?
        .get("non_authoritative_broadcast_rejected_total")
        .and_then(|total| total.as_u64())
        .unwrap_or(0);
    println!(
        "broadcast_slot_bench burst={burst} writers={writers} creates={} commits_per_s={:.0} p50_ms={:.2} p99_ms={:.2} received={received}/{written} delivered_ms={delivered_ms} rejected={rejected}",
        latencies.len(),
        latencies.len() as f64 / elapsed.as_secs_f64(),
        at(0.50).as_secs_f64() * 1e3,
        at(0.99).as_secs_f64() * 1e3,
    );
    if received < written {
        // Where the receiver's missing documents are.
        let status = receiver
            .p2p_system()
            .expect("receiver p2p")
            .ops()
            .sync_status()
            .await?;
        let fields = [
            "pending_dags",
            "persisted_pending_dags",
            "gossip_heads_deferred",
            "gossip_heads_shed",
            "pending_dag_registered",
            "pending_dag_resolved",
            "pending_dag_expired",
            "pending_dag_fetch_exhausted",
            "pending_dag_terminal_merged",
            "quarantined_pending_dags",
            "gossip_decode_failures",
            "single_flight_suppressed",
            "already_merged_fast_path",
            "next_pending_retry_in_ms",
            "request_dispatch",
        ];
        for field in fields {
            println!(
                "  receiver {field}={}",
                status.get(field).unwrap_or(&serde_json::Value::Null)
            );
        }
    }
    shutdown(sender).await?;
    shutdown(receiver).await
}

/// How many Logs the receiver holds once it has `expected`, or once `wait`
/// passes with the count no longer growing, and when the last one arrived.
async fn received_within(receiver: &Node, expected: usize, wait: Duration) -> (usize, Instant) {
    let mut last = (0, Instant::now());
    loop {
        let response = receiver.execute("query { COUNT(Log: {}) }").await;
        let count = response
            .data
            .as_ref()
            .and_then(|data| data.get("COUNT"))
            .and_then(|count| count.as_u64())
            .map_or(0, |count| count as usize);
        if count != last.0 {
            last = (count, Instant::now());
        }
        if count >= expected || last.1.elapsed() >= wait {
            return last;
        }
        sleep(Duration::from_millis(200)).await;
    }
}

/// A sender and a receiver, connected, both on the Log topic.
async fn pair() -> Result<(Node, Node)> {
    let sender = NodeBuilder::default()
        .with_libp2p("/ip4/127.0.0.1/tcp/0")
        .build()
        .await?;
    let receiver = NodeBuilder::default()
        .with_libp2p("/ip4/127.0.0.1/tcp/0")
        .build()
        .await?;
    for node in [&sender, &receiver] {
        node.add_collection(SDL).await?;
    }
    let p2p_sender = sender.p2p_system().cloned().expect("sender p2p");
    let p2p_receiver = receiver.p2p_system().cloned().expect("receiver p2p");
    let receiver_peer = p2p_receiver.ops().local_peer_id().await?;
    let receiver_addr = loop {
        let addrs = p2p_receiver.ops().listen_addresses().await?;
        if let Some(addr) = addrs.into_iter().find(|a| a.contains("/tcp/")) {
            break format!("{addr}/p2p/{receiver_peer}");
        }
        sleep(Duration::from_millis(20)).await;
    };
    p2p_sender.ops().connect_peer(&receiver_addr).await?;
    for p2p in [&p2p_sender, &p2p_receiver] {
        p2p.ops().add_collections(vec!["Log".to_string()]).await?;
    }
    // Let the collection topic mesh form before the timed writes.
    sleep(Duration::from_secs(3)).await;
    Ok((sender, receiver))
}

async fn shutdown(node: Node) -> Result<()> {
    if let Some(p2p) = node.p2p_system().cloned() {
        p2p.shutdown().await;
    }
    node.database().close().await?;
    Ok(())
}
