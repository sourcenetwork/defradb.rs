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

async fn coordinator_on(edge: &edge::Edge) -> Coordinator {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store, true));
    let (coordinator, _events) = SyncCoordinator::new(
        NoopTransport::new(),
        blockstore,
        crate::network::Network::on(edge.clone()),
        SyncConfig::default(),
    )
    .await
    .unwrap();
    Arc::new(coordinator)
}

async fn coordinator() -> Coordinator {
    coordinator_on(&edge::testing::edge()).await
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

#[nuclei::test]
async fn spawn_or_run_detaches_while_a_slot_is_free() {
    let coordinator = coordinator().await;
    let (release_tx, release_rx) = crate::rt::oneshot::channel::<()>();
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

#[nuclei::test]
async fn spawn_or_run_runs_inline_when_the_pool_is_full() {
    let coordinator = coordinator().await;
    let _slots = take_every_slot(&coordinator);
    let hold = Arc::new(crate::rt::Notify::new());
    for _ in 0..NON_AUTHORITATIVE_BROADCAST_TASK_LIMIT {
        let hold = Arc::clone(&hold);
        coordinator
            .spawn_non_authoritative_broadcast_task("filler", 0, async move {
                hold.notified().await;
            })
            .await
            .unwrap();
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
    let (coordinator, _events) = SyncCoordinator::new(
        NoopTransport::new(),
        blockstore,
        crate::network::Network::on(edge.clone()),
        config,
    )
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

#[test]
fn waiting_gossip_retains_one_task_handle_however_many_writes_wait() {
    // #1099: each write's gossip waits out its coalescing window; as one
    // spawned task each, the retained handles grew with the writes in flight.
    const WRITES: usize = 200;
    let edge = edge::testing::edge();
    edge.block_on(async {
        let coordinator = coordinator_on(&edge).await;
        coordinator.use_network(Network::on(edge.clone()));
        let baseline = coordinator.runtime.shutdown.retained_task_count();
        let (hold, held) = regolith::sync::unbounded::<()>();
        let (finished, done) = regolith::sync::unbounded::<()>();
        for _ in 0..WRITES {
            let (held, finished) = (held.clone(), finished.clone());
            coordinator
                .spawn_non_authoritative_broadcast_task("write", 10, async move {
                    let _ = held.recv_async().await;
                    let _ = finished.try_send(());
                })
                .await
                .unwrap();
        }
        coordinator.network().sleep(Duration::from_millis(20)).await;
        assert!(
            coordinator.runtime.shutdown.retained_task_count() <= baseline,
            "{WRITES} waiting writes retained {} task handles over a baseline of {baseline}",
            coordinator.runtime.shutdown.retained_task_count()
        );
        assert_eq!(coordinator.runtime.broadcast_room.in_use(), WRITES * 10);
        drop((hold, held));
        for _ in 0..WRITES {
            done.recv_async().await.unwrap();
        }
    })
    .unwrap();
}

/// A head block with a priority, so the coalescer can order it.
fn head_block() -> (cid::Cid, Vec<u8>) {
    use defra_core::{Block, CompositeDeltaPayload, CrdtDelta};

    let block = Block::new_with_options(
        CrdtDelta::Composite(CompositeDeltaPayload {
            collection_version_id: "schema".to_string(),
            priority: 1,
            status: 1,
        }),
        vec![],
        vec![],
        None,
        None,
    );
    let bytes = block.to_dag_cbor().unwrap();
    let cid = defra_core::block::generate_cid_from_bytes(&bytes).unwrap();
    (cid, bytes)
}

/// A write committed while no peer is connected is announced at once, to no
/// one, as Go's `SendUpdate` does. It is not held in the coalescing window for
/// a peer that connects and subscribes during it (#2841).
#[nuclei::test]
async fn a_write_with_no_peer_connected_is_not_held_for_a_peer_that_joins() {
    let transport = NoopTransport::new();
    transport.set_publish_open(false);
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store, true));
    let (coordinator, _events) = SyncCoordinator::new(
        transport.clone(),
        blockstore,
        crate::network::Network::on(edge::testing::edge()),
        SyncConfig::default(),
    )
    .await
    .unwrap();
    let (cid, block) = head_block();

    // The write's announcement is polled first; then, before the window
    // would close, a peer connects and subscribes.
    let (announced, ()) = futures::join!(
        coordinator.broadcast_local_update(&cid, &block, "doc", "collection"),
        async {
            coordinator.access.peer_state.peer_connected("joiner");
            transport.set_publish_open(true);
        },
    );

    assert!(
        announced
            .as_ref()
            .is_err_and(|error| error.to_string().contains("InsufficientPeers")),
        "the write was announced to the peer that joined after it: {announced:?}"
    );
    assert!(transport.published().is_empty());
}
