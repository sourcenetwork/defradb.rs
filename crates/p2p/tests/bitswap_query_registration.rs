#![cfg(feature = "test-utils")]

//! A Bitswap fetch that finishes before its query is registered must still
//! leave the query registry empty.
//!
//! `handle_bitswap_sync` spawns the fetch task and then records its abort
//! handle. The task removes its own entry when it ends, so an entry inserted
//! after the task has already finished is one nothing will ever remove: the
//! registry grows by one per fetch and `bitswap_cancel` reports success for a
//! query that is long gone. Registration has to be ordered ahead of
//! completion. This pins that by stalling the handler between spawning the
//! task and registering it, for far longer than the whole fetch takes: if the
//! task is free to run in that window it will finish first, every time.

use std::sync::atomic::Ordering;
use std::time::Duration;

use defra_core::{Block as DefraBlock, CompositeDeltaPayload, CrdtDelta};
use p2p::testutil::{MockBitswapStore, BITSWAP_REGISTER_STALL_MS};
use p2p::{HostEvent, P2PHost};
use tokio::time::timeout;

/// A fetch with nothing to fetch ends in single-digit milliseconds, so this is
/// two orders of magnitude more than enough for it to finish inside the window.
const STALL: Duration = Duration::from_secs(1);

fn make_data_block() -> cid::Cid {
    let payload = CompositeDeltaPayload {
        priority: 1,
        schema_version_id: "bafyusers".to_string(),
        status: 1,
    };
    let block = DefraBlock::new(CrdtDelta::Composite(payload), Vec::new(), Vec::new());
    let bytes = block.to_dag_cbor().unwrap();
    defra_core::block::generate_cid_from_bytes(&bytes).unwrap()
}

#[tokio::test]
async fn a_fetch_completing_before_registration_leaves_no_query_behind() {
    let cid = make_data_block();
    let (host, handle, mut events, _) = P2PHost::new(MockBitswapStore::new()).await.unwrap();
    tokio::spawn(host.run());

    BITSWAP_REGISTER_STALL_MS.store(STALL.as_millis() as u64, Ordering::Relaxed);
    // Nothing missing, so the fetch task runs straight to completion.
    let query = handle.bitswap_sync(cid, vec![], vec![]).await.unwrap();
    BITSWAP_REGISTER_STALL_MS.store(0, Ordering::Relaxed);

    // The fetch itself has to reach its end, since that is what runs the
    // removal half of the bookkeeping this asserts on.
    let completed = timeout(Duration::from_secs(5), async {
        loop {
            match events.recv().await.expect("host event channel closed") {
                HostEvent::BitswapComplete { query_id, .. } if query_id == query => return,
                _ => continue,
            }
        }
    })
    .await;
    completed.expect("the fetch never completed");

    let cancelled = handle.bitswap_cancel(query).await.unwrap();
    assert!(
        !cancelled,
        "query {query:?} ran to completion, but is still registered: its entry \
         was re-added after the fetch task had already removed it, and nothing \
         will ever remove it again"
    );

    handle.shutdown().await.ok();
}
