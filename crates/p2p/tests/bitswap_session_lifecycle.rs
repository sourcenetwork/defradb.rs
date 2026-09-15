#![cfg(feature = "test-utils")]

//! A Bitswap fetch must release what it allocates once it has completed.
//!
//! `handle_bitswap_sync` opens an iroh-bitswap `Session` per fetch. The
//! session manager keeps every session in its map until `Session::stop`, and
//! each one owns a worker task, a periodic-search timer, a bounded op queue
//! and a want-sender with its own tasks. The runtime's task census is the
//! observable: a fetch that stops its session leaves the runtime with the
//! number of live tasks it started with. The same handler also records an
//! abort handle per query and only removes it on cancel, so a completed
//! query must not be cancellable afterwards.

use std::time::Duration;

use defra_core::{Block as DefraBlock, CompositeDeltaPayload, CrdtDelta};
use p2p::testutil::MockBitswapStore;
use p2p::{HostEvent, P2PHost, P2PHostHandle, QueryId};
use tokio::sync::mpsc::Receiver;
use tokio::time::timeout;

fn make_data_block() -> (cid::Cid, Vec<u8>) {
    let payload = CompositeDeltaPayload {
        priority: 1,
        schema_version_id: "bafyusers".to_string(),
        status: 1,
    };
    let block = DefraBlock::new(CrdtDelta::Composite(payload), Vec::new(), Vec::new());
    let bytes = block.to_dag_cbor().unwrap();
    let cid = defra_core::block::generate_cid_from_bytes(&bytes).unwrap();
    (cid, bytes)
}

async fn wait_connected(handle: &P2PHostHandle, target: libp2p::PeerId) {
    let start = std::time::Instant::now();
    while !handle
        .connected_peers()
        .await
        .unwrap_or_default()
        .contains(&target)
    {
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "timed out waiting to connect to {target}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn fetch_to_completion(
    consumer: &P2PHostHandle,
    events: &mut Receiver<HostEvent>,
    producer: libp2p::PeerId,
    cid: cid::Cid,
) -> QueryId {
    let query = consumer
        .bitswap_sync(cid, vec![producer], vec![cid])
        .await
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        match timeout(remaining, events.recv())
            .await
            .expect("timed out waiting for BitswapComplete")
            .expect("host event channel closed")
        {
            HostEvent::BitswapComplete {
                query_id, success, ..
            } if query_id == query => {
                assert!(
                    success,
                    "the producer holds the block, the fetch must succeed"
                );
                return query;
            }
            _ => continue,
        }
    }
}

fn live_tasks() -> usize {
    tokio::runtime::Handle::current()
        .metrics()
        .num_alive_tasks()
}

/// Waits for the task census to hold still for `hold`, and returns it.
async fn settled_live_tasks(hold: Duration) -> usize {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let mut last = live_tasks();
    let mut since = std::time::Instant::now();
    while since.elapsed() < hold {
        assert!(
            std::time::Instant::now() < deadline,
            "task census never settled"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
        let now = live_tasks();
        if now != last {
            last = now;
            since = std::time::Instant::now();
        }
    }
    last
}

struct Pair {
    consumer: P2PHostHandle,
    consumer_events: Receiver<HostEvent>,
    producer: P2PHostHandle,
    producer_peer: libp2p::PeerId,
}

async fn connected_pair(cid: cid::Cid, bytes: Vec<u8>) -> Pair {
    let (producer, producer_handle, _producer_events, _) =
        P2PHost::new(MockBitswapStore::new().with_block(cid, bytes))
            .await
            .unwrap();
    let (consumer, consumer_handle, consumer_events, _) =
        P2PHost::new(MockBitswapStore::new()).await.unwrap();
    let producer_peer = producer.local_peer_id();
    let consumer_peer = consumer.local_peer_id();
    tokio::spawn(producer.run());
    tokio::spawn(consumer.run());

    producer_handle
        .listen("/ip4/127.0.0.1/tcp/0".parse().unwrap())
        .await
        .unwrap();
    let producer_addr = producer_handle.listen_addresses().await.unwrap().remove(0);
    consumer_handle
        .dial(producer_peer, vec![producer_addr])
        .await
        .unwrap();
    wait_connected(&consumer_handle, producer_peer).await;
    wait_connected(&producer_handle, consumer_peer).await;

    Pair {
        consumer: consumer_handle,
        consumer_events,
        producer: producer_handle,
        producer_peer,
    }
}

#[tokio::test]
async fn a_completed_bitswap_fetch_leaves_no_session_tasks_behind() {
    let (cid, bytes) = make_data_block();
    let mut pair = connected_pair(cid, bytes).await;

    // One fetch first, so anything the hosts spawn lazily on their first
    // Bitswap exchange is inside the baseline rather than counted as a leak.
    fetch_to_completion(
        &pair.consumer,
        &mut pair.consumer_events,
        pair.producer_peer,
        cid,
    )
    .await;
    let baseline = settled_live_tasks(Duration::from_millis(300)).await;

    const FETCHES: usize = 3;
    for _ in 0..FETCHES {
        fetch_to_completion(
            &pair.consumer,
            &mut pair.consumer_events,
            pair.producer_peer,
            cid,
        )
        .await;
    }
    let after = settled_live_tasks(Duration::from_millis(300)).await;

    assert!(
        after <= baseline,
        "{FETCHES} completed Bitswap fetches left {} live tasks behind ({} per fetch): \
         each fetch's session is still registered and running",
        after - baseline,
        (after - baseline) / FETCHES,
    );

    pair.consumer.shutdown().await.ok();
    pair.producer.shutdown().await.ok();
}

#[tokio::test]
async fn a_completed_bitswap_query_is_no_longer_cancellable() {
    let (cid, bytes) = make_data_block();
    let mut pair = connected_pair(cid, bytes).await;

    let query = fetch_to_completion(
        &pair.consumer,
        &mut pair.consumer_events,
        pair.producer_peer,
        cid,
    )
    .await;

    let cancelled = pair.consumer.bitswap_cancel(query).await.unwrap();
    assert!(
        !cancelled,
        "query {query:?} completed but its abort handle is still registered"
    );

    pair.consumer.shutdown().await.ok();
    pair.producer.shutdown().await.ok();
}
