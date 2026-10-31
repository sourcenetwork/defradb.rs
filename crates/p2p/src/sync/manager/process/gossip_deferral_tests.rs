use std::sync::Arc;

use blockstore::DefraBlockstore;
use defra_core::{Block, CompositeDeltaPayload, CrdtDelta, DAGLink, LwwDeltaPayload};
use storage::RegolithStore;

use super::*;
use crate::message::PushLogRequest;
use crate::sync::manager::config::DEFAULT_GOSSIP_DEFERRAL_PEER_BYTES;
use crate::sync::pending_store::PendingDagStore;
use crate::sync::{PeerStateTracker, SyncConfig};

/// A libp2p peer ID, as an origin is named.
const PEER: &str = "12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN";
const OTHER_PEER: &str = "12D3KooWRBhwfeP2Y4TCx1SM6s9rUoHhR5STiGwxBhgFRcw3UERE";
/// A Shinzo Log collection ID and an Ed25519 creator DID, at their real lengths.
const COLLECTION: &str = "bafyreibjraiyakk3irmdr7nl6ijzamzemaebryyxsbfpdexz3dhmglxtae";
const CREATOR: &str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";

type TestManager = SyncManager<DefraBlockstore<RegolithStore>>;

async fn manager_with(config: SyncConfig) -> (TestManager, tokio::sync::mpsc::Receiver<SyncEvent>) {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store.clone(), true));
    let (manager, events) = SyncManager::new(blockstore, Arc::new(PeerStateTracker::new()), config);
    manager
        .install_pending_dag_store(Arc::new(PendingDagStore::new(store)))
        .await;
    (manager, events)
}

fn with_pending_dags(max_pending_dags: usize) -> SyncConfig {
    SyncConfig {
        max_pending_dags,
        ..SyncConfig::default()
    }
}

/// A write-once document head whose one field block is not local.
fn head_with_missing_link(doc: usize) -> (Cid, PushLogBroadcast) {
    let field = Block::new(
        CrdtDelta::Lww(LwwDeltaPayload {
            field_name: "name".to_string(),
            priority: 1,
            collection_version_id: "schema1".to_string(),
            data: format!("doc-{doc}").into_bytes(),
        }),
        vec![],
        vec![],
    );
    let head = Block::new(
        CrdtDelta::Composite(CompositeDeltaPayload {
            collection_version_id: "schema1".to_string(),
            priority: 1,
            status: 1,
        }),
        vec![],
        vec![DAGLink::new("name", field.generate_cid().unwrap())],
    );
    let cid = head.generate_cid().unwrap();
    let request = PushLogRequest::new(
        format!("bae-{doc:08x}-0000-5000-8000-000000000000"),
        bytes::Bytes::from(cid.to_bytes()),
        COLLECTION.to_string(),
        CREATOR.to_string(),
        bytes::Bytes::from(head.to_dag_cbor().unwrap()),
    );
    (cid, PushLogBroadcast::from_request(&request))
}

/// What one deferred head of a peer takes, as the budgets count it.
fn footprint_of(doc: usize, origin: &str) -> u64 {
    let (cid, msg) = head_with_missing_link(doc);
    let record = PersistedPendingDag {
        doc_id: msg.doc_id.clone(),
        collection_id: msg.collection_id.clone(),
        head_priority: Some(1),
        creator: msg.creator.clone(),
        source_peer: Some(origin.to_owned()),
        alternate_providers: Vec::new(),
        is_explicit_replicator: false,
        explicit_replay_authorization: None,
    };
    record_footprint(&cid, &record)
}

#[test]
fn the_default_peer_budget_holds_a_million_shinzo_log_heads() {
    let bytes = footprint_of(0, PEER);
    println!("deferred gossip head footprint: {bytes} bytes");
    assert!(1_000_000 * bytes <= DEFAULT_GOSSIP_DEFERRAL_PEER_BYTES);
}

#[test]
fn refused_gossip_heads_are_kept_durably_and_promoted_when_a_slot_frees() {
    workers::block_on(async {
        // 4 / PENDING_DAG_PEER_CAPACITY_DIVISOR: one slot for the peer.
        let (manager, _events) = manager_with(with_pending_dags(4)).await;
        let heads: Vec<_> = (0..6).map(head_with_missing_link).collect();

        for (_, msg) in &heads {
            match manager.process_pushlog(msg, Some(PEER), false, None).await {
                Ok(()) => {}
                Err(Error::PendingDagCapacity { .. }) => {
                    manager.defer_gossip_head(msg, PEER).await.unwrap();
                }
                Err(error) => panic!("unexpected refusal: {error}"),
            }
        }
        assert_eq!(manager.pending_dag_count(), 1);
        assert_eq!(manager.persisted_pending_count(), heads.len());
        let snapshot = manager.diagnostics().snapshot();
        assert_eq!(snapshot.gossip_heads_deferred as usize, heads.len() - 1);
        assert_eq!(snapshot.gossip_heads_shed, 0);
        assert_eq!(manager.resync_persisted_pending_dags().await, 0);

        manager.discharge_merged(&heads[0].0).await;
        manager.durable_only_ready().await;
        assert_eq!(manager.resync_persisted_pending_dags().await, 1);
        assert_eq!(manager.pending_dag_count(), 1);
        assert_eq!(manager.persisted_pending_count(), heads.len() - 1);
    })
    .unwrap();
}

#[test]
fn deferrals_are_not_held_to_the_pushed_registration_count_cap() {
    workers::block_on(async {
        let (manager, _events) = manager_with(with_pending_dags(1)).await;
        let past_count_cap = manager.persisted_pending_capacity() + 3;
        for doc in 0..past_count_cap {
            let (_, msg) = head_with_missing_link(doc);
            manager.defer_gossip_head(&msg, PEER).await.unwrap();
        }
        assert_eq!(manager.persisted_pending_count(), past_count_cap);
        assert_eq!(manager.diagnostics().snapshot().gossip_heads_shed, 0);
    })
    .unwrap();
}

#[test]
fn a_head_past_the_peer_budget_is_dropped_and_reported() {
    workers::block_on(async {
        let budget = 3 * footprint_of(0, PEER);
        let (manager, mut events) = manager_with(SyncConfig {
            gossip_deferral_peer_bytes: budget,
            ..SyncConfig::default()
        })
        .await;
        for doc in 0..3 {
            let (_, msg) = head_with_missing_link(doc);
            manager.defer_gossip_head(&msg, PEER).await.unwrap();
        }
        let (dropped, overflow) = head_with_missing_link(3);
        assert!(matches!(
            manager.defer_gossip_head(&overflow, PEER).await,
            Err(Error::GossipHeadDropped { .. })
        ));
        assert_eq!(manager.persisted_pending_count(), 3);
        assert!(manager.persisted.source_bytes(PEER) <= budget);
        assert_eq!(manager.diagnostics().snapshot().gossip_heads_shed, 1);
        match events.try_recv() {
            Ok(SyncEvent::GossipHeadDropped {
                root_cid, peer_id, ..
            }) => {
                assert_eq!(root_cid, dropped);
                assert_eq!(peer_id, PEER);
            }
            other => panic!("expected a dropped-head report, got {other:?}"),
        }

        // Another peer still has its own budget.
        let (_, other) = head_with_missing_link(4);
        manager.defer_gossip_head(&other, OTHER_PEER).await.unwrap();
        assert_eq!(manager.persisted_pending_count(), 4);
    })
    .unwrap();
}

#[test]
fn the_total_budget_bounds_every_peer_together() {
    workers::block_on(async {
        let one = footprint_of(0, PEER);
        let (manager, mut events) = manager_with(SyncConfig {
            gossip_deferral_peer_bytes: 10 * one,
            gossip_deferral_total_bytes: 3 * one,
            ..SyncConfig::default()
        })
        .await;
        for (doc, origin) in [(0, PEER), (1, PEER), (2, OTHER_PEER)] {
            let (_, msg) = head_with_missing_link(doc);
            manager.defer_gossip_head(&msg, origin).await.unwrap();
        }
        let (dropped, overflow) = head_with_missing_link(3);
        assert!(matches!(
            manager.defer_gossip_head(&overflow, OTHER_PEER).await,
            Err(Error::GossipHeadDropped { .. })
        ));
        assert!(matches!(
            events.try_recv(),
            Ok(SyncEvent::GossipHeadDropped { root_cid, .. }) if root_cid == dropped
        ));
    })
    .unwrap();
}

#[test]
fn a_merged_record_returns_its_bytes_to_the_peer_budget() {
    workers::block_on(async {
        let (manager, _events) = manager_with(SyncConfig::default()).await;
        let (cid, msg) = head_with_missing_link(0);
        manager.defer_gossip_head(&msg, PEER).await.unwrap();
        assert_eq!(manager.persisted.source_bytes(PEER), footprint_of(0, PEER));
        manager.discharge_merged(&cid).await;
        assert_eq!(manager.persisted.source_bytes(PEER), 0);
        assert_eq!(manager.persisted.total_bytes(), 0);
    })
    .unwrap();
}

#[test]
fn a_repeated_gossip_head_writes_one_record() {
    workers::block_on(async {
        let (manager, _events) = manager_with(with_pending_dags(4)).await;
        let (_, msg) = head_with_missing_link(0);
        manager.defer_gossip_head(&msg, PEER).await.unwrap();
        manager.defer_gossip_head(&msg, PEER).await.unwrap();
        assert_eq!(manager.persisted_pending_count(), 1);
        assert_eq!(manager.diagnostics().snapshot().gossip_heads_deferred, 1);
    })
    .unwrap();
}
