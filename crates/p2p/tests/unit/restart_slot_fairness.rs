use super::*;
use crate::sync::pending_store::{PendingDagStorage, PendingDagStore, PersistedPendingDag};
use crate::sync::{
    BlockMetadata, MergeHandler, MergeOutcome, ReplicationConfig, ReplicationLoop,
    ReplicationResult, SyncConfig, SyncCoordinator, DEFAULT_MAX_CONCURRENT_DAG_FETCHES,
};
use defra_core::{Block, CompositeDeltaPayload, CrdtDelta, DAGLink, LwwDeltaPayload};

#[derive(Debug, thiserror::Error)]
#[error("no merge expected")]
struct NoMergeExpected;

struct NoMerge;

#[async_trait]
impl MergeHandler for NoMerge {
    type Error = NoMergeExpected;

    async fn handle_block(
        &self,
        _cid: &Cid,
        _block_data: &[u8],
        _metadata: BlockMetadata<'_>,
    ) -> Result<MergeOutcome, NoMergeExpected> {
        Err(NoMergeExpected)
    }
}

/// A document head whose one field block is missing locally.
fn head_missing_its_field(field: &str) -> (Cid, Vec<u8>, Cid) {
    let leaf = Block::new(
        CrdtDelta::Lww(LwwDeltaPayload {
            field_name: field.to_string(),
            priority: 1,
            collection_version_id: "schema1".to_string(),
            data: b"value".to_vec(),
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
        vec![DAGLink::new(field, leaf_cid)],
    );
    (
        head.generate_cid().unwrap(),
        head.to_dag_cbor().unwrap(),
        leaf_cid,
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

/// After a restart the receiver clock hands every fetch slot to restored
/// catch-up roots whose providers stall. A slot comes back after every
/// attempt, and the root that attempt was for is due again only a backoff
/// after it ended, so a root acknowledged meanwhile takes the first freed
/// slot instead of waiting out every holder's retries.
#[tokio::test(start_paused = true)]
async fn a_root_acked_after_restart_takes_the_first_slot_a_stalled_catch_up_frees() {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store, true));
    let pending_store = Arc::new(PendingDagStore::new(Arc::new(
        RegolithStore::in_memory().unwrap(),
    )));

    let mut catch_up = Vec::new();
    for slot in 0..DEFAULT_MAX_CONCURRENT_DAG_FETCHES {
        let (head, head_data, _) = head_missing_its_field(&format!("field{slot}"));
        blockstore.put(&head, &head_data).await.unwrap();
        pending_store
            .put(
                &head,
                &record(&format!("catch-up-{slot}"), &format!("stalled-{slot}")),
            )
            .await
            .unwrap();
        catch_up.push(head);
    }
    let (fresh, fresh_data, _) = head_missing_its_field("fresh");

    let transport = TestTransport::new(
        blockstore.clone(),
        fresh,
        fresh_data.clone(),
        RapidHashMap::new(),
        RapidHashMap::new(),
    );
    transport.set_force_rooted_sync();
    let stalled: Vec<PeerId> = (0..=DEFAULT_MAX_CONCURRENT_DAG_FETCHES)
        .map(|slot| PeerId::new(format!("stalled-{slot}")))
        .collect();
    for peer in &stalled {
        transport.mark_provider_dead(peer.as_str());
    }
    transport.set_connected_peers(stalled);

    let (coordinator, mut events) =
        SyncCoordinator::new(transport, blockstore.clone(), SyncConfig::default())
            .await
            .unwrap();
    let coordinator = Arc::new(coordinator);
    coordinator
        .install_pending_dag_store(pending_store.clone())
        .await;
    assert_eq!(
        coordinator.restore_pending_dags().await,
        DEFAULT_MAX_CONCURRENT_DAG_FETCHES
    );

    let dispatched = Arc::new(SegQueue::new());
    let pump = {
        let coordinator = Arc::clone(&coordinator);
        let dispatched = Arc::clone(&dispatched);
        n0_future::task::spawn(async move {
            let config = ReplicationConfig::default();
            loop {
                match ReplicationLoop::process_next(&coordinator, &mut events, &NoMerge, &config)
                    .await
                {
                    ReplicationResult::DagFetchStarted { root_cid } => {
                        dispatched.push((root_cid, n0_future::time::Instant::now()));
                    }
                    ReplicationResult::ChannelClosed => return,
                    _ => {}
                }
            }
        })
    };
    let clock = {
        let coordinator = Arc::clone(&coordinator);
        n0_future::task::spawn(async move {
            coordinator
                .run_pending_dag_retry_clock(Duration::from_secs(2))
                .await
                .unwrap();
        })
    };

    let mut order = Vec::new();
    while order.len() < DEFAULT_MAX_CONCURRENT_DAG_FETCHES {
        n0_future::time::sleep(Duration::from_millis(10)).await;
        order.extend(std::iter::from_fn(|| dispatched.pop()));
    }
    let mut first_wave: Vec<Cid> = order.iter().map(|(root, _)| *root).collect();
    first_wave.sort_unstable_by_key(Cid::to_bytes);
    catch_up.sort_unstable_by_key(Cid::to_bytes);
    assert_eq!(first_wave, catch_up, "the restored roots hold every slot");

    // Acked past the first retry rung of the slot holders, as in the fleet.
    n0_future::time::sleep(Duration::from_secs(10)).await;
    coordinator
        .manager()
        .process_pushlog(
            &PushLogBroadcast::new(
                "fresh".to_string(),
                bytes::Bytes::from(fresh.to_bytes()),
                "collection".to_string(),
                "creator".to_string(),
                bytes::Bytes::from(fresh_data),
            ),
            Some("stalled-4"),
            false,
            None,
        )
        .await
        .expect("the fresh head is acknowledged");
    let acked_at = n0_future::time::Instant::now();

    let deadline = acked_at + Duration::from_secs(30 * 60);
    while !order.iter().any(|(root, _)| *root == fresh)
        && n0_future::time::Instant::now() < deadline
    {
        n0_future::time::sleep(Duration::from_millis(100)).await;
        order.extend(std::iter::from_fn(|| dispatched.pop()));
    }
    let after_first_wave: Vec<Cid> = order[DEFAULT_MAX_CONCURRENT_DAG_FETCHES..]
        .iter()
        .map(|(root, _)| *root)
        .collect();
    let waited = order
        .iter()
        .find(|(root, _)| *root == fresh)
        .map(|(_, at)| at.duration_since(acked_at));

    coordinator.shutdown().await;
    clock.await.unwrap();
    pump.abort();

    let waited = waited.expect("the acked root is dispatched");
    let overtaken_by = after_first_wave
        .iter()
        .take_while(|root| **root != fresh)
        .count();
    assert_eq!(
        overtaken_by, 0,
        "{overtaken_by} slot-holder retries overtook the acked root, dispatched {waited:?} after its ack"
    );
    assert!(
        waited <= 2 * BLOCK_SYNC_COMPLETION_WATCHDOG,
        "the acked root waited {waited:?}, longer than one stalled attempt"
    );
}

/// After a restart one spoke's restored roots fill the front of the clock's
/// queue. The limiter lets one source peer hold all but one fetch permit, so
/// the clock gives a root of that peer a slot only with one of its peer's
/// permits: the last slot goes to another peer's root, not to a fetch that
/// would hold it while it waits for a permit.
#[tokio::test(start_paused = true)]
async fn a_slot_goes_to_another_peer_while_a_saturated_peer_has_no_free_permit() {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store, true));
    let pending_store = Arc::new(PendingDagStore::new(Arc::new(
        RegolithStore::in_memory().unwrap(),
    )));

    let mut spoke_roots = Vec::new();
    for slot in 0..DEFAULT_MAX_CONCURRENT_DAG_FETCHES {
        let (head, head_data, _) = head_missing_its_field(&format!("field{slot}"));
        blockstore.put(&head, &head_data).await.unwrap();
        pending_store
            .put(&head, &record(&format!("spoke-{slot}"), "spoke-d"))
            .await
            .unwrap();
        spoke_roots.push(head);
    }
    let (fresh, fresh_data, _) = head_missing_its_field("fresh");

    let transport = TestTransport::new(
        blockstore.clone(),
        fresh,
        fresh_data.clone(),
        RapidHashMap::new(),
        RapidHashMap::new(),
    );
    transport.set_force_rooted_sync();
    for peer in ["spoke-d", "spoke-e"] {
        transport.mark_provider_dead(peer);
    }
    transport.set_connected_peers(vec![
        PeerId::new("spoke-d".to_string()),
        PeerId::new("spoke-e".to_string()),
    ]);

    let (coordinator, mut events) =
        SyncCoordinator::new(transport, blockstore.clone(), SyncConfig::default())
            .await
            .unwrap();
    let coordinator = Arc::new(coordinator);
    coordinator
        .install_pending_dag_store(pending_store.clone())
        .await;
    assert_eq!(
        coordinator.restore_pending_dags().await,
        DEFAULT_MAX_CONCURRENT_DAG_FETCHES
    );

    let dispatched = Arc::new(SegQueue::new());
    let pump = {
        let coordinator = Arc::clone(&coordinator);
        let dispatched = Arc::clone(&dispatched);
        n0_future::task::spawn(async move {
            let config = ReplicationConfig::default();
            loop {
                match ReplicationLoop::process_next(&coordinator, &mut events, &NoMerge, &config)
                    .await
                {
                    ReplicationResult::DagFetchStarted { root_cid } => {
                        dispatched.push((root_cid, n0_future::time::Instant::now()));
                    }
                    ReplicationResult::ChannelClosed => return,
                    _ => {}
                }
            }
        })
    };
    let clock = {
        let coordinator = Arc::clone(&coordinator);
        n0_future::task::spawn(async move {
            coordinator
                .run_pending_dag_retry_clock(Duration::from_secs(2))
                .await
                .unwrap();
        })
    };

    n0_future::time::sleep(Duration::from_secs(10)).await;
    let mut order: Vec<(Cid, n0_future::time::Instant)> =
        std::iter::from_fn(|| dispatched.pop()).collect();
    let slots_in_use = coordinator.pending_dag_work_count();
    let permits_in_use = DEFAULT_MAX_CONCURRENT_DAG_FETCHES
        - coordinator.dag_fetch_limiter().global.available_permits();

    coordinator
        .manager()
        .process_pushlog(
            &PushLogBroadcast::new(
                "fresh".to_string(),
                bytes::Bytes::from(fresh.to_bytes()),
                "collection".to_string(),
                "creator".to_string(),
                bytes::Bytes::from(fresh_data),
            ),
            Some("spoke-e"),
            false,
            None,
        )
        .await
        .expect("the fresh head is acknowledged");
    let acked_at = n0_future::time::Instant::now();

    let deadline = acked_at + Duration::from_secs(30 * 60);
    let spoke_turns = |order: &[(Cid, n0_future::time::Instant)]| {
        order
            .iter()
            .filter(|(root, _)| spoke_roots.contains(root))
            .count()
    };
    while (!order.iter().any(|(root, _)| *root == fresh)
        || spoke_turns(&order) <= DEFAULT_MAX_CONCURRENT_DAG_FETCHES)
        && n0_future::time::Instant::now() < deadline
    {
        n0_future::time::sleep(Duration::from_millis(100)).await;
        order.extend(std::iter::from_fn(|| dispatched.pop()));
    }
    let waited = order
        .iter()
        .find(|(root, _)| *root == fresh)
        .map(|(_, at)| at.duration_since(acked_at));
    let spoke_order: Vec<Cid> = order
        .iter()
        .map(|(root, _)| *root)
        .filter(|root| spoke_roots.contains(root))
        .collect();

    coordinator.shutdown().await;
    clock.await.unwrap();
    pump.abort();

    assert_eq!(
        slots_in_use,
        permits_in_use,
        "{} of {slots_in_use} fetch slots are held by fetches waiting for a permit of spoke-d",
        slots_in_use - permits_in_use
    );
    let waited = waited.expect("the other peer's root is dispatched");
    assert!(
        waited <= Duration::from_secs(4),
        "the other peer's root was dispatched {waited:?} after its ack, behind spoke-d's fetches"
    );
    let mut first_turns = spoke_order[..DEFAULT_MAX_CONCURRENT_DAG_FETCHES].to_vec();
    first_turns.sort_unstable_by_key(Cid::to_bytes);
    spoke_roots.sort_unstable_by_key(Cid::to_bytes);
    assert_eq!(
        first_turns, spoke_roots,
        "every spoke-d root is dispatched once before any is dispatched again"
    );
}
