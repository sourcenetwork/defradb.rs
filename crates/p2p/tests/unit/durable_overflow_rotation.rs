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
#[tokio::test(start_paused = true)]
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
    let (coordinator, mut events) = SyncCoordinator::new(transport, blockstore.clone(), config)
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
        n0_future::task::spawn(async move {
            let config = ReplicationConfig::default();
            while !matches!(
                ReplicationLoop::process_next(&coordinator, &mut events, &MergeAll, &config).await,
                ReplicationResult::ChannelClosed
            ) {}
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
    let resync = {
        let coordinator = Arc::clone(&coordinator);
        n0_future::task::spawn(async move {
            coordinator
                .run_pending_dag_resync(Duration::from_secs(60))
                .await;
        })
    };

    let deadline = n0_future::time::Instant::now() + Duration::from_secs(30 * 60);
    let mut merged = 0;
    while n0_future::time::Instant::now() < deadline {
        n0_future::time::sleep(Duration::from_secs(1)).await;
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
    clock.await.unwrap();
    resync.await.unwrap();
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
