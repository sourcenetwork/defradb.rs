use super::*;
use crate::sync::coordinator::SyncCoordinator;
use crate::sync::manager::pending::PENDING_MERGE_CONTINUATION_DELAY;
use crate::sync::pending_store::{
    PendingDagStorage, PendingDagStore, PersistedPendingDag, PersistedView,
};
use defra_core::{Block, CompositeDeltaPayload, CrdtDelta, DAGLink, LwwDeltaPayload};

type TestCoordinator = SyncCoordinator<DefraBlockstore<RegolithStore>, NoopTransport>;

async fn restart(
    blockstore: Arc<DefraBlockstore<RegolithStore>>,
    pending: Arc<PendingDagStore<RegolithStore>>,
) -> (TestCoordinator, crate::sync::SyncEventReceiver) {
    let (coordinator, events) = SyncCoordinator::with_access_control(
        NoopTransport::new(),
        blockstore,
        crate::network::test_network(),
        crate::sync::SyncConfig {
            max_concurrent_dag_fetches: 1,
            ..Default::default()
        },
        AccessMode::Open,
        Arc::new(crate::ReplicatorRegistry::new()),
        Arc::new(crate::sync::collection_store::MemoryCollectionStorage::default()),
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
            collection_version_id: "schema1".to_string(),
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
            collection_version_id: "schema1".to_string(),
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
    coordinator.dispatch_due_pending_dag_fetches_for_test(crate::rt::time::Instant::now())
}

#[nuclei::test(start_paused = true)]
async fn pulled_roots_continue_without_a_pushlog_registration() {
    let (blockstore, pending) = stores();
    let root = seed(&blockstore, &pending, "pulled").await;
    let view = Arc::new(PersistedView::default());
    pending.sweep(&view).await.unwrap();
    assert!(pending.remove(&root, &view).await.unwrap());
    let (coordinator, mut events) = restart(blockstore.clone(), pending).await;
    let handler = RetryThenMergeHandler::yielding(root, 2);
    let config = ReplicationConfig::default();
    coordinator
        .manager()
        .event_sender()
        .send(SyncEvent::BlockReceived {
            cid: root,
            doc_id: "pulled".into(),
            collection_id: "col1".into(),
            creator: "creator".into(),
            sender_peer: Some("source".into()),
            is_explicit_replicator: false,
            explicit_replay_authorization: None,
        })
        .await
        .unwrap();
    assert_eq!(coordinator.pending_dag_count(), 0);
    for turn in 0..3 {
        if turn > 0 {
            nuclei::time::advance(PENDING_MERGE_CONTINUATION_DELAY).await;
            assert_eq!(dispatch(&coordinator), 1);
        }
        let results =
            ReplicationLoop::process_next_batch(&coordinator, &mut events, &handler, &config).await;
        if turn < 2 {
            assert!(matches!(
                results.as_slice(),
                [ReplicationResult::Skipped {
                    terminal: false,
                    ..
                }]
            ));
            assert!(
                coordinator
                    .manager()
                    .pending_dag_snapshot(&root)
                    .unwrap()
                    .merge_continuation
            );
        } else {
            assert!(matches!(
                results.as_slice(),
                [ReplicationResult::Merged { .. }]
            ));
        }
    }
    assert!(blockstore.is_merged(&root).await.unwrap());
    assert_eq!(coordinator.pending_dag_count(), 0);
    coordinator.shutdown().await;
}

// Observe the scheduler's actual event, then return it to the production batch
// path. DagNeedsFetch here would redo discovery even if no network call occurs.
async fn assert_local_turn(
    coordinator: &TestCoordinator,
    events: &mut crate::sync::SyncEventReceiver,
    root: Cid,
) {
    let event = events.try_recv().expect("scheduled continuation");
    assert!(
        matches!(&event, SyncEvent::DagReady { root_cid, .. } if *root_cid == root),
        "expected local continuation for {root}, received {event:?}"
    );
    coordinator
        .manager()
        .event_sender()
        .send(event)
        .await
        .unwrap();
}

#[nuclei::test(start_paused = true)]
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
            assert_eq!(
                dispatch(&coordinator),
                1,
                "local progress is ready at turn {turn}"
            );
            assert_eq!(
                dispatch(&coordinator),
                0,
                "the queued continuation owns its slot"
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
    nuclei::time::advance(Duration::from_secs(120)).await;
    assert_eq!(dispatch(&coordinator), 0);
    assert!(events.try_recv().is_err());
    assert_eq!(handler.call_count.load(Ordering::SeqCst), TURNS + 2);
    coordinator.shutdown().await;
}

#[nuclei::test(start_paused = true)]
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
    nuclei::time::advance(Duration::from_millis(1)).await;
    coordinator
        .manager()
        .schedule_pending_merge_continuation(&other);
    nuclei::time::advance(PENDING_MERGE_CONTINUATION_DELAY).await;
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
    // Keep the independent root strictly older than the next committed yield.
    nuclei::time::advance(Duration::from_millis(1)).await;
    // Consume the event as the individual (duplicate-CID fallback) path does.
    let handler = RetryThenMergeHandler::yielding(root, 2);
    let event = events.try_recv().unwrap();
    let result = super::super::handlers::process_one_event(
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

#[nuclei::test(start_paused = true)]
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
    nuclei::time::advance(PENDING_MERGE_CONTINUATION_DELAY).await;
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
    nuclei::time::advance(PENDING_MERGE_CONTINUATION_DELAY).await;
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

#[nuclei::test(start_paused = true)]
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

#[nuclei::test(start_paused = true)]
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
    nuclei::time::advance(PENDING_MERGE_CONTINUATION_DELAY).await;
    assert_eq!(dispatch(&coordinator), 1);
    let event = events.try_recv().unwrap();
    assert!(
        matches!(&event, SyncEvent::DagReady { root_cid, .. } if *root_cid == root),
        "expected local continuation for {root}, received {event:?}"
    );
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

#[nuclei::test(start_paused = true)]
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
    nuclei::time::advance(PENDING_MERGE_CONTINUATION_DELAY).await;
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

#[nuclei::test(start_paused = true)]
async fn already_merged_duplicate_continuation_releases_reservation_without_handler() {
    let (blockstore, pending) = stores();
    let root = seed(&blockstore, &pending, "long-history").await;
    let (coordinator, mut events) = restart(blockstore.clone(), pending.clone()).await;
    coordinator.restore_pending_dags().await;
    let handler = RetryThenMergeHandler::yielding(root, 2);
    let config = ReplicationConfig::default();
    ReplicationLoop::process_next_batch(&coordinator, &mut events, &handler, &config).await;
    nuclei::time::advance(PENDING_MERGE_CONTINUATION_DELAY).await;
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

#[nuclei::test(start_paused = true)]
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
        nuclei::time::advance(PENDING_MERGE_CONTINUATION_DELAY).await;
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
        nuclei::time::advance(crate::sync::manager::pending::PENDING_RETRY_BASE).await;
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

#[nuclei::test(start_paused = true)]
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
    nuclei::time::advance(PENDING_MERGE_CONTINUATION_DELAY).await;
    assert_eq!(dispatch(&coordinator), 1);
    assert_local_turn(&coordinator, &mut events, root).await;
    let failing = TestMergeHandler::new(false, false);
    let results =
        ReplicationLoop::process_next_batch(&coordinator, &mut events, &failing, &config).await;
    assert!(matches!(
        results.as_slice(),
        [ReplicationResult::Failed { .. }]
    ));

    let mut delay = crate::sync::manager::pending::PENDING_RETRY_BASE;
    for round in 1..=3 {
        // From round 2 the failed DagReady came from a fetch task on the edge
        // runtime, which drops its slot only after it records the attempt.
        coordinator.pending_dag_work_released().await;
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
        nuclei::time::advance(delay).await;
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
    coordinator.pending_dag_work_released().await;
    assert_eq!(coordinator.pending_dag_work_count(), 0);
    assert_eq!(dispatch(&coordinator), 0);
    assert_eq!(failing.calls(), 4);
    assert!(!blockstore.is_merged(&root).await.unwrap());
    assert_eq!(pending.load_all().await.unwrap().len(), 1);
    coordinator.shutdown().await;
}

/// A fetch task sends its `DagReady` and only then records its attempt and
/// drops its slot. The gate holds the task between the two, as a busy edge
/// thread does, so the merge of that event finishes while the slot is held.
#[nuclei::test(start_paused = true)]
async fn a_fetch_task_keeps_its_slot_until_it_ends_after_its_dag_ready() {
    let (blockstore, pending) = stores();
    let root = seed(&blockstore, &pending, "long-history").await;
    let (coordinator, mut events) = restart(blockstore, pending).await;
    let (open, gate) = futures::channel::oneshot::channel::<()>();
    let event_tx = coordinator.manager().event_sender();
    assert!(
        coordinator.spawn_pending_dag_fetch_task(root, "gated_fetch", async move {
            event_tx.clone().send(dag_ready_event(root)).await.unwrap();
            gate.await.unwrap();
        })
    );
    let failing = TestMergeHandler::new(false, false);
    let results = ReplicationLoop::process_next_batch(
        &coordinator,
        &mut events,
        &failing,
        &ReplicationConfig::default(),
    )
    .await;
    assert!(matches!(
        results.as_slice(),
        [ReplicationResult::Failed { .. }]
    ));
    assert_eq!(
        coordinator.pending_dag_work_count(),
        1,
        "the fetch task still owns its slot"
    );
    open.send(()).unwrap();
    coordinator.pending_dag_work_released().await;
    assert_eq!(coordinator.pending_dag_work_count(), 0);
    coordinator.shutdown().await;
}

#[nuclei::test(start_paused = true)]
async fn retry_clock_continues_local_history_without_a_timer_tick() {
    let (blockstore, pending) = stores();
    let root = seed(&blockstore, &pending, "ready-now").await;
    let view = Arc::new(PersistedView::default());
    pending.sweep(&view).await.unwrap();
    assert!(pending.remove(&root, &view).await.unwrap());
    let (coordinator, mut events) = restart(blockstore.clone(), pending).await;
    let coordinator = Arc::new(coordinator);
    let clock_coordinator = coordinator.clone();
    let clock = crate::rt::spawn(async move {
        clock_coordinator
            .run_pending_dag_retry_clock(Duration::from_secs(60))
            .await
    });
    let handler = RetryThenMergeHandler::yielding(root, 2);
    coordinator
        .manager()
        .event_sender()
        .send(SyncEvent::BlockReceived {
            cid: root,
            doc_id: "ready-now".into(),
            collection_id: "col1".into(),
            creator: "creator".into(),
            sender_peer: Some("source".into()),
            is_explicit_replicator: false,
            explicit_replay_authorization: None,
        })
        .await
        .unwrap();
    let started = crate::rt::time::Instant::now();
    for turn in 0..3 {
        let results = crate::rt::time::timeout(
            Duration::from_millis(1),
            ReplicationLoop::process_next_batch(
                &coordinator,
                &mut events,
                &handler,
                &ReplicationConfig::default(),
            ),
        )
        .await
        .expect("locally ready history must not wait for a retry timer");
        if turn < 2 {
            assert!(matches!(
                results.as_slice(),
                [ReplicationResult::Skipped {
                    terminal: false,
                    ..
                }]
            ));
        } else {
            assert!(matches!(
                results.as_slice(),
                [ReplicationResult::Merged { .. }]
            ));
        }
    }
    assert_eq!(crate::rt::time::Instant::now(), started);
    assert!(blockstore.is_merged(&root).await.unwrap());
    assert_eq!(coordinator.pending_dag_count(), 0);
    coordinator.shutdown().await;
    clock.join().await.unwrap().unwrap();
}

/// Yields its first turn, then waits for an input that never arrives.
#[derive(Default)]
struct YieldThenWait {
    calls: AtomicUsize,
}

#[async_trait]
impl MergeHandler for YieldThenWait {
    type Error = TestError;

    async fn handle_block(
        &self,
        _cid: &Cid,
        _block_data: &[u8],
        _metadata: BlockMetadata<'_>,
    ) -> Result<MergeOutcome, Self::Error> {
        Ok(if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
            MergeOutcome::Yielded
        } else {
            MergeOutcome::retryable_skip("predecessor definition is not yet available")
        })
    }
}

/// A pull-sync merge that yields leaves a local continuation: no sender waits
/// on it and it has no durable record. When its next turn ends in a
/// non-terminal skip, the entry leaves the map at once, so it neither counts
/// against `max_pending_dags` nor is re-driven forever; the next pull or push
/// of the head starts the merge again.
#[nuclei::test(start_paused = true)]
async fn a_local_continuation_ends_on_a_non_terminal_skip() {
    let (blockstore, pending) = stores();
    let root = seed(&blockstore, &pending, "pulled").await;
    let view = Arc::new(PersistedView::default());
    pending.sweep(&view).await.unwrap();
    assert!(pending.remove(&root, &view).await.unwrap());
    let (coordinator, mut events) = restart(blockstore.clone(), pending).await;
    let handler = YieldThenWait::default();
    let config = ReplicationConfig::default();
    coordinator
        .manager()
        .event_sender()
        .send(SyncEvent::BlockReceived {
            cid: root,
            doc_id: "pulled".into(),
            collection_id: "col1".into(),
            creator: "creator".into(),
            sender_peer: Some("source".into()),
            is_explicit_replicator: false,
            explicit_replay_authorization: None,
        })
        .await
        .unwrap();
    ReplicationLoop::process_next_batch(&coordinator, &mut events, &handler, &config).await;
    assert_eq!(coordinator.pending_dag_count(), 1, "the yield continues");

    nuclei::time::advance(PENDING_MERGE_CONTINUATION_DELAY).await;
    assert_eq!(dispatch(&coordinator), 1);
    let results =
        ReplicationLoop::process_next_batch(&coordinator, &mut events, &handler, &config).await;
    assert!(matches!(
        results.as_slice(),
        [ReplicationResult::Skipped {
            terminal: false,
            ..
        }]
    ));
    assert_eq!(
        coordinator.pending_dag_count(),
        0,
        "a skipped local continuation must not keep a pending slot"
    );
    nuclei::time::advance(crate::sync::manager::pending::PENDING_RETRY_CAP).await;
    assert_eq!(dispatch(&coordinator), 0, "nothing re-drives it");
    assert!(!blockstore.is_merged(&root).await.unwrap());
    coordinator.shutdown().await;
}

/// A push for a root that only has a local continuation must not ride on it:
/// the continuation has no durable record and ends with its turn. The push
/// registers the root as its own durable obligation before it is
/// acknowledged, so a later turn that skips leaves the head owned.
#[nuclei::test(start_paused = true)]
async fn a_push_of_a_continued_root_is_registered_before_its_ack() {
    let (blockstore, pending) = stores();
    let root = seed(&blockstore, &pending, "pulled").await;
    let view = Arc::new(PersistedView::default());
    pending.sweep(&view).await.unwrap();
    assert!(pending.remove(&root, &view).await.unwrap());
    let (coordinator, mut events) = restart(blockstore.clone(), pending.clone()).await;
    let handler = YieldThenWait::default();
    let config = ReplicationConfig::default();
    coordinator
        .manager()
        .event_sender()
        .send(SyncEvent::BlockReceived {
            cid: root,
            doc_id: "pulled".into(),
            collection_id: "col1".into(),
            creator: "creator".into(),
            sender_peer: Some("source".into()),
            is_explicit_replicator: false,
            explicit_replay_authorization: None,
        })
        .await
        .unwrap();
    ReplicationLoop::process_next_batch(&coordinator, &mut events, &handler, &config).await;
    assert_eq!(coordinator.pending_dag_count(), 1, "the yield continues");

    let root_data = blockstore.get(&root).await.unwrap().unwrap();
    coordinator
        .manager()
        .process_pushlog(
            &crate::message::PushLogBroadcast::new(
                "pulled".to_string(),
                bytes::Bytes::from(root.to_bytes()),
                "col1".to_string(),
                "creator".to_string(),
                root_data,
            ),
            Some("source"),
            false,
            None,
        )
        .await
        .expect("the push is acknowledged");

    // The next merge turn of the root skips without a terminal outcome.
    let mut skipped = false;
    for _ in 0..8 {
        nuclei::time::advance(crate::sync::manager::pending::PENDING_RETRY_BASE).await;
        dispatch(&coordinator);
        let results =
            ReplicationLoop::process_next_batch(&coordinator, &mut events, &handler, &config).await;
        if matches!(
            results.as_slice(),
            [ReplicationResult::Skipped {
                terminal: false,
                ..
            }]
        ) {
            skipped = true;
            break;
        }
    }
    assert!(skipped, "the root's next merge turn skips");
    assert!(!blockstore.is_merged(&root).await.unwrap());
    assert_eq!(
        pending.load_all().await.unwrap().len(),
        1,
        "the acknowledged head keeps a durable record"
    );
    assert_eq!(coordinator.pending_dag_count(), 1, "and its pending entry");
    coordinator.shutdown().await;
}
