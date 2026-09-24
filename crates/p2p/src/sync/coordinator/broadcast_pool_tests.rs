use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use blockstore::DefraBlockstore;
use storage::RegolithStore;

use super::access_tests::NoopTransport;
use super::{SyncCoordinator, NON_AUTHORITATIVE_BROADCAST_TASK_LIMIT};
use crate::sync::SyncConfig;

async fn coordinator() -> Arc<SyncCoordinator<DefraBlockstore<RegolithStore>, NoopTransport>> {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store, true));
    let (coordinator, _events) =
        SyncCoordinator::new(NoopTransport::new(), blockstore, SyncConfig::default())
            .await
            .unwrap();
    Arc::new(coordinator)
}

#[tokio::test]
async fn spawn_or_run_detaches_while_a_slot_is_free() {
    let coordinator = coordinator().await;
    let (release_tx, release_rx) = tokio::sync::oneshot::channel::<()>();
    let finished = Arc::new(AtomicBool::new(false));
    let observed = Arc::clone(&finished);

    coordinator
        .spawn_or_run_non_authoritative_broadcast("test", async move {
            let _ = release_rx.await;
            observed.store(true, Ordering::SeqCst);
        })
        .await;

    assert!(
        !finished.load(Ordering::SeqCst),
        "a free slot means the future was detached"
    );
    let _ = release_tx.send(());
}

#[tokio::test]
async fn spawn_or_run_runs_inline_when_the_pool_is_full() {
    let coordinator = coordinator().await;
    let hold = Arc::new(tokio::sync::Notify::new());
    for _ in 0..NON_AUTHORITATIVE_BROADCAST_TASK_LIMIT {
        let hold = Arc::clone(&hold);
        coordinator.spawn_non_authoritative_broadcast_task("filler", async move {
            hold.notified().await;
        });
    }
    let ran = Arc::new(AtomicBool::new(false));
    let observed = Arc::clone(&ran);

    coordinator
        .spawn_or_run_non_authoritative_broadcast("test", async move {
            observed.store(true, Ordering::SeqCst);
        })
        .await;

    assert!(
        ran.load(Ordering::SeqCst),
        "a full pool must run the future inline, not drop it"
    );
    hold.notify_waiters();
}
