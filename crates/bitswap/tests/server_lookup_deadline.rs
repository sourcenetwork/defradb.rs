use std::future::pending;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use bitswap::server::ServerConfig;
use bitswap::{BitswapMessage, Block, Store, WantType};
use cid::Cid;
use libp2p::PeerId;

mod common;
mod server_harness;
use common::mem_store::MemStore;
use common::{block_v1, cid_v1};
use server_harness::{has_block, has_dont_have, start, start_with, want, Harness};

#[derive(Debug, Clone)]
struct HangStore {
    inner: MemStore,
    hung: Option<Cid>,
    hang_all: Arc<AtomicBool>,
    calls: Arc<AtomicUsize>,
}

impl HangStore {
    fn new(blocks: &[Block], hung: Option<Cid>) -> Self {
        HangStore {
            inner: MemStore::new(blocks),
            hung,
            hang_all: Arc::new(AtomicBool::new(false)),
            calls: Arc::new(AtomicUsize::new(0)),
        }
    }
}

#[async_trait]
impl Store for HangStore {
    async fn get_size(&self, cid: &Cid) -> anyhow::Result<usize> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.hang_all.load(Ordering::SeqCst) || self.hung == Some(*cid) {
            pending::<()>().await;
        }
        self.inner.get_size(cid).await
    }

    async fn get(&self, cid: &Cid) -> anyhow::Result<Block> {
        self.inner.get(cid).await
    }

    async fn has(&self, cid: &Cid) -> anyhow::Result<bool> {
        self.inner.has(cid).await
    }
}

fn hanging_filter(hung: Cid) -> ServerConfig {
    ServerConfig {
        peer_block_request_filter: Some(Box::new(move |_: &PeerId, cid: &Cid| {
            let stuck = *cid == hung;
            Box::pin(async move {
                if stuck {
                    pending::<()>().await;
                }
                true
            })
        })),
        ..Default::default()
    }
}

async fn past_deadline() {
    tokio::time::sleep(Duration::from_secs(11)).await;
}

fn count_blocks(messages: &[BitswapMessage], cid: &Cid) -> usize {
    messages
        .iter()
        .flat_map(|m| m.blocks())
        .filter(|b| b.cid == *cid)
        .count()
}

async fn still_serves(h: &mut Harness, good: &Block) {
    let other = PeerId::random();
    h.server.peer_connected(other);
    h.send(want(good.cid, WantType::Block, false));
    h.server
        .try_receive_message(other, want(good.cid, WantType::Block, false))
        .expect("inbound queue has room");
    let out = h.drain().await;
    assert_eq!(count_blocks(&out, &good.cid), 2);
}

#[test]
fn the_default_lookup_timeout_is_ten_seconds() {
    assert_eq!(
        ServerConfig::default().message_lookup_timeout,
        Duration::from_secs(10)
    );
}

#[tokio::test(start_paused = true)]
async fn a_hung_size_lookup_answers_dont_have_and_does_not_stall_the_stage() {
    let good = block_v1(b"good");
    let hung = block_v1(b"hung");
    let store = HangStore::new(&[good.clone(), hung.clone()], Some(hung.cid));
    let mut h = start_with(store, ServerConfig::default());

    h.send(want(hung.cid, WantType::Block, true));
    past_deadline().await;
    let out = h.drain().await;
    assert!(has_dont_have(&out, &hung.cid));
    assert!(!has_block(&out, &hung.cid));

    still_serves(&mut h, &good).await;
}

#[tokio::test(start_paused = true)]
async fn a_hung_filter_denies_and_does_not_stall_the_stage() {
    let good = block_v1(b"good");
    let hung = block_v1(b"hung");
    let mut h = start(&[good.clone(), hung.clone()], hanging_filter(hung.cid));

    h.send(want(hung.cid, WantType::Block, true));
    past_deadline().await;
    let out = h.drain().await;
    assert!(has_dont_have(&out, &hung.cid));
    assert!(!has_block(&out, &hung.cid));

    h.send(want(hung.cid, WantType::Block, false));
    past_deadline().await;
    let out = h.drain().await;
    assert!(!has_dont_have(&out, &hung.cid));
    assert!(!has_block(&out, &hung.cid));

    still_serves(&mut h, &good).await;
}

#[tokio::test(start_paused = true)]
async fn the_deadline_is_per_message_and_skips_remaining_lookups() {
    let blocks: Vec<Block> = (0..3u8).map(|i| block_v1(&[i])).collect();
    let store = HangStore::new(&blocks, None);
    store.hang_all.store(true, Ordering::SeqCst);
    let calls = store.calls.clone();
    let hang_all = store.hang_all.clone();
    let mut h = start_with(store, ServerConfig::default());

    let mut message = BitswapMessage::new(false);
    for block in &blocks {
        message.add_entry(block.cid, 1, WantType::Block, true);
    }
    h.send(message);
    past_deadline().await;
    let out = h.drain().await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(blocks.iter().all(|b| has_dont_have(&out, &b.cid)));

    hang_all.store(false, Ordering::SeqCst);
    h.send(want(blocks[0].cid, WantType::Block, false));
    let out = h.drain().await;
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert!(has_block(&out, &blocks[0].cid));
}

#[tokio::test(start_paused = true)]
async fn a_cancel_is_applied_when_the_message_lookups_hang() {
    let held = [cid_v1(b"held-1"), cid_v1(b"held-2")];
    let hung = block_v1(b"hung");
    let store = HangStore::new(std::slice::from_ref(&hung), None);
    let hang_all = store.hang_all.clone();
    let mut h = start_with(store, ServerConfig::default());

    for cid in held {
        h.send(want(cid, WantType::Block, false));
    }
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(h.server.ledger_wants(h.peer).await, Some(2));

    hang_all.store(true, Ordering::SeqCst);
    let mut message = BitswapMessage::new(false);
    for cid in held {
        message.cancel(cid);
    }
    message.add_entry(hung.cid, 1, WantType::Block, false);
    h.send(message);
    past_deadline().await;
    assert_eq!(h.server.ledger_wants(h.peer).await, Some(1));
}

#[tokio::test(start_paused = true)]
async fn an_unrepresentable_lookup_timeout_means_no_deadline() {
    let good = block_v1(b"good");
    let store = HangStore::new(std::slice::from_ref(&good), None);
    let config = ServerConfig {
        message_lookup_timeout: std::time::Duration::MAX,
        ..ServerConfig::default()
    };
    let mut h = start_with(store, config);

    still_serves(&mut h, &good).await;
}
