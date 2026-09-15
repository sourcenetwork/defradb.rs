#![cfg(feature = "test-utils")]

//! Cancelling a Bitswap query must actually stop its session.
//!
//! `handle_bitswap_cancel` aborts the fetch task and stops the session by id.
//! `abort` only schedules cancellation, so the task's `Session` clone can still
//! be alive when the stop runs, and `Session::stop` refuses to run at all while
//! any other handle exists (`ensure!(count == 2)`, iroh-bitswap
//! `client/session.rs`): the stop returns an error, the session stays in the
//! manager's map, and its worker tasks run forever. That is the same leak this
//! branch removed from the completion path, surviving on the cancel path.
//!
//! The interleaving is forced rather than raced: the fetch task blocks its
//! worker thread while holding its clone, which is a window an `abort` cannot
//! land inside, so the stop is guaranteed to meet a live clone. The runtime's
//! task census is the observable, as in `bitswap_session_lifecycle`.

use std::sync::atomic::Ordering;
use std::time::Duration;

use defra_core::{Block as DefraBlock, CompositeDeltaPayload, CrdtDelta};
use p2p::testutil::{MockBitswapStore, BITSWAP_FETCH_BLOCK_MS};
use p2p::{P2PHost, P2PHostHandle};

/// How long the fetch task holds its worker thread. Long enough that the
/// cancel below lands squarely inside the window.
const BLOCK: Duration = Duration::from_millis(400);
/// Time given to the fetch task to reach the block before it is cancelled.
const REACH_BLOCK: Duration = Duration::from_millis(120);

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

fn live_tasks() -> usize {
    tokio::runtime::Handle::current()
        .metrics()
        .num_alive_tasks()
}

/// Waits for the task census to hold still for `hold`, and returns it.
async fn settled_live_tasks(hold: Duration) -> usize {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
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

/// Starts a fetch that can never complete, waits for it to take hold of its
/// session, then cancels it.
async fn cancelled_fetch(handle: &P2PHostHandle, cid: cid::Cid) {
    let query = handle.bitswap_sync(cid, vec![], vec![cid]).await.unwrap();
    tokio::time::sleep(REACH_BLOCK).await;
    assert!(
        handle.bitswap_cancel(query).await.unwrap(),
        "a fetch still in flight must be cancellable"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_cancelled_bitswap_fetch_leaves_no_session_tasks_behind() {
    let cid = make_data_block();
    let (host, handle, _events, _) = P2PHost::new(MockBitswapStore::new()).await.unwrap();
    tokio::spawn(host.run());

    BITSWAP_FETCH_BLOCK_MS.store(BLOCK.as_millis() as u64, Ordering::Relaxed);

    // One cancellation first, so anything the host spawns lazily on its first
    // Bitswap exchange is inside the baseline rather than counted as a leak.
    cancelled_fetch(&handle, cid).await;
    let baseline = settled_live_tasks(Duration::from_millis(300)).await;

    const FETCHES: usize = 3;
    for _ in 0..FETCHES {
        cancelled_fetch(&handle, cid).await;
    }
    let after = settled_live_tasks(Duration::from_millis(300)).await;

    BITSWAP_FETCH_BLOCK_MS.store(0, Ordering::Relaxed);

    assert!(
        after <= baseline,
        "{FETCHES} cancelled Bitswap fetches left {} live tasks behind ({} per fetch): \
         the session stop ran while the aborted task still held a handle, so it \
         refused and the session is still registered and running",
        after.saturating_sub(baseline),
        after.saturating_sub(baseline) / FETCHES,
    );

    handle.shutdown().await.ok();
}
