use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use bitswap::server::ServerConfig;
use bitswap::{BitswapMessage, Block, Store, WantType};
use cid::Cid;

mod common;
mod server_harness;
use common::mem_store::MemStore;
use common::{block_v1, cid_v1};
use server_harness::{cancel, has_block, has_dont_have, start, start_with, want_with, Harness};

const CAP: usize = 3;

fn capped() -> ServerConfig {
    ServerConfig {
        max_queued_wantlist_entries_per_peer: CAP,
        ..Default::default()
    }
}

fn blocks(count: usize) -> Vec<Block> {
    (0..count).map(|i| block_v1(&i.to_le_bytes())).collect()
}

async fn settle(h: &Harness) -> Option<usize> {
    tokio::time::sleep(Duration::from_millis(50)).await;
    h.server.ledger_wants(h.peer).await
}

async fn fill_with_missing(h: &mut Harness) -> Vec<Cid> {
    let missing: Vec<Cid> = (0..CAP).map(|i| cid_v1(&[b'm', i as u8])).collect();
    for cid in &missing {
        h.send(want_with(*cid, 1, false));
    }
    assert_eq!(settle(h).await, Some(CAP));
    missing
}

#[tokio::test(start_paused = true)]
async fn one_message_queues_at_most_the_cap_of_tasks() {
    let all = blocks(2000);
    let mut h = start(&all, ServerConfig::default());

    let mut message = BitswapMessage::new(false);
    for block in &all {
        message.add_entry(block.cid, 1, WantType::Block, false);
    }
    h.send(message);

    let served: usize = h.drain().await.iter().map(|m| m.blocks_len()).sum();
    assert_eq!(served, 1024);
}

#[tokio::test(start_paused = true)]
async fn one_message_records_at_most_the_cap_of_wants() {
    let mut h = start(&[], ServerConfig::default());

    let mut message = BitswapMessage::new(false);
    for i in 0..2000usize {
        message.add_entry(cid_v1(&i.to_le_bytes()), 1, WantType::Block, false);
    }
    h.send(message);

    assert_eq!(settle(&h).await, Some(1024));
}

#[tokio::test(start_paused = true)]
async fn a_zero_cap_disables_the_bound() {
    let config = ServerConfig {
        max_queued_wantlist_entries_per_peer: 0,
        ..Default::default()
    };
    let mut h = start(&[], config);

    let mut message = BitswapMessage::new(false);
    for i in 0..2000usize {
        message.add_entry(cid_v1(&i.to_le_bytes()), 1, WantType::Block, false);
    }
    h.send(message);

    assert_eq!(settle(&h).await, Some(2000));
}

#[tokio::test(start_paused = true)]
async fn absent_wants_are_evicted_before_lower_priority_ones() {
    let present = blocks(CAP);
    let mut h = start(&present, capped());

    h.send(want_with(cid_v1(b"absent"), 5, false));
    h.send(want_with(present[0].cid, 1, false));
    h.send(want_with(present[1].cid, 1, false));
    h.send(want_with(present[2].cid, 1, false));

    let messages = h.drain().await;
    for block in &present {
        assert!(has_block(&messages, &block.cid));
    }
}

#[tokio::test(start_paused = true)]
async fn overflow_replaces_an_existing_want_of_no_higher_priority() {
    let present = blocks(CAP + 1);
    let mut h = start(&present, capped());

    for (i, block) in present.iter().take(CAP).enumerate() {
        h.send(want_with(block.cid, i as i32 + 1, false));
    }
    h.send(want_with(present[CAP].cid, 9, false));

    let messages = h.drain().await;
    assert!(has_block(&messages, &present[CAP].cid));
    assert!(has_block(&messages, &present[0].cid));
    assert!(has_block(&messages, &present[1].cid));
    assert!(
        !has_block(&messages, &present[2].cid),
        "the evicted want must leave the task queue"
    );
}

#[tokio::test(start_paused = true)]
async fn overflow_that_fits_nowhere_is_dropped_without_dont_have() {
    let present = blocks(CAP);
    let dropped = cid_v1(b"dropped");
    let mut h = start(&present, capped());

    for (i, block) in present.iter().enumerate() {
        h.send(want_with(block.cid, i as i32 + 5, false));
    }
    h.send(want_with(dropped, 1, true));

    let messages = h.drain().await;
    assert!(!has_dont_have(&messages, &dropped));
    for block in &present {
        assert!(has_block(&messages, &block.cid));
    }
}

#[tokio::test(start_paused = true)]
async fn cancel_frees_room_at_the_cap() {
    let wanted = block_v1(b"wanted");
    let mut h = start(std::slice::from_ref(&wanted), capped());
    let missing = fill_with_missing(&mut h).await;

    h.send(cancel(missing[0]));
    assert_eq!(settle(&h).await, Some(CAP - 1));

    h.send(want_with(wanted.cid, 1, false));
    assert!(has_block(&h.drain().await, &wanted.cid));
    assert_eq!(h.server.ledger_wants(h.peer).await, Some(CAP - 1));
}

#[tokio::test(start_paused = true)]
async fn full_wantlist_replaces_the_ledger_at_the_cap() {
    let present = blocks(CAP);
    let mut h = start(&present, capped());
    fill_with_missing(&mut h).await;

    let mut message = BitswapMessage::new(true);
    for block in &present {
        message.add_entry(block.cid, 1, WantType::Block, false);
    }
    h.send(message);

    let messages = h.drain().await;
    for block in &present {
        assert!(has_block(&messages, &block.cid));
    }
    assert_eq!(h.server.ledger_wants(h.peer).await, Some(0));
}

#[tokio::test(start_paused = true)]
async fn updating_a_cid_already_in_the_ledger_is_not_overflow() {
    let mut h = start(&[], capped());
    let missing = fill_with_missing(&mut h).await;

    h.send(want_with(missing[0], 1, true));

    assert!(has_dont_have(&h.drain().await, &missing[0]));
    assert_eq!(h.server.ledger_wants(h.peer).await, Some(CAP));
}

#[derive(Debug, Clone)]
struct HangingStore {
    inner: MemStore,
    hang_on: Vec<Cid>,
    armed: Arc<AtomicBool>,
}

#[async_trait]
impl Store for HangingStore {
    async fn get_size(&self, cid: &Cid) -> anyhow::Result<usize> {
        if self.armed.load(Ordering::SeqCst) && self.hang_on.contains(cid) {
            std::future::pending::<()>().await;
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

#[tokio::test(start_paused = true)]
async fn eviction_never_calls_the_store_from_the_engine() {
    let present = blocks(CAP + 1);
    let armed = Arc::new(AtomicBool::new(false));
    let missing: Vec<Cid> = (0..CAP).map(|i| cid_v1(&[b'm', i as u8])).collect();
    let store = HangingStore {
        inner: MemStore::new(&present),
        hang_on: missing.clone(),
        armed: armed.clone(),
    };
    let mut h = start_with(store, capped());
    for cid in &missing {
        h.send(want_with(*cid, 5, false));
    }
    assert_eq!(settle(&h).await, Some(CAP));

    armed.store(true, Ordering::SeqCst);
    h.send(want_with(present[0].cid, 1, false));

    assert!(has_block(&h.drain().await, &present[0].cid));
}
