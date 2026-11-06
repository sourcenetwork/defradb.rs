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

/// Blocks by CID, each with its encoded bytes.
type Blocks = Vec<(Cid, Vec<u8>)>;

/// A catch-up root over a long chain of history: the root is local, every
/// block below it is missing, and each one names the next.
fn long_history(label: usize, length: usize) -> (Cid, Vec<u8>, Blocks) {
    let mut chain = Vec::with_capacity(length);
    let mut below: Option<Cid> = None;
    for step in 0..length {
        let data = match below {
            None => encode_ipld(ipld!({ "root": label as u64, "step": step as u64 })),
            Some(child) => {
                encode_ipld(ipld!({ "root": label as u64, "step": step as u64, "child": child }))
            }
        };
        let cid = make_cid(&data);
        chain.push((cid, data));
        below = Some(cid);
    }
    let root_data = encode_ipld(ipld!({ "root": label as u64, "child": below.unwrap() }));
    (make_cid(&root_data), root_data, chain)
}

/// A document head, held locally, whose one field block is missing.
fn head_missing_its_field() -> (Cid, Vec<u8>, Cid, Vec<u8>) {
    let leaf = Block::new(
        CrdtDelta::Lww(LwwDeltaPayload {
            field_name: "name".to_string(),
            priority: 1,
            collection_version_id: "schema1".to_string(),
            data: b"fresh".to_vec(),
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
        head_priority: None,
        creator: "creator".to_string(),
        source_peer: Some(source_peer.to_string()),
        alternate_providers: Vec::new(),
        is_explicit_replicator: false,
        explicit_replay_authorization: None,
    }
}

/// Every fetch slot goes to a catch-up root whose provider trickles: each
/// window delivers a block, so no attempt ever stalls, and the history is far
/// longer than any wait we accept. A head acknowledged meanwhile is still
/// dispatched within one attempt quantum: a productive attempt gives its slot
/// back at the quantum, keeps what it fetched, and is due again behind the
/// roots that were waiting. Ending an attempt that way is not a failure.
#[test]
fn an_acked_root_is_dispatched_within_a_quantum_of_trickling_holders() {
    edge::testing::paused(|edge| async move {
        const DRIP: Duration = Duration::from_secs(10);
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let blockstore = Arc::new(DefraBlockstore::new(store, true));
        let pending_store = Arc::new(PendingDagStore::new(Arc::new(
            RegolithStore::in_memory().unwrap(),
        )));

        let mut served = RapidHashMap::new();
        let mut catch_up = Vec::new();
        let mut histories = Vec::new();
        let mut connected = Vec::new();
        for slot in 0..DEFAULT_MAX_CONCURRENT_DAG_FETCHES {
            let (root, root_data, chain) = long_history(slot, 1_000);
            blockstore.put(&root, &root_data).await.unwrap();
            let provider = format!("trickle-{slot}");
            pending_store
                .put(&root, &record(&format!("catch-up-{slot}"), &provider))
                .await
                .unwrap();
            connected.push(PeerId::new(provider));
            histories.push(chain.iter().map(|(cid, _)| *cid).collect::<Vec<_>>());
            served.extend(chain);
            catch_up.push(root);
        }
        let (fresh, fresh_data, leaf, leaf_data) = head_missing_its_field();
        served.insert(leaf, leaf_data);
        connected.push(PeerId::new("fresh-peer".to_string()));

        let transport = TestTransport::new(
            blockstore.clone(),
            fresh,
            fresh_data.clone(),
            RapidHashMap::new(),
            served,
        );
        transport.set_force_rooted_sync();
        transport.set_drip_delay(DRIP);
        transport.set_connected_peers(connected);

        let (coordinator, mut events) = SyncCoordinator::new(
            transport,
            blockstore.clone(),
            crate::network::Network::on(edge.clone()),
            SyncConfig::default(),
        )
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
            crate::rt::spawn(async move {
                let config = ReplicationConfig::default();
                loop {
                    match ReplicationLoop::process_next(
                        &coordinator,
                        &mut events,
                        &NoMerge,
                        &config,
                    )
                    .await
                    {
                        ReplicationResult::DagFetchStarted { root_cid } => {
                            dispatched.push((root_cid, crate::rt::time::Instant::now()));
                        }
                        ReplicationResult::ChannelClosed => return,
                        _ => {}
                    }
                }
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

        crate::rt::time::sleep(DRIP).await;
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
                Some("fresh-peer"),
                false,
                None,
            )
            .await
            .expect("the fresh head is acknowledged");
        let acked_at = crate::rt::time::Instant::now();

        let mut order = Vec::new();
        let deadline = acked_at + Duration::from_secs(30 * 60);
        while !order.iter().any(|(root, _)| *root == fresh)
            && crate::rt::time::Instant::now() < deadline
        {
            crate::rt::time::sleep(Duration::from_millis(100)).await;
            order.extend(std::iter::from_fn(|| dispatched.pop()));
        }
        let waited = order
            .iter()
            .find(|(root, _)| *root == fresh)
            .map(|(_, at)| at.duration_since(acked_at));

        // The holders keep their progress and come back for more.
        crate::rt::time::sleep(CLOCK_ATTEMPT_QUANTUM).await;
        order.extend(std::iter::from_fn(|| dispatched.pop()));
        let mut fetched = 0;
        for history in &histories {
            for cid in history {
                if matches!(blockstore.has(cid).await, Ok(true)) {
                    fetched += 1;
                }
            }
        }
        let exhausted = coordinator
            .manager()
            .diagnostics()
            .snapshot()
            .pending_dag_fetch_exhausted;

        coordinator.shutdown().await;
        clock.await;
        pump.abort();

        let waited = waited.expect("the acked root is dispatched within 30 minutes");
        assert!(
            waited <= CLOCK_ATTEMPT_QUANTUM + 2 * BLOCK_SYNC_COMPLETION_WATCHDOG,
            "the acked root waited {waited:?} for a slot"
        );
        let redispatched = order
            .iter()
            .filter(|(root, _)| catch_up.contains(root))
            .count();
        assert!(
            redispatched > DEFAULT_MAX_CONCURRENT_DAG_FETCHES,
            "the trickling roots are due again after giving their slots back ({redispatched} dispatches)"
        );
        assert!(
            fetched
                > DEFAULT_MAX_CONCURRENT_DAG_FETCHES
                    * (CLOCK_ATTEMPT_QUANTUM.as_secs() / DRIP.as_secs()) as usize
                    / 2,
            "blocks fetched before a quantum ended are kept ({fetched})"
        );
        assert_eq!(exhausted, 0, "a quantum is not a failure");
    })
}
