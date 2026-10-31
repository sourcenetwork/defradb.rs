use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use blockstore::DefraBlockstore;
use storage::RegolithStore;

use super::access_tests::NoopTransport;
use super::{SyncCoordinator, NON_AUTHORITATIVE_BROADCAST_TASK_LIMIT};
use crate::network::Network;
use crate::sync::SyncConfig;

type Coordinator = Arc<SyncCoordinator<DefraBlockstore<RegolithStore>, NoopTransport>>;

/// The bytes of pending gossip the room-bounded tests allow.
const ROOM: usize = 1_000;

async fn coordinator() -> Coordinator {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store, true));
    let (coordinator, _events) =
        SyncCoordinator::new(NoopTransport::new(), blockstore, SyncConfig::default())
            .await
            .unwrap();
    Arc::new(coordinator)
}

/// Every publish slot, held until the returned permits are dropped.
fn take_every_slot(coordinator: &Coordinator) -> Vec<regolith::sync::OwnedSemaphorePermit> {
    (0..NON_AUTHORITATIVE_BROADCAST_TASK_LIMIT)
        .map(|_| {
            coordinator
                .runtime
                .shutdown
                .try_acquire_non_authoritative_broadcast_slot()
                .expect("a free slot")
        })
        .collect()
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
    let _slots = take_every_slot(&coordinator);
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
}

/// A coordinator on an edge whose waits for room or a slot last `deadline`,
/// with all [`ROOM`] bytes of pending gossip held by one task that runs until
/// the returned sender is dropped.
async fn full_room(
    edge: &edge::Edge,
    deadline: Duration,
) -> (Coordinator, regolith::sync::UnboundedSender<()>) {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store, true));
    let config = SyncConfig {
        broadcast_wait_deadline: deadline,
        broadcast_pending_bytes: ROOM,
        ..SyncConfig::default()
    };
    let (coordinator, _events) = SyncCoordinator::new(NoopTransport::new(), blockstore, config)
        .await
        .unwrap();
    let coordinator = Arc::new(coordinator);
    coordinator.use_network(Network::on(edge.clone()));
    let (hold, held) = regolith::sync::unbounded::<()>();
    coordinator
        .spawn_non_authoritative_broadcast_task("filler", ROOM, async move {
            let _ = held.recv_async().await;
        })
        .await
        .unwrap();
    (coordinator, hold)
}

#[test]
fn a_committed_writes_gossip_waits_for_room_instead_of_being_shed() {
    let edge = edge::testing::edge();
    edge.block_on(async {
        let (coordinator, hold) = full_room(&edge, Duration::from_secs(30)).await;
        let (ran, done) = regolith::sync::unbounded::<()>();
        let network = coordinator.network();
        let (waited, ()) = futures::join!(
            coordinator.spawn_non_authoritative_broadcast_task("late", 10, async move {
                let _ = ran.try_send(());
            }),
            async {
                network.sleep(Duration::from_millis(20)).await;
                drop(hold);
            }
        );
        assert_eq!(waited, Ok(()), "freed room takes the waiting broadcast");
        done.recv_async().await.unwrap();
    })
    .unwrap();
}

#[test]
fn past_the_wait_deadline_the_gossip_is_dropped_counted_and_returned() {
    let edge = edge::testing::edge();
    edge.block_on(async {
        let (coordinator, _hold) = full_room(&edge, Duration::from_millis(20)).await;
        let late = coordinator
            .spawn_non_authoritative_broadcast_task("late", 10, async {
                panic!("a dropped broadcast must not run");
            })
            .await;
        assert_eq!(late, Err(crate::network::Elapsed));
        assert_eq!(
            coordinator
                .runtime
                .shutdown
                .non_authoritative_broadcast_stats()
                .2,
            1
        );
    })
    .unwrap();
}

#[test]
fn spawned_gossip_holds_room_but_no_publish_slot() {
    let edge = edge::testing::edge();
    edge.block_on(async {
        let (coordinator, _hold) = full_room(&edge, Duration::from_secs(30)).await;
        assert_eq!(coordinator.runtime.broadcast_room.in_use(), ROOM);
        assert_eq!(
            coordinator
                .runtime
                .shutdown
                .non_authoritative_broadcast_stats()
                .0,
            0,
            "a slot is taken only around a publish, never across a coalescing window"
        );
    })
    .unwrap();
}

#[test]
fn a_publish_slot_wait_past_its_deadline_is_counted() {
    let edge = edge::testing::edge();
    edge.block_on(async {
        let (coordinator, _hold) = full_room(&edge, Duration::from_millis(20)).await;
        let _slots = take_every_slot(&coordinator);
        assert!(coordinator.publish_slot().await.is_none());
        assert_eq!(
            coordinator
                .runtime
                .shutdown
                .non_authoritative_broadcast_stats()
                .2,
            1
        );
    })
    .unwrap();
}
