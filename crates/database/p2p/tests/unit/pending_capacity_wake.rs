//! Freed pending-DAG capacity wakes queued work without waiting for the retry clock.

use std::sync::Arc;
use std::time::Duration;

use blockstore::DefraBlockstore;
use storage::RegolithStore;

use super::tests::{create_composite_block, create_lww_block, make_broadcast, TestTransport};
use crate::sync::{SyncConfig, SyncCoordinator, SyncEvent};

#[test]
fn completed_pending_fetch_wakes_queued_work_without_a_clock_tick() {
    edge::testing::paused(|edge| assert_pending_capacity_release_wakes_dispatch(edge, false));
}

#[test]
fn final_pending_merge_owner_wakes_queued_work_without_a_clock_tick() {
    edge::testing::paused(|edge| assert_pending_capacity_release_wakes_dispatch(edge, true));
}

async fn assert_pending_capacity_release_wakes_dispatch(edge: edge::Edge, merging: bool) {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store, true));
    let (coordinator, mut events) = SyncCoordinator::new(
        TestTransport::new(),
        blockstore,
        crate::network::Network::on(edge.clone()),
        SyncConfig {
            max_concurrent_dag_fetches: 4,
            ..SyncConfig::default()
        },
    )
    .await
    .unwrap();
    let coordinator = Arc::new(coordinator);
    let mut roots = Vec::new();
    for index in 0..5 {
        // One peer each: this exercises the global slots, not the per-peer cap.
        let peer = format!("peer-{index}");
        let field = format!("field-{index}");
        let (field_cid, _) = create_lww_block(&field);
        let (root, block) = create_composite_block(&format!("queued-{index}"), &field, field_cid);
        coordinator
            .manager()
            .process_pushlog(
                &make_broadcast(&format!("queued-{index}"), root, block, "collection1"),
                Some(peer.as_str()),
                false,
                None,
            )
            .await
            .unwrap();
        if merging {
            coordinator
                .manager()
                .schedule_pending_merge_continuation(&root);
        }
        roots.push(root);
    }
    assert_eq!(
        coordinator.dispatch_due_pending_dag_fetches_for_test(crate::rt::time::Instant::now()),
        4
    );
    let mut releases = Vec::new();
    let mut owners = Vec::new();
    for _ in 0..4 {
        let root = match (merging, events.try_recv().unwrap()) {
            (false, SyncEvent::DagNeedsFetch { root_cid, .. })
            | (true, SyncEvent::DagReady { root_cid, .. }) => root_cid,
            (_, other) => panic!("expected pending dispatch, got {other:?}"),
        };
        roots.retain(|queued| *queued != root);
        if merging {
            owners.push(coordinator.pending_dag_merge_guard(root).unwrap());
        } else {
            let (release, released) = crate::rt::oneshot::channel::<()>();
            assert!(coordinator.spawn_pending_dag_fetch_task(
                root,
                "test.pending.capacity",
                async move {
                    let _ = released.await;
                }
            ));
            releases.push(release);
        }
    }
    assert_eq!(roots.len(), 1);
    assert_eq!(coordinator.pending_dag_work_count(), 4);
    let runner = Arc::clone(&coordinator);
    let clock = edge.spawn(async move {
        runner
            .run_pending_dag_retry_clock(Duration::from_secs(2))
            .await
            .unwrap();
    });
    // Drain the initial tick and registration signal while all slots are held.
    for _ in 0..16 {
        crate::rt::yield_now().await;
    }
    assert!(events.try_recv().is_err());
    let before_release = crate::rt::time::Instant::now();
    if merging {
        let owner = owners.remove(0);
        let last_owner = Arc::clone(&owner);
        drop(owner);
        for _ in 0..16 {
            crate::rt::yield_now().await;
        }
        assert_eq!(
            coordinator.pending_dag_work_count(),
            4,
            "a remaining clone keeps its slot"
        );
        assert!(events.try_recv().is_err());
        drop(last_owner);
    } else {
        releases.remove(0).send(()).unwrap();
    }
    // The paused clock moves only if this wake is missing, which the clock
    // check below refuses.
    let dispatched = events.recv().await;
    assert!(
        matches!((merging, &dispatched),
            (false, Some(SyncEvent::DagNeedsFetch { root_cid, .. }))
            | (true, Some(SyncEvent::DagReady { root_cid, .. })) if *root_cid == roots[0]),
        "free capacity must wake the existing owner; retained work={}, event={dispatched:?}",
        coordinator.pending_dag_work_count()
    );
    assert_eq!(
        crate::rt::time::Instant::now(),
        before_release,
        "no retry tick may be required"
    );
    assert_eq!(
        coordinator.sync_status().await.pending_dag_retry_dispatched,
        5
    );
    drop(owners);
    drop(releases);
    coordinator.shutdown().await;
    clock.await.unwrap();
}

#[nuclei::test(start_paused = true)]
async fn a_full_dispatch_lane_does_not_self_wake_pending_dispatch() {
    use futures::FutureExt;
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store, true));
    let (coordinator, mut events) = SyncCoordinator::new(
        TestTransport::new(),
        blockstore,
        crate::network::test_network(),
        SyncConfig::default(),
    )
    .await
    .unwrap();
    let (field, _) = create_lww_block("full-event-queue");
    let (root, block) = create_composite_block("queued", "full-event-queue", field);
    coordinator
        .manager()
        .process_pushlog(
            &make_broadcast("queued", root, block, "collection1"),
            Some("peer-1"),
            false,
            None,
        )
        .await
        .unwrap();
    while coordinator.manager().can_dispatch() {
        assert!(coordinator.manager().dispatch(SyncEvent::SyncError {
            cid: field,
            error: "filler".into(),
        }));
    }
    assert_eq!(
        coordinator.dispatch_due_pending_dag_fetches_for_test(crate::rt::time::Instant::now()),
        0
    );
    assert_eq!(coordinator.pending_dag_work_count(), 0);
    assert!(
        coordinator
            .runtime
            .shutdown
            .inner
            .pending_dag_capacity_released
            .notified()
            .now_or_never()
            .is_none(),
        "a full lane must not repeatedly wake its own retry"
    );
    while events.try_recv().is_ok() {}
    assert_eq!(
        coordinator.dispatch_due_pending_dag_fetches_for_test(crate::rt::time::Instant::now()),
        1
    );
    assert!(
        matches!(events.try_recv().unwrap(), SyncEvent::DagNeedsFetch { root_cid, .. } if root_cid == root)
    );
    coordinator.shutdown().await;
}
