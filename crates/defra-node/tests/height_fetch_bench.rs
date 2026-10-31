//! What the height-only fetch (#2464) saves on a Shinzo-shaped push.
//!
//! A sender pushes Logs, ten per block height and nine in ten below the
//! receiver's retention floor, to a receiver that sends no CAR requests, so
//! every pushed document reaches the block-by-block walk. With the height
//! fetch, a Log below the floor costs its height block alone; without it,
//! every field block is fetched and the merge refuses the Log.
//!
//! Run with:
//!
//! ```text
//! cargo test -p defra-node --features test-utils --test height_fetch_bench -- --ignored --nocapture
//! ```

#[path = "support/clock.rs"]
mod clock;
#[path = "support/outcome.rs"]
mod outcome;

use clock::sleep;
use defra_node::{Node, NodeBuilder};
use outcome::Result;
use std::time::{Duration, Instant};

const SDL: &str = "type Log { address: String blockNumber: Int data: String topics: String \
                   logIndex: Int transactionHash: String blockHash: String transactionIndex: Int }";
const FLOOR: i64 = 1_000;
const LOGS_PER_HEIGHT: usize = 10;
/// Logs pushed, a multiple of [`BATCH`]; `HEIGHT_BENCH_DOCS` overrides it.
const DOCS: usize = 2_000;
const BATCH: usize = 100;
const TIMEOUT: Duration = Duration::from_secs(600);
const SETTLE: Duration = Duration::from_secs(2);

#[test]
#[ignore = "benchmark: run by hand with --ignored --nocapture"]
fn height_fetch_bench() -> Result<()> {
    edge::testing::edge().block_on(async {
        let docs: usize = std::env::var("HEIGHT_BENCH_DOCS")
            .ok()
            .and_then(|docs| docs.parse().ok())
            .unwrap_or(DOCS);
        // One in ten Logs is above the floor.
        let kept_docs = docs / 10;
        let sender = NodeBuilder::default()
            .with_libp2p("/ip4/127.0.0.1/tcp/0")
            .build()
            .await?;
        sender.add_collection(SDL).await?;
        let log_id = sender
            .get_collection("Log")?
            .expect("Log on the sender")
            .collection_id;
        let mut floors = rapidhash::RapidHashMap::default();
        floors.insert(
            log_id,
            db::retention::RetentionFloor {
                field: "blockNumber".to_string(),
                floor: FLOOR,
            },
        );
        let receiver = NodeBuilder::default()
            .with_libp2p("/ip4/127.0.0.1/tcp/0")
            .with_retention(db::retention::StaticRetentionRule::new(floors))
            .without_car_requests()
            .build()
            .await?;
        receiver.add_collection(SDL).await?;
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
        // Let the collection topic mesh form before the timed pushes.
        sleep(Duration::from_secs(3)).await;
        let commits_before = commits(&receiver);

        let started = Instant::now();
        let mut kept = 0usize;
        for batch in 0..docs / BATCH {
            let mut inputs = Vec::with_capacity(BATCH);
            for i in batch * BATCH..(batch + 1) * BATCH {
                // Every tenth Log is above the floor; the rest are below it,
                // ten to a height, as the Logs of one block are.
                let height = if i % 10 == 9 {
                    kept += 1;
                    FLOOR + 1 + (kept / LOGS_PER_HEIGHT) as i64
                } else {
                    1 + (i / LOGS_PER_HEIGHT) as i64
                };
                inputs.push(format!(
                    "{{address: \"0x{i:040x}\", blockNumber: {height}, data: \"0x{:064x}\", \
                     topics: \"[0x{i:064x}]\", logIndex: {}, transactionHash: \"0x{:064x}\", \
                     blockHash: \"0x{height:064x}\", transactionIndex: {}}}",
                    i * 7,
                    i % LOGS_PER_HEIGHT,
                    i * 13,
                    i % 3,
                ));
            }
            let response = sender
                .execute(&format!(
                    "mutation {{ create_Log(input: [{}]) {{ _docID }} }}",
                    inputs.join(", ")
                ))
                .await;
            assert!(!response.has_errors(), "{:?}", response.errors);
        }
        let pushed = started.elapsed();

        let mut visible = None;
        let mut last = (commits(&receiver), Instant::now());
        let mut progress = Instant::now();
        while started.elapsed() < TIMEOUT {
            let stored = count(&receiver).await;
            if progress.elapsed() >= Duration::from_secs(10) {
                println!(
                    "  {}s: {stored}/{kept_docs} kept Logs, receiver commits {}",
                    started.elapsed().as_secs(),
                    commits(&receiver).saturating_sub(commits_before)
                );
                progress = Instant::now();
            }
            if visible.is_none() && stored >= kept_docs {
                visible = Some(started.elapsed());
            }
            let now = commits(&receiver);
            if now != last.0 {
                last = (now, Instant::now());
            } else if visible.is_some() && last.1.elapsed() >= SETTLE {
                break;
            }
            sleep(Duration::from_millis(20)).await;
        }
        let visible = visible.expect("every kept Log arrived");
        let settled = last.1.duration_since(started);
        println!(
            "height_fetch_bench docs={docs} kept={kept_docs} pushed_ms={} visible_ms={} settled_ms={} \
             receiver_commits={} stored_logs={}",
            pushed.as_millis(),
            visible.as_millis(),
            settled.as_millis(),
            last.0.saturating_sub(commits_before),
            count(&receiver).await,
        );
        p2p_sender.shutdown().await;
        p2p_receiver.shutdown().await;
        sender.database().close().await?;
        receiver.database().close().await?;
        Ok(())
    })
    .unwrap()
}

/// Commits the receiver's store has made, a proxy for the blocks it wrote.
fn commits(node: &Node) -> u64 {
    node.transaction_stats().map_or(0, |stats| stats.commits)
}

async fn count(node: &Node) -> usize {
    let response = node.execute("query { Log { _docID } }").await;
    response
        .data
        .as_ref()
        .and_then(|data| data.get("Log"))
        .and_then(|logs| logs.as_array())
        .map_or(0, Vec::len)
}
