use super::*;
use crate::sync::pending_store::PendingDagStore;
use crate::sync::{
    BlockMetadata, MergeHandler, MergeOutcome, ReplicationConfig, ReplicationLoop,
    ReplicationResult, SyncConfig, SyncCoordinator,
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

/// A document head and its one field block, which is not stored locally.
fn head_and_field() -> (Cid, Vec<u8>, Cid, Vec<u8>) {
    let leaf = Block::new(
        CrdtDelta::Lww(LwwDeltaPayload {
            field_name: "name".to_string(),
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
        vec![DAGLink::new("name", leaf_cid)],
    );
    (
        head.generate_cid().unwrap(),
        head.to_dag_cbor().unwrap(),
        leaf_cid,
        leaf.to_dag_cbor().unwrap(),
    )
}

fn push(cid: Cid, block: &[u8]) -> PushLogBroadcast {
    PushLogBroadcast::new(
        "doc".to_string(),
        bytes::Bytes::from(cid.to_bytes()),
        "collection".to_string(),
        "creator".to_string(),
        bytes::Bytes::copy_from_slice(block),
    )
}

fn merge_work() -> SyncEvent {
    SyncEvent::SyncError {
        cid: make_cid(b"merge work"),
        error: "queued merge work".to_string(),
    }
}

/// A receiver whose merge-loop channel holds one event, with a coordinator
/// that has acknowledged one head whose field block is missing.
async fn acked_head_behind_full_channel() -> (
    SyncCoordinator<DefraBlockstore<RegolithStore>, TestTransport>,
    crate::sync::SyncEventReceiver,
    Cid,
    Cid,
    Vec<u8>,
) {
    let blockstore = Arc::new(DefraBlockstore::new(
        Arc::new(RegolithStore::in_memory().unwrap()),
        true,
    ));
    let (head, head_data, leaf, leaf_data) = head_and_field();
    let transport = TestTransport::new(
        blockstore.clone(),
        head,
        head_data.clone(),
        RapidHashMap::new(),
        RapidHashMap::new(),
    );
    let config = SyncConfig {
        event_buffer_size: 1,
        ..SyncConfig::default()
    };
    let (coordinator, events) = SyncCoordinator::new(transport, blockstore, crate::network::test_network(), config)
        .await
        .unwrap();
    coordinator
        .install_pending_dag_store(Arc::new(PendingDagStore::new(Arc::new(
            RegolithStore::in_memory().unwrap(),
        ))))
        .await;
    coordinator
        .manager()
        .process_pushlog(&push(head, &head_data), Some("remote-peer"), false, None)
        .await
        .expect("the head is acknowledged");
    coordinator
        .manager()
        .event_sender()
        .send(merge_work())
        .await
        .unwrap();
    (coordinator, events, head, leaf, leaf_data)
}

/// The merge loop's channel stays full while senders block on it, as in a
/// catch-up storm: each freed cell goes to the oldest blocked sender. The
/// receiver clock must still dispatch a due root, and the merge loop must
/// start that fetch before the merge work queued ahead of it.
#[nuclei::test(start_paused = true)]
async fn clock_dispatch_not_starved_by_full_sync_event_channel() {
    let (coordinator, mut events, head, _, _) = acked_head_behind_full_channel().await;
    let sender = coordinator.manager().event_sender();
    let blocked = {
        let sender = sender.clone();
        crate::rt::spawn(async move { sender.send(merge_work()).await })
    };
    crate::rt::time::sleep(Duration::from_millis(1)).await;
    assert!(matches!(
        events.recv().await,
        Some(SyncEvent::SyncError { .. })
    ));
    blocked.join().await.unwrap().unwrap();
    let blocked = crate::rt::spawn(async move { sender.send(merge_work()).await });
    crate::rt::time::sleep(Duration::from_millis(1)).await;

    let dispatched =
        coordinator.dispatch_due_pending_dag_fetches_for_test(crate::rt::time::Instant::now());
    let config = ReplicationConfig::default();
    let first = if dispatched == 1 {
        Some(ReplicationLoop::process_next(&coordinator, &mut events, &NoMerge, &config).await)
    } else {
        None
    };

    coordinator.shutdown().await;
    blocked.abort();

    assert_eq!(
        dispatched, 1,
        "a due root waits while blocked senders keep the merge loop's channel full"
    );
    assert!(
        matches!(first, Some(ReplicationResult::DagFetchStarted { root_cid }) if root_cid == head),
        "the merge loop processed {first:?} before starting the dispatched fetch"
    );
}

/// A PushLog handler that completes another root's DAG must not wait for
/// room in the merge loop's channel: it holds an admission lane slot and the
/// CID guard. The completed root goes to the receiver clock instead.
#[nuclei::test(start_paused = true)]
async fn pushlog_reply_not_blocked_by_full_event_channel() {
    let (coordinator, mut events, head, leaf, leaf_data) = acked_head_behind_full_channel().await;
    let now = crate::rt::time::Instant::now();
    assert!(coordinator
        .manager()
        .try_claim_pending_dag_dispatch(&head, now));

    let reply = crate::rt::time::timeout(
        Duration::from_secs(5),
        coordinator.manager().process_pushlog(
            &push(leaf, &leaf_data),
            Some("remote-peer"),
            false,
            None,
        ),
    )
    .await;
    let redispatched = reply.is_ok().then(|| {
        coordinator.dispatch_due_pending_dag_fetches_for_test(crate::rt::time::Instant::now())
    });
    let config = ReplicationConfig::default();
    let first = if redispatched == Some(1) {
        Some(ReplicationLoop::process_next(&coordinator, &mut events, &NoMerge, &config).await)
    } else {
        None
    };

    coordinator.shutdown().await;

    assert!(
        matches!(reply, Ok(Ok(()))),
        "the PushLog reply waited on the merge loop's full channel: {reply:?}"
    );
    assert_eq!(
        redispatched,
        Some(1),
        "the completed root is due for the receiver clock at once"
    );
    assert!(
        matches!(first, Some(ReplicationResult::DagFetchStarted { root_cid }) if root_cid == head),
        "the merge loop processed {first:?} before the completed root"
    );
}

/// A fetch that completed its DAG waits for room in the merge loop's channel
/// to report it. It needs no network while it waits, so it holds no fetch
/// permit: the permits bound transfers, and pull fetches share them.
#[nuclei::test(start_paused = true)]
async fn no_permit_held_in_emit_dag_ready() {
    let blockstore = Arc::new(DefraBlockstore::new(
        Arc::new(RegolithStore::in_memory().unwrap()),
        true,
    ));
    let child_data = encode_ipld(ipld!({ "value": 1 }));
    let child_cid = make_cid(&child_data);
    let root_data = encode_ipld(ipld!({ "child": child_cid }));
    let root_cid = make_cid(&root_data);
    blockstore.put(&root_cid, &root_data).await.unwrap();
    let transport = TestTransport::new(
        blockstore.clone(),
        root_cid,
        root_data,
        RapidHashMap::new(),
        RapidHashMap::from_iter([(child_cid, child_data)]),
    );

    let (event_tx, mut event_rx) = mpsc::channel(1);
    event_tx.send(merge_work()).await.unwrap();
    let limiter = DagFetchLimiter::new(2);
    let fetch = crate::rt::spawn(poll_fetch_dag(
        transport,
        blockstore.clone(),
        event_tx,
        root_cid,
        DagFetchContext::new(
            "doc-id".to_string(),
            "collection-id".to_string(),
            "creator-id".to_string(),
            PeerId::new("remote-peer".to_string()),
        ),
        limiter.clone(),
        diagnostics(),
    ));
    while !matches!(blockstore.has(&child_cid).await, Ok(true)) {
        crate::rt::time::sleep(Duration::from_millis(10)).await;
    }
    crate::rt::time::sleep(Duration::from_secs(1)).await;
    let waiting = !fetch.is_finished();
    let free_permits = limiter.global.available_permits();

    assert!(matches!(
        event_rx.recv().await,
        Some(SyncEvent::SyncError { .. })
    ));
    assert!(matches!(
        event_rx.recv().await,
        Some(SyncEvent::DagReady { root_cid: ready, .. }) if ready == root_cid
    ));
    fetch.join().await.unwrap();

    assert!(
        waiting,
        "the fetch reports its DAG once the channel has room"
    );
    assert_eq!(
        free_permits, 2,
        "a fetch waiting to report a complete DAG still holds a fetch permit"
    );
}
