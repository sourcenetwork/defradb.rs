use super::*;
use crate::sync::coordinator::SyncCoordinator;
use crate::sync::manager::pending::PENDING_MERGE_CONTINUATION_DELAY;
use crate::sync::pending_store::{PendingDagStorage, PendingDagStore, PersistedPendingDag};
use defra_core::{Block, CompositeDeltaPayload, CrdtDelta, DAGLink, LwwDeltaPayload};

type TestCoordinator = SyncCoordinator<DefraBlockstore<RegolithStore>, NoopTransport>;

async fn restart(
    blockstore: Arc<DefraBlockstore<RegolithStore>>,
    pending: Arc<PendingDagStore<RegolithStore>>,
) -> (TestCoordinator, mpsc::Receiver<SyncEvent>) {
    let (coordinator, events) = SyncCoordinator::with_access_control(
        NoopTransport::new(),
        blockstore,
        crate::sync::SyncConfig {
            max_concurrent_dag_fetches: 1,
            ..Default::default()
        },
        AccessMode::Open,
        Arc::new(crate::ReplicatorRegistry::new()),
        Arc::new(crate::sync::collection_store::NoOpCollectionStorage),
        Arc::new(EqOnlyFilterMatcher),
    )
    .await
    .unwrap();
    coordinator.install_pending_dag_store(pending).await;
    (coordinator, events)
}

async fn seed(
    blockstore: &DefraBlockstore<RegolithStore>,
    pending: &PendingDagStore<RegolithStore>,
    doc: &str,
) -> Cid {
    let child = Block::new(
        CrdtDelta::Lww(LwwDeltaPayload {
            field_name: "name".to_string(),
            priority: 1,
            schema_version_id: "schema1".to_string(),
            data: doc.as_bytes().to_vec(),
        }),
        vec![],
        vec![],
    );
    let child_cid = child.generate_cid().unwrap();
    blockstore
        .put(&child_cid, &child.to_dag_cbor().unwrap())
        .await
        .unwrap();
    let root = Block::new(
        CrdtDelta::Composite(CompositeDeltaPayload {
            schema_version_id: "schema1".to_string(),
            priority: 1,
            status: 1,
        }),
        vec![],
        vec![DAGLink::new("name", child_cid)],
    );
    let cid = root.generate_cid().unwrap();
    blockstore
        .put(&cid, &root.to_dag_cbor().unwrap())
        .await
        .unwrap();
    pending
        .put(
            &cid,
            &PersistedPendingDag {
                doc_id: doc.to_string(),
                collection_id: "col1".to_string(),
                head_priority: None,
                creator: "creator".to_string(),
                source_peer: Some("disconnected-source".to_string()),
                alternate_providers: Vec::new(),
                is_explicit_replicator: true,
                explicit_replay_authorization: None,
            },
        )
        .await
        .unwrap();
    cid
}

fn stores() -> (
    Arc<DefraBlockstore<RegolithStore>>,
    Arc<PendingDagStore<RegolithStore>>,
) {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    (
        Arc::new(DefraBlockstore::new(store.clone(), true)),
        Arc::new(PendingDagStore::new(store)),
    )
}

fn dispatch(coordinator: &TestCoordinator) -> usize {
    coordinator.dispatch_due_pending_dag_fetches_for_test(n0_future::time::Instant::now())
}

// Observe the scheduler's actual event, then return it to the production batch
// path. DagNeedsFetch here would redo discovery even if no network call occurs.
async fn assert_local_turn(
    coordinator: &TestCoordinator,
    events: &mut mpsc::Receiver<SyncEvent>,
    root: Cid,
) {
    let event = events.try_recv().expect("scheduled continuation");
    assert!(matches!(&event, SyncEvent::DagReady { root_cid, .. } if *root_cid == root));
    coordinator
        .manager()
        .event_sender()
        .send(event)
        .await
        .unwrap();
}

#[tokio::test(start_paused = true)]
async fn thousands_of_yielded_turns_remain_pending_without_refetch_or_backoff() {
    const TURNS: usize = 2048;
    let (blockstore, pending) = stores();
    let root = seed(&blockstore, &pending, "long-history").await;
    let other = seed(&blockstore, &pending, "independent").await;
    let (coordinator, mut events) = restart(blockstore.clone(), pending.clone()).await;
    assert!(coordinator
        .transport()
        .connected_peers()
        .await
        .unwrap()
        .is_empty());
    assert_eq!(coordinator.restore_pending_dags().await, 2);
    let handler = RetryThenMergeHandler::yielding(root, TURNS);
    let config = ReplicationConfig {
        batch_size: 2,
        ..Default::default()
    };

    for turn in 0..=TURNS {
        if turn != 0 {
            assert_eq!(dispatch(&coordinator), 0, "no immediate spinning");
            tokio::time::advance(PENDING_MERGE_CONTINUATION_DELAY).await;
            assert_eq!(
                dispatch(&coordinator),
                1,
                "no exponential backoff at turn {turn}"
            );
            assert_local_turn(&coordinator, &mut events, root).await;
        }
        let results =
            ReplicationLoop::process_next_batch(&coordinator, &mut events, &handler, &config).await;
        if turn == 0 {
            assert!(results
                .iter()
                .any(|r| matches!(r, ReplicationResult::Merged { cid, .. } if *cid == other)));
            assert!(blockstore.is_merged(&other).await.unwrap());
        }
        assert_eq!(coordinator.pending_dag_work_count(), 0);
        if turn < TURNS {
            assert!(results.iter().any(|r| matches!(r, ReplicationResult::Skipped { cid, terminal: false, .. } if *cid == root)));
            assert!(!blockstore.is_merged(&root).await.unwrap());
            assert_eq!(pending.load_all().await.unwrap().len(), 1);
            assert!(pending.load_quarantined().await.unwrap().is_empty());
            let dag = coordinator.manager().pending_dag_snapshot(&root).unwrap();
            assert!(dag.merge_continuation);
            assert_eq!(dag.dispatches, 0, "merge work is not a fetch attempt");
            assert_eq!(dag.fetch_failures, 0);
        } else {
            assert!(
                matches!(results.as_slice(), [ReplicationResult::Merged { cid, .. }] if *cid == root)
            );
        }
    }
    assert!(blockstore.is_merged(&root).await.unwrap());
    assert!(pending.load_all().await.unwrap().is_empty());
    assert_eq!(coordinator.pending_dag_count(), 0);
    let diagnostics = coordinator.manager().diagnostics().snapshot();
    assert_eq!(diagnostics.pending_dag_fetch_exhausted, 0);
    assert_eq!(diagnostics.pending_dag_terminal_quarantined, 0);
    assert_eq!(
        coordinator
            .transport()
            .sync_blocks_calls
            .load(Ordering::SeqCst),
        0
    );
    tokio::time::advance(Duration::from_secs(120)).await;
    assert_eq!(dispatch(&coordinator), 0);
    assert!(events.try_recv().is_err());
    assert_eq!(handler.call_count.load(Ordering::SeqCst), TURNS + 2);
    coordinator.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn continuation_reservations_bound_queued_work_and_allow_independent_root() {
    let (blockstore, pending) = stores();
    let root = seed(&blockstore, &pending, "long-history").await;
    let other = seed(&blockstore, &pending, "independent").await;
    let (coordinator, mut events) = restart(blockstore, pending).await;
    assert_eq!(coordinator.restore_pending_dags().await, 2);
    // Both initial validations completed; model their committed yields.
    events.try_recv().unwrap();
    events.try_recv().unwrap();
    coordinator
        .manager()
        .schedule_pending_merge_continuation(&root);
    tokio::time::advance(Duration::from_millis(1)).await;
    coordinator
        .manager()
        .schedule_pending_merge_continuation(&other);
    tokio::time::advance(PENDING_MERGE_CONTINUATION_DELAY).await;
    assert_eq!(dispatch(&coordinator), 1);
    assert_local_turn(&coordinator, &mut events, root).await;
    assert_eq!(coordinator.pending_dag_work_count(), 1);
    assert_eq!(dispatch(&coordinator), 0, "queued event owns the only slot");

    // Cancellation releases ownership; nested handlers must not release the
    // outer owner's reservation early.
    let owner = coordinator.pending_dag_merge_guard(root).unwrap();
    assert!(coordinator.pending_dag_merge_guard(root).is_none());
    assert_eq!(coordinator.pending_dag_work_count(), 1);
    assert_eq!(dispatch(&coordinator), 0);
    drop(owner);
    assert_eq!(coordinator.pending_dag_work_count(), 0);
    // Consume the event as the individual (duplicate-CID fallback) path does.
    let handler = RetryThenMergeHandler::yielding(root, 2);
    let event = events.try_recv().unwrap();
    let result = super::super::handlers::process_event_serialized(
        &coordinator,
        event,
        &handler,
        &ReplicationConfig::default(),
    )
    .await;
    assert!(matches!(
        result,
        ReplicationResult::Skipped {
            terminal: false,
            ..
        }
    ));
    assert_eq!(
        dispatch(&coordinator),
        1,
        "older independent root gets next slot"
    );
    assert_local_turn(&coordinator, &mut events, other).await;
    let results = ReplicationLoop::process_next_batch(
        &coordinator,
        &mut events,
        &handler,
        &ReplicationConfig::default(),
    )
    .await;
    assert!(matches!(results.as_slice(), [ReplicationResult::Merged { cid, .. }] if *cid == other));
    assert_eq!(coordinator.pending_dag_work_count(), 0);
    coordinator.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn yielded_root_survives_shutdown_and_revalidates_on_restore() {
    let (blockstore, pending) = stores();
    let root = seed(&blockstore, &pending, "long-history").await;
    let (coordinator, mut events) = restart(blockstore.clone(), pending.clone()).await;
    coordinator.restore_pending_dags().await;
    let handler = RetryThenMergeHandler::yielding(root, 2);
    ReplicationLoop::process_next_batch(
        &coordinator,
        &mut events,
        &handler,
        &ReplicationConfig::default(),
    )
    .await;
    tokio::time::advance(PENDING_MERGE_CONTINUATION_DELAY).await;
    assert_eq!(dispatch(&coordinator), 1);
    coordinator.shutdown().await;
    assert_eq!(dispatch(&coordinator), 0);
    assert_eq!(coordinator.pending_dag_work_count(), 0);
    assert_eq!(pending.load_all().await.unwrap().len(), 1);
    assert!(!blockstore.is_merged(&root).await.unwrap());
    drop(events);

    let (coordinator, mut events) = restart(blockstore.clone(), pending.clone()).await;
    assert_eq!(coordinator.restore_pending_dags().await, 1);
    assert!(
        !coordinator
            .manager()
            .pending_dag_snapshot(&root)
            .unwrap()
            .merge_continuation
    );
    assert_local_turn(&coordinator, &mut events, root).await;
    ReplicationLoop::process_next_batch(
        &coordinator,
        &mut events,
        &handler,
        &ReplicationConfig::default(),
    )
    .await;
    tokio::time::advance(PENDING_MERGE_CONTINUATION_DELAY).await;
    assert_eq!(dispatch(&coordinator), 1);
    assert_local_turn(&coordinator, &mut events, root).await;
    ReplicationLoop::process_next_batch(
        &coordinator,
        &mut events,
        &handler,
        &ReplicationConfig::default(),
    )
    .await;
    assert!(blockstore.is_merged(&root).await.unwrap());
    assert!(pending.load_all().await.unwrap().is_empty());
    coordinator.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn acp_skip_does_not_opt_into_local_continuation() {
    let (blockstore, pending) = stores();
    let root = seed(&blockstore, &pending, "acp").await;
    let (coordinator, mut events) = restart(blockstore.clone(), pending.clone()).await;
    coordinator.restore_pending_dags().await;
    let results = ReplicationLoop::process_next_batch(
        &coordinator,
        &mut events,
        &RetryThenMergeHandler::new(),
        &ReplicationConfig::default(),
    )
    .await;
    assert!(matches!(
        results.as_slice(),
        [ReplicationResult::Skipped {
            terminal: false,
            ..
        }]
    ));
    assert!(
        !coordinator
            .manager()
            .pending_dag_snapshot(&root)
            .unwrap()
            .merge_continuation
    );
    assert!(!blockstore.is_merged(&root).await.unwrap());
    assert_eq!(pending.load_all().await.unwrap().len(), 1);
    assert_eq!(dispatch(&coordinator), 1);
    assert!(
        matches!(events.try_recv().unwrap(), SyncEvent::DagNeedsFetch { root_cid, .. } if root_cid == root)
    );
    coordinator.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn duplicate_continuation_batch_completes_only_after_final_turn() {
    let (blockstore, pending) = stores();
    let root = seed(&blockstore, &pending, "long-history").await;
    let (coordinator, mut events) = restart(blockstore.clone(), pending.clone()).await;
    coordinator.restore_pending_dags().await;
    let handler = RetryThenMergeHandler::yielding(root, 2);
    let config = ReplicationConfig {
        batch_size: 2,
        ..Default::default()
    };
    ReplicationLoop::process_next_batch(&coordinator, &mut events, &handler, &config).await;
    assert!(!blockstore.is_merged(&root).await.unwrap());
    tokio::time::advance(PENDING_MERGE_CONTINUATION_DELAY).await;
    assert_eq!(dispatch(&coordinator), 1);
    let event = events.try_recv().unwrap();
    assert!(matches!(&event, SyncEvent::DagReady { root_cid, .. } if *root_cid == root));
    // A second arrival for the same root switches the production batch
    // runner to its serialized per-CID path.
    coordinator
        .manager()
        .event_sender()
        .send(dag_ready_event(root))
        .await
        .unwrap();
    coordinator
        .manager()
        .event_sender()
        .send(event)
        .await
        .unwrap();
    let results =
        ReplicationLoop::process_next_batch(&coordinator, &mut events, &handler, &config).await;
    assert!(matches!(
        results.as_slice(),
        [
            ReplicationResult::Skipped {
                terminal: false,
                ..
            },
            ReplicationResult::Merged { .. }
        ]
    ));
    assert!(blockstore.is_merged(&root).await.unwrap());
    assert!(pending.load_all().await.unwrap().is_empty());
    assert_eq!(coordinator.pending_dag_work_count(), 0);
    assert_eq!(dispatch(&coordinator), 0);
    coordinator.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn rejected_continuation_releases_slot_and_quarantines_without_marking() {
    let (blockstore, pending) = stores();
    let root = seed(&blockstore, &pending, "long-history").await;
    let (coordinator, mut events) = restart(blockstore.clone(), pending.clone()).await;
    coordinator.restore_pending_dags().await;
    let handler = RetryThenMergeHandler::yielding(root, 1);
    ReplicationLoop::process_next_batch(
        &coordinator,
        &mut events,
        &handler,
        &ReplicationConfig::default(),
    )
    .await;
    tokio::time::advance(PENDING_MERGE_CONTINUATION_DELAY).await;
    assert_eq!(dispatch(&coordinator), 1);
    let results = ReplicationLoop::process_next_batch(
        &coordinator,
        &mut events,
        &RejectingMergeHandler::new("invalid content"),
        &ReplicationConfig::default(),
    )
    .await;
    assert!(matches!(
        results.as_slice(),
        [ReplicationResult::Quarantined { .. }]
    ));
    assert!(!blockstore.is_merged(&root).await.unwrap());
    assert!(pending.load_all().await.unwrap().is_empty());
    assert!(pending.is_quarantined(&root).await.unwrap());
    assert_eq!(coordinator.pending_dag_count(), 0);
    assert_eq!(coordinator.pending_dag_work_count(), 0);
    assert_eq!(dispatch(&coordinator), 0);
    coordinator.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn already_merged_duplicate_continuation_releases_reservation_without_handler() {
    let (blockstore, pending) = stores();
    let root = seed(&blockstore, &pending, "long-history").await;
    let (coordinator, mut events) = restart(blockstore.clone(), pending.clone()).await;
    coordinator.restore_pending_dags().await;
    let handler = RetryThenMergeHandler::yielding(root, 2);
    let config = ReplicationConfig::default();
    ReplicationLoop::process_next_batch(&coordinator, &mut events, &handler, &config).await;
    tokio::time::advance(PENDING_MERGE_CONTINUATION_DELAY).await;
    assert_eq!(dispatch(&coordinator), 1);
    blockstore.mark_as_merged(&root).await.unwrap();
    coordinator
        .manager()
        .event_sender()
        .send(dag_ready_event(root))
        .await
        .unwrap();
    let results =
        ReplicationLoop::process_next_batch(&coordinator, &mut events, &handler, &config).await;
    assert_eq!(results.len(), 2);
    assert!(results
        .iter()
        .all(|r| matches!(r, ReplicationResult::Skipped { terminal: true, .. })));
    assert_eq!(handler.call_count.load(Ordering::SeqCst), 1);
    assert!(pending.load_all().await.unwrap().is_empty());
    assert_eq!(coordinator.pending_dag_work_count(), 0);
    assert_eq!(dispatch(&coordinator), 0);
    coordinator.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn failed_continuation_validation_releases_batch_and_duplicate_reservations() {
    for duplicate in [false, true] {
        let (blockstore, pending) = stores();
        let root = seed(&blockstore, &pending, "long-history").await;
        let (coordinator, mut events) = restart(blockstore.clone(), pending.clone()).await;
        coordinator.restore_pending_dags().await;
        let config = ReplicationConfig::default();
        ReplicationLoop::process_next_batch(
            &coordinator,
            &mut events,
            &RetryThenMergeHandler::yielding(root, 2),
            &config,
        )
        .await;
        tokio::time::advance(PENDING_MERGE_CONTINUATION_DELAY).await;
        assert_eq!(dispatch(&coordinator), 1);
        let mut event = events.try_recv().unwrap();
        let SyncEvent::DagReady {
            explicit_replay_authorization,
            ..
        } = &mut event
        else {
            panic!("continuation must not refetch");
        };
        *explicit_replay_authorization = Some(ExplicitReplayAuthorization {
            source_peer_id: "disconnected-source".to_string(),
            target_peer_id: "local-peer".to_string(),
            collection_id: "col1".to_string(),
            authorizer_did: "creator".to_string(),
            expires_at: u64::MAX,
            capability: None,
        });
        if duplicate {
            coordinator
                .manager()
                .event_sender()
                .send(event.clone())
                .await
                .unwrap();
        }
        coordinator
            .manager()
            .event_sender()
            .send(event)
            .await
            .unwrap();
        let handler = RejectingAuthorizationHandler::new();
        let results =
            ReplicationLoop::process_next_batch(&coordinator, &mut events, &handler, &config).await;
        assert_eq!(results.len(), if duplicate { 2 } else { 1 });
        assert!(results.iter().all(|r| matches!(r, ReplicationResult::Failed { error, .. } if error.contains("authorization rejected"))));
        assert_eq!(handler.batch_calls(), 0);
        assert!(!blockstore.is_merged(&root).await.unwrap());
        assert_eq!(pending.load_all().await.unwrap().len(), 1);
        assert!(
            !coordinator
                .manager()
                .pending_dag_snapshot(&root)
                .unwrap()
                .merge_continuation
        );
        assert_eq!(coordinator.pending_dag_work_count(), 0);
        tokio::time::advance(PENDING_MERGE_CONTINUATION_DELAY).await;
        assert_eq!(
            dispatch(&coordinator),
            1,
            "failed validation must not leak a slot"
        );
        assert!(matches!(
            events.try_recv().unwrap(),
            SyncEvent::DagNeedsFetch { .. }
        ));
        coordinator.shutdown().await;
    }
}

#[tokio::test(start_paused = true)]
async fn retryable_error_after_yield_returns_to_paced_fetch_retry_without_spinning() {
    use crate::sync::manager::pending::retry_backoff;

    let (blockstore, pending) = stores();
    let root = seed(&blockstore, &pending, "long-history").await;
    let (coordinator, mut events) = restart(blockstore.clone(), pending.clone()).await;
    coordinator.restore_pending_dags().await;
    let config = ReplicationConfig::default();
    ReplicationLoop::process_next_batch(
        &coordinator,
        &mut events,
        &RetryThenMergeHandler::yielding(root, 1),
        &config,
    )
    .await;
    tokio::time::advance(PENDING_MERGE_CONTINUATION_DELAY).await;
    assert_eq!(dispatch(&coordinator), 1);
    assert_local_turn(&coordinator, &mut events, root).await;
    let failing = TestMergeHandler::new(false, false);
    let results =
        ReplicationLoop::process_next_batch(&coordinator, &mut events, &failing, &config).await;
    assert!(matches!(
        results.as_slice(),
        [ReplicationResult::Failed { .. }]
    ));

    let mut delay = PENDING_MERGE_CONTINUATION_DELAY;
    for round in 1..=3 {
        assert_eq!(coordinator.pending_dag_work_count(), 0);
        assert!(
            !coordinator
                .manager()
                .pending_dag_snapshot(&root)
                .unwrap()
                .merge_continuation
        );
        for _ in 0..16 {
            assert_eq!(dispatch(&coordinator), 0, "ordinary errors must not spin");
            assert!(events.try_recv().is_err());
        }
        tokio::time::advance(delay).await;
        assert_eq!(dispatch(&coordinator), 1);
        let event = events.try_recv().unwrap();
        assert!(matches!(&event, SyncEvent::DagNeedsFetch { root_cid, .. } if *root_cid == root));
        coordinator
            .manager()
            .event_sender()
            .send(event)
            .await
            .unwrap();
        let results =
            ReplicationLoop::process_next_batch(&coordinator, &mut events, &failing, &config).await;
        assert!(matches!(
            results.as_slice(),
            [ReplicationResult::DagFetchStarted { .. }]
        ));
        // The ordinary fetch path validates the local DAG without a provider,
        // then retries the merge. The error must not notify an immediate turn.
        let results =
            ReplicationLoop::process_next_batch(&coordinator, &mut events, &failing, &config).await;
        assert!(matches!(
            results.as_slice(),
            [ReplicationResult::Failed { .. }]
        ));
        let dag = coordinator.manager().pending_dag_snapshot(&root).unwrap();
        assert_eq!(dag.dispatches, round);
        assert_eq!(dag.fetch_failures, 0);
        delay = retry_backoff(round);
    }
    assert_eq!(coordinator.pending_dag_work_count(), 0);
    assert_eq!(dispatch(&coordinator), 0);
    assert_eq!(failing.calls(), 4);
    assert!(!blockstore.is_merged(&root).await.unwrap());
    assert_eq!(pending.load_all().await.unwrap().len(), 1);
    coordinator.shutdown().await;
}
