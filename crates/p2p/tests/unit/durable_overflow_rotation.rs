use super::*;
use crate::sync::pending_store::{PendingDagStorage, PendingDagStore, PersistedPendingDag};
use crate::sync::{
    BlockMetadata, MergeHandler, MergeOutcome, ReplicationConfig, ReplicationLoop,
    ReplicationResult, SyncConfig, SyncCoordinator,
};
use defra_core::{Block, CompositeDeltaPayload, CrdtDelta, DAGLink, LwwDeltaPayload};

#[derive(Debug, thiserror::Error)]
#[error("merge failed")]
struct MergeFailed;

/// Merges every root it is handed.
struct MergeAll;

#[async_trait]
impl MergeHandler for MergeAll {
    type Error = MergeFailed;

    async fn handle_block(
        &self,
        _cid: &Cid,
        _block_data: &[u8],
        _metadata: BlockMetadata<'_>,
    ) -> Result<MergeOutcome, MergeFailed> {
        Ok(MergeOutcome::Merged)
    }
}

/// A document head, held locally, and the one field block it links, which is
/// missing.
fn head_missing_its_field(doc: &str) -> (Cid, Vec<u8>, Cid, Vec<u8>) {
    let leaf = Block::new(
        CrdtDelta::Lww(LwwDeltaPayload {
            field_name: "name".to_string(),
            priority: 1,
            collection_version_id: "schema1".to_string(),
            data: doc.as_bytes().to_vec(),
        }),
        vec![],
        vec![],
    );
    let leaf_cid = leaf.generate_cid().unwrap();
    let head = Block::new(
        CrdtDelta::Composite(CompositeDeltaPayload {
            collection_version_id: "schema1".to_string(),
            priority: 1,
            status: 1,
        }),
        vec![],
        vec![DAGLink::new("name", leaf_cid)],
    );
    (
        head.generate_cid().unwrap(),
        head.to_dag_cbor().unwrap(),
        leaf_cid,
        leaf.to_dag_cbor().unwrap(),
    )
}

fn record(doc_id: &str, source_peer: &str) -> PersistedPendingDag {
    PersistedPendingDag {
        doc_id: doc_id.to_string(),
        collection_id: "collection".to_string(),
        head_priority: Some(1),
        creator: "creator".to_string(),
        source_peer: Some(source_peer.to_string()),
        alternate_providers: Vec::new(),
        is_explicit_replicator: false,
        explicit_replay_authorization: None,
    }
}

/// The durable store can hold more acknowledged roots than the in-memory
/// pending cap, after an upgrade or a lower cap. Here two of five such roots,
/// first in the store's key order, name providers that never connect. They
/// take both in-memory slots at restore; once each has spent its dispatches
/// it gives its slot back while keeping its record, and the resync carries on
/// past the roots it admitted last, so the other three are fetched and merge.
#[nuclei::test(start_paused = true)]
async fn resync_rotates_past_unfetchable_records() {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store, true));
    let pending_store = Arc::new(PendingDagStore::new(Arc::new(
        RegolithStore::in_memory().unwrap(),
    )));

    let mut roots: Vec<_> = (0..5)
        .map(|doc| head_missing_its_field(&format!("doc-{doc}")))
        .collect();
    roots.sort_unstable_by_key(|(head, ..)| head.to_string());
    let mut fetchable = Vec::new();
    let mut leaves = RapidHashMap::new();
    let mut connected = Vec::new();
    for (index, (head, head_data, leaf, leaf_data)) in roots.iter().enumerate() {
        blockstore.put(head, head_data).await.unwrap();
        let provider = if index < 2 {
            format!("never-connected-{index}")
        } else {
            fetchable.push(*head);
            leaves.insert(*leaf, leaf_data.clone());
            connected.push(PeerId::new(format!("live-{index}")));
            format!("live-{index}")
        };
        pending_store
            .put(head, &record(&format!("doc-{index}"), &provider))
            .await
            .unwrap();
    }

    let (any_root, any_root_data, ..) = roots[0].clone();
    let transport = TestTransport::new(
        blockstore.clone(),
        any_root,
        any_root_data,
        RapidHashMap::new(),
        leaves,
    );
    transport.set_force_rooted_sync();
    transport.set_connected_peers(connected);

    let config = SyncConfig {
        max_pending_dags: 2,
        ..SyncConfig::default()
    };
    let (coordinator, mut events) = SyncCoordinator::new(
        transport,
        blockstore.clone(),
        crate::network::test_network(),
        config,
    )
    .await
    .unwrap();
    let coordinator = Arc::new(coordinator);
    coordinator
        .install_pending_dag_store(pending_store.clone())
        .await;
    assert_eq!(coordinator.restore_pending_dags().await, 2);
    let mut admitted = coordinator.manager().pending_dag_cids();
    admitted.sort_unstable_by_key(Cid::to_string);
    assert_eq!(
        admitted,
        vec![roots[0].0, roots[1].0],
        "the unfetchable roots take both in-memory slots"
    );

    let pump = {
        let coordinator = Arc::clone(&coordinator);
        crate::rt::spawn(async move {
            let config = ReplicationConfig::default();
            while !matches!(
                ReplicationLoop::process_next(&coordinator, &mut events, &MergeAll, &config).await,
                ReplicationResult::ChannelClosed
            ) {}
        })
    };
    let clock = {
        let coordinator = Arc::clone(&coordinator);
        crate::rt::spawn(async move {
            coordinator
                .run_pending_dag_retry_clock(Duration::from_secs(2))
                .await
                .unwrap();
        })
    };
    let resync = {
        let coordinator = Arc::clone(&coordinator);
        crate::rt::spawn(async move {
            coordinator
                .run_pending_dag_resync(Duration::from_secs(60))
                .await;
        })
    };

    let deadline = crate::rt::time::Instant::now() + Duration::from_secs(30 * 60);
    let mut merged = 0;
    while crate::rt::time::Instant::now() < deadline {
        crate::rt::time::sleep(Duration::from_secs(1)).await;
        merged = 0;
        for root in &fetchable {
            if coordinator.manager().is_merged(root).await.unwrap() {
                merged += 1;
            }
        }
        if merged == fetchable.len() {
            break;
        }
    }

    coordinator.shutdown().await;
    clock.await;
    resync.await;
    pump.abort();

    assert_eq!(
        merged,
        fetchable.len(),
        "only {merged} of the {} fetchable durable roots merged in 30 minutes",
        fetchable.len()
    );
    assert_eq!(
        pending_store.load_all().await.unwrap().len(),
        2,
        "the unfetchable roots keep their durable records"
    );
}

fn push(doc_id: &str, head: Cid, head_data: &[u8]) -> PushLogBroadcast {
    PushLogBroadcast::new(
        doc_id.to_string(),
        bytes::Bytes::from(head.to_bytes()),
        "collection".to_string(),
        "creator".to_string(),
        bytes::Bytes::from(head_data.to_vec()),
    )
}

/// The in-memory map is full of acknowledged roots whose providers never
/// connect, and no durable root waits, so none of them gives its entry up.
/// A fresh head from a peer that serves it is offered the way the sender's
/// retry ledger offers it, every 30 s until acknowledged. It is acknowledged
/// at once with a durable record and no entry, the never-serving roots give
/// their entries up to it, and it merges.
#[nuclei::test(start_paused = true)]
async fn a_push_at_the_in_memory_cap_is_acknowledged_and_merged() {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store, true));
    let pending_store = Arc::new(PendingDagStore::new(Arc::new(
        RegolithStore::in_memory().unwrap(),
    )));

    let stalled: Vec<_> = (0..2)
        .map(|doc| head_missing_its_field(&format!("stalled-{doc}")))
        .collect();
    let (fresh, fresh_data, fresh_leaf, fresh_leaf_data) = head_missing_its_field("fresh");
    let mut leaves = RapidHashMap::new();
    leaves.insert(fresh_leaf, fresh_leaf_data);
    let transport = TestTransport::new(
        blockstore.clone(),
        fresh,
        fresh_data.clone(),
        RapidHashMap::new(),
        leaves,
    );
    transport.set_force_rooted_sync();
    transport.set_connected_peers(vec![PeerId::new("live".to_string())]);

    let config = SyncConfig {
        max_pending_dags: 2,
        ..SyncConfig::default()
    };
    let (coordinator, mut events) = SyncCoordinator::new(
        transport,
        blockstore.clone(),
        crate::network::test_network(),
        config,
    )
    .await
    .unwrap();
    let coordinator = Arc::new(coordinator);
    coordinator
        .install_pending_dag_store(pending_store.clone())
        .await;
    for (index, (head, head_data, ..)) in stalled.iter().enumerate() {
        coordinator
            .manager()
            .process_pushlog(
                &push(&format!("stalled-{index}"), *head, head_data),
                Some(&format!("never-connected-{index}")),
                false,
                None,
            )
            .await
            .expect("a never-serving head is acknowledged while the map has room");
    }
    assert_eq!(coordinator.manager().pending_dag_count(), 2);

    let pump = {
        let coordinator = Arc::clone(&coordinator);
        crate::rt::spawn(async move {
            let config = ReplicationConfig::default();
            while !matches!(
                ReplicationLoop::process_next(&coordinator, &mut events, &MergeAll, &config).await,
                ReplicationResult::ChannelClosed
            ) {}
        })
    };
    let clock = {
        let coordinator = Arc::clone(&coordinator);
        crate::rt::spawn(async move {
            coordinator
                .run_pending_dag_retry_clock(Duration::from_secs(2))
                .await
                .unwrap();
        })
    };
    let resync = {
        let coordinator = Arc::clone(&coordinator);
        crate::rt::spawn(async move {
            coordinator
                .run_pending_dag_resync(Duration::from_secs(60))
                .await;
        })
    };

    let offer = push("fresh", fresh, &fresh_data);
    let deadline = crate::rt::time::Instant::now() + Duration::from_secs(30 * 60);
    let mut nacks = 0;
    let mut acked = false;
    let mut merged = false;
    while crate::rt::time::Instant::now() < deadline {
        if !acked {
            match coordinator
                .manager()
                .process_pushlog(&offer, Some("live"), false, None)
                .await
            {
                Ok(()) => acked = true,
                Err(crate::error::Error::PendingDagCapacity { .. }) => nacks += 1,
                Err(error) => panic!("the fresh head was refused: {error}"),
            }
        }
        if acked && coordinator.manager().is_merged(&fresh).await.unwrap() {
            merged = true;
            break;
        }
        crate::rt::time::sleep(Duration::from_secs(30)).await;
    }

    coordinator.shutdown().await;
    clock.join().await.unwrap();
    resync.join().await.unwrap();
    pump.abort();

    assert_eq!(
        (nacks, merged),
        (0, true),
        "the fresh head was nacked at the in-memory cap {nacks} times in 30 minutes \
         (acknowledged: {acked}, merged: {merged})"
    );
    assert_eq!(
        pending_store.load_all().await.unwrap().len(),
        2,
        "the never-serving roots keep their durable records"
    );
}

/// One peer holds more durable roots than its quota of the in-memory map
/// (`max_pending_dags / 4`, one here): `unservable` roots, whose blocks it
/// cannot serve, and `waiting`, which it serves and which sorts after them. A
/// root of a second peer that never connects, `other_peer`, sorts after all of
/// them. The restore admits the first unservable root and `other_peer` and
/// leaves the others waiting on the quota. `other_peer` gives its entry up
/// after its dispatches and comes back at every sweep, so a sweep that resumes
/// after the root it admitted last starts after `other_peer` and re-admits the
/// first unservable root ahead of `waiting` whenever the peer's place is free.
/// The sweep must give the peer's place to its roots in turn, so `waiting` is
/// fetched and merges.
async fn a_peers_roots_take_its_place_in_turn(unservable_roots: usize, edge: edge::Edge) {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store, true));
    let pending_store = Arc::new(PendingDagStore::new(Arc::new(
        RegolithStore::in_memory().unwrap(),
    )));

    let mut roots: Vec<_> = (0..unservable_roots + 2)
        .map(|doc| head_missing_its_field(&format!("quota-doc-{doc}")))
        .collect();
    roots.sort_unstable_by_key(|(head, ..)| head.to_string());
    let unservable: Vec<Cid> = roots[..unservable_roots]
        .iter()
        .map(|(head, ..)| *head)
        .collect();
    let (waiting, _, waiting_leaf, waiting_leaf_data) = roots[unservable_roots].clone();
    let other_peer = roots[unservable_roots + 1].0;
    for (index, (head, head_data, ..)) in roots.iter().enumerate() {
        blockstore.put(head, head_data).await.unwrap();
        let provider = if *head == other_peer {
            "never-connected"
        } else {
            "quota-peer"
        };
        pending_store
            .put(head, &record(&format!("quota-doc-{index}"), provider))
            .await
            .unwrap();
    }

    let (any_root, any_root_data, ..) = roots[0].clone();
    let mut leaves = RapidHashMap::new();
    leaves.insert(waiting_leaf, waiting_leaf_data);
    let transport = TestTransport::new(
        blockstore.clone(),
        any_root,
        any_root_data,
        RapidHashMap::new(),
        leaves,
    );
    transport.set_force_rooted_sync();
    transport.set_connected_peers(vec![PeerId::new("quota-peer".to_string())]);

    let config = SyncConfig {
        max_pending_dags: 4,
        ..SyncConfig::default()
    };
    let (coordinator, mut events) = SyncCoordinator::new(
        transport,
        blockstore.clone(),
        crate::network::Network::on(edge.clone()),
        config,
    )
    .await
    .unwrap();
    let coordinator = Arc::new(coordinator);
    coordinator
        .install_pending_dag_store(pending_store.clone())
        .await;
    assert_eq!(coordinator.restore_pending_dags().await, 2);
    let mut admitted = coordinator.manager().pending_dag_cids();
    admitted.sort_unstable_by_key(Cid::to_string);
    assert_eq!(
        admitted,
        vec![unservable[0], other_peer],
        "the peer's quota leaves its other roots waiting"
    );

    let pump = {
        let coordinator = Arc::clone(&coordinator);
        crate::rt::spawn(async move {
            let config = ReplicationConfig::default();
            while !matches!(
                ReplicationLoop::process_next(&coordinator, &mut events, &MergeAll, &config).await,
                ReplicationResult::ChannelClosed
            ) {}
        })
    };
    let clock = {
        let coordinator = Arc::clone(&coordinator);
        crate::rt::spawn(async move {
            coordinator
                .run_pending_dag_retry_clock(Duration::from_secs(2))
                .await
                .unwrap();
        })
    };
    let resync = {
        let coordinator = Arc::clone(&coordinator);
        crate::rt::spawn(async move {
            coordinator
                .run_pending_dag_resync(Duration::from_secs(60))
                .await;
        })
    };

    let deadline = crate::rt::time::Instant::now() + Duration::from_secs(30 * 60);
    let mut merged = false;
    let mut peer_place_freed = 0usize;
    let mut holding = true;
    while !merged && crate::rt::time::Instant::now() < deadline {
        crate::rt::time::sleep(Duration::from_secs(1)).await;
        let held = coordinator
            .manager()
            .pending_dag_cids()
            .iter()
            .any(|root| unservable.contains(root));
        if holding && !held {
            peer_place_freed += 1;
        }
        holding = held;
        merged = coordinator.manager().is_merged(&waiting).await.unwrap();
    }

    coordinator.shutdown().await;
    clock.join().await.unwrap();
    resync.join().await.unwrap();
    pump.abort();

    assert!(
        peer_place_freed > 0,
        "the unservable roots never gave the peer's place up"
    );
    assert!(
        merged,
        "the waiting root never merged in 30 minutes; its peer freed its place {peer_place_freed} times"
    );
}

#[test]
fn a_root_left_waiting_on_its_peer_quota_takes_the_place_its_peer_frees() {
    edge::testing::paused(|edge| a_peers_roots_take_its_place_in_turn(1, edge));
}

/// The peer's place must go past both unservable roots before `waiting`, not
/// back and forth between them.
#[test]
fn a_root_behind_two_unservable_roots_of_its_peer_takes_its_turn() {
    edge::testing::paused(|edge| a_peers_roots_take_its_place_in_turn(2, edge));
}

/// Yields its work budget on one root after a second of work, as a long pull
/// merge does, and merges every other root it is handed.
struct YieldOn(Cid);

#[async_trait]
impl MergeHandler for YieldOn {
    type Error = MergeFailed;

    async fn handle_block(
        &self,
        cid: &Cid,
        _block_data: &[u8],
        _metadata: BlockMetadata<'_>,
    ) -> Result<MergeOutcome, MergeFailed> {
        if *cid != self.0 {
            return Ok(MergeOutcome::Merged);
        }
        crate::rt::time::sleep(Duration::from_secs(1)).await;
        Ok(MergeOutcome::Yielded)
    }
}

/// A pull merge of a head from `quota-peer` keeps yielding, so its local
/// continuation holds that peer's whole quota (one entry of a map of two), and
/// the peer's durable root `held_back`, first in key order, waits on it.
/// `unservable`, from a peer that never connects, takes the other place, gives
/// it up after its dispatches and comes back at every sweep. `servable`, from
/// a third peer, sorts last. The place `unservable` frees must reach
/// `servable` in turn while the continuation holds `quota-peer` at its quota.
#[nuclei::test(start_paused = true)]
async fn a_peer_held_at_its_quota_does_not_hold_back_other_peers() {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store, true));
    let pending_store = Arc::new(PendingDagStore::new(Arc::new(
        RegolithStore::in_memory().unwrap(),
    )));

    let mut roots: Vec<_> = (0..3)
        .map(|doc| head_missing_its_field(&format!("held-doc-{doc}")))
        .collect();
    roots.sort_unstable_by_key(|(head, ..)| head.to_string());
    let [(held_back, ..), (unservable, ..), (servable, _, servable_leaf, servable_leaf_data)] =
        roots.clone().try_into().unwrap();
    for (index, (head, head_data, ..)) in roots.iter().enumerate() {
        blockstore.put(head, head_data).await.unwrap();
        let provider = match index {
            0 => "quota-peer",
            1 => "never-connected",
            _ => "serving-peer",
        };
        pending_store
            .put(head, &record(&format!("held-doc-{index}"), provider))
            .await
            .unwrap();
    }
    let (continuation, continuation_data, ..) = head_missing_its_field("pulled-doc");
    blockstore
        .put(&continuation, &continuation_data)
        .await
        .unwrap();

    let (any_root, any_root_data, ..) = roots[0].clone();
    let mut leaves = RapidHashMap::new();
    leaves.insert(servable_leaf, servable_leaf_data);
    let transport = TestTransport::new(
        blockstore.clone(),
        any_root,
        any_root_data,
        RapidHashMap::new(),
        leaves,
    );
    transport.set_force_rooted_sync();
    transport.set_connected_peers(vec![PeerId::new("serving-peer".to_string())]);

    let config = SyncConfig {
        max_pending_dags: 2,
        ..SyncConfig::default()
    };
    let (coordinator, mut events) = SyncCoordinator::new(
        transport,
        blockstore.clone(),
        crate::network::test_network(),
        config,
    )
    .await
    .unwrap();
    let coordinator = Arc::new(coordinator);
    coordinator
        .install_pending_dag_store(pending_store.clone())
        .await;
    assert!(coordinator.manager().continue_merge(
        &continuation,
        Some(&BlockMetadata::normal(
            "pulled-doc",
            "collection",
            "creator",
            Some("quota-peer"),
            false,
        )),
    ));
    assert_eq!(coordinator.restore_pending_dags().await, 1);
    let mut admitted = coordinator.manager().pending_dag_cids();
    admitted.sort_unstable_by_key(Cid::to_string);
    let mut expected = vec![continuation, unservable];
    expected.sort_unstable_by_key(Cid::to_string);
    assert_eq!(
        admitted, expected,
        "the continuation holds the peer's quota and the unservable root the other place"
    );

    let pump = {
        let coordinator = Arc::clone(&coordinator);
        crate::rt::spawn(async move {
            let config = ReplicationConfig::default();
            let merger = YieldOn(continuation);
            while !matches!(
                ReplicationLoop::process_next(&coordinator, &mut events, &merger, &config).await,
                ReplicationResult::ChannelClosed
            ) {}
        })
    };
    let clock = {
        let coordinator = Arc::clone(&coordinator);
        crate::rt::spawn(async move {
            coordinator
                .run_pending_dag_retry_clock(Duration::from_secs(2))
                .await
                .unwrap();
        })
    };
    let resync = {
        let coordinator = Arc::clone(&coordinator);
        crate::rt::spawn(async move {
            coordinator
                .run_pending_dag_resync(Duration::from_secs(60))
                .await;
        })
    };

    let deadline = crate::rt::time::Instant::now() + Duration::from_secs(30 * 60);
    let mut merged = false;
    let mut place_freed = 0usize;
    let mut holding = true;
    let mut continuation_held = true;
    while !merged && crate::rt::time::Instant::now() < deadline {
        crate::rt::time::sleep(Duration::from_secs(1)).await;
        let held = coordinator.manager().pending_dag_cids();
        if holding && !held.contains(&unservable) {
            place_freed += 1;
        }
        holding = held.contains(&unservable);
        continuation_held &= held.contains(&continuation);
        merged = coordinator.manager().is_merged(&servable).await.unwrap();
    }

    coordinator.shutdown().await;
    clock.join().await.unwrap();
    resync.join().await.unwrap();
    pump.abort();

    assert!(
        continuation_held,
        "the continuation gave the peer's quota up"
    );
    assert!(
        !coordinator
            .manager()
            .pending_dag_cids()
            .contains(&held_back),
        "the held-back root took a place its peer never freed"
    );
    assert!(
        place_freed > 0,
        "the unservable root never gave its place up"
    );
    assert!(
        merged,
        "the servable root never merged in 30 minutes; the unservable root freed its place {place_freed} times"
    );
}
