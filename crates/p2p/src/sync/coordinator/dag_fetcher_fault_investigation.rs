use super::*;

#[derive(Clone, Copy)]
enum ChildReadFault {
    MissingAfterHas,
    ErrorAfterHas,
    TransactionConflict,
    ErrorOnce,
    ErrorOnceAfterReads(usize),
    CorruptAfterReads(usize),
}

struct FaultyChildStore {
    inner: Arc<DefraBlockstore<RegolithStore>>,
    child: Cid,
    fault: ChildReadFault,
    enabled: AtomicBool,
    fail_has: AtomicBool,
    injected_reads: AtomicUsize,
}

#[async_trait]
impl Blockstore for FaultyChildStore {
    async fn get(&self, cid: &Cid) -> blockstore::Result<Option<bytes::Bytes>> {
        if *cid == self.child && self.enabled.load(Ordering::SeqCst) {
            let previous = self.injected_reads.fetch_add(1, Ordering::SeqCst);
            return match self.fault {
                ChildReadFault::MissingAfterHas => Ok(None),
                ChildReadFault::ErrorAfterHas => Err(blockstore::Error::Internal(
                    "injected child read error".into(),
                )),
                ChildReadFault::TransactionConflict => Err(blockstore::Error::Storage(
                    storage::corekv::Error::TxnConflict,
                )),
                ChildReadFault::ErrorOnce if previous == 0 => Err(blockstore::Error::Internal(
                    "injected transient error".into(),
                )),
                ChildReadFault::ErrorOnce => self.inner.get(cid).await,
                ChildReadFault::ErrorOnceAfterReads(allowed) if previous == allowed => Err(
                    blockstore::Error::Internal("injected delayed transient error".into()),
                ),
                ChildReadFault::ErrorOnceAfterReads(_) => self.inner.get(cid).await,
                ChildReadFault::CorruptAfterReads(allowed) if previous >= allowed => {
                    Ok(Some(bytes::Bytes::from_static(&[0xff])))
                }
                ChildReadFault::CorruptAfterReads(_) => self.inner.get(cid).await,
            };
        }
        self.inner.get(cid).await
    }
    async fn put(&self, cid: &Cid, data: &[u8]) -> blockstore::Result<()> {
        self.inner.put(cid, data).await
    }
    async fn put_many(&self, blocks: &[(&Cid, &[u8])]) -> blockstore::Result<()> {
        self.inner.put_many(blocks).await
    }
    async fn has(&self, cid: &Cid) -> blockstore::Result<bool> {
        if *cid == self.child && self.fail_has.load(Ordering::SeqCst) {
            return Err(blockstore::Error::Internal(
                "injected child has error".into(),
            ));
        }
        self.inner.has(cid).await
    }
    async fn delete(&self, cid: &Cid) -> blockstore::Result<()> {
        self.inner.delete(cid).await
    }
    async fn get_size(&self, cid: &Cid) -> blockstore::Result<Option<usize>> {
        self.inner.get_size(cid).await
    }
    async fn all_cids(&self) -> blockstore::Result<Vec<Cid>> {
        self.inner.all_cids().await
    }
    fn hash_on_read(&self, enabled: bool) {
        self.inner.hash_on_read(enabled)
    }
    async fn is_merged(&self, cid: &Cid) -> blockstore::Result<bool> {
        self.inner.is_merged(cid).await
    }
    async fn mark_as_merged(&self, cid: &Cid) -> blockstore::Result<()> {
        self.inner.mark_as_merged(cid).await
    }
    async fn get_unmerged(&self) -> blockstore::Result<Vec<Cid>> {
        self.inner.get_unmerged().await
    }
}

async fn observe_unread_child(fault: ChildReadFault) {
    let inner = Arc::new(DefraBlockstore::new(
        Arc::new(RegolithStore::in_memory().unwrap()),
        true,
    ));
    let child_data = encode_ipld(ipld!({"unread_leaf": 1}));
    let child = make_cid(&child_data);
    let root_data = encode_ipld(ipld!({"child": child}));
    let root = make_cid(&root_data);
    inner.put(&child, &child_data).await.unwrap();
    inner.put(&root, &root_data).await.unwrap();
    assert!(inner.has(&child).await.unwrap());
    assert!(!inner.is_merged(&child).await.unwrap());
    let faulty = Arc::new(FaultyChildStore {
        inner: inner.clone(),
        child,
        fault,
        enabled: AtomicBool::new(true),
        fail_has: AtomicBool::new(false),
        injected_reads: AtomicUsize::new(0),
    });
    let transport = TestTransport::new(
        inner,
        root,
        root_data,
        RapidHashMap::new(),
        RapidHashMap::new(),
    );
    let (events, mut receiver) = mpsc::channel(1);
    let outcome = poll_fetch_dag(
        transport.clone(),
        faulty.clone(),
        events,
        root,
        DagFetchContext::new(
            "doc".into(),
            "collection".into(),
            "creator".into(),
            PeerId::new("remote-peer".into()),
        ),
        DagFetchLimiter::new(1),
        diagnostics(),
    )
    .await;
    assert!(matches!(outcome.0, FetchTaskEnd::LocalFailure));
    assert!(
        receiver.try_recv().is_err(),
        "an inconclusive walk must not emit DagReady"
    );
    assert!(faulty.injected_reads.load(Ordering::SeqCst) >= 1);
    assert_eq!(transport.car_request_count(), 0);
    assert!(transport.sync_batches().is_empty());
    assert!(
        transport.disconnected_peers().is_empty(),
        "a local failure must not penalize the provider"
    );
}

/// A child disappearing after has must not be called ready.
#[nuclei::test(start_paused = true)]
async fn missing_after_has_never_emits_dag_ready_for_unread_child() {
    observe_unread_child(ChildReadFault::MissingAfterHas).await;
}

/// An actual child read error must not be called ready.
#[nuclei::test(start_paused = true)]
async fn read_error_after_has_never_emits_dag_ready_for_unread_child() {
    observe_unread_child(ChildReadFault::ErrorAfterHas).await;
}

/// A later walker parse error exercises completion paths that swallow an Err,
/// independently of the preceding has/get fault in the current walker.
async fn observe_late_parse_error(after_car: bool) {
    let inner = Arc::new(DefraBlockstore::new(
        Arc::new(RegolithStore::in_memory().unwrap()),
        true,
    ));
    let leaf_data = encode_ipld(ipld!({"value": 1}));
    let leaf = make_cid(&leaf_data);
    let child_data = encode_ipld(ipld!({"leaf": leaf}));
    let child = make_cid(&child_data);
    let root_data = encode_ipld(ipld!({"child": child}));
    let root = make_cid(&root_data);
    inner.put(&root, &root_data).await.unwrap();
    inner.put(&child, &child_data).await.unwrap();
    let allowed = if after_car { 2 } else { 4 };
    let faulty = Arc::new(FaultyChildStore {
        inner: inner.clone(),
        child,
        fault: ChildReadFault::CorruptAfterReads(allowed),
        enabled: AtomicBool::new(true),
        fail_has: AtomicBool::new(false),
        injected_reads: AtomicUsize::new(0),
    });
    let car = if after_car {
        RapidHashMap::from_iter([(leaf, leaf_data.clone())])
    } else {
        RapidHashMap::new()
    };
    let selective = RapidHashMap::from_iter([(leaf, leaf_data)]);
    let transport = TestTransport::new(inner, root, root_data, car, selective);
    if !after_car {
        transport.set_force_rooted_sync();
    }
    assert!(
        crate::sync::manager::links::extract_ipld_links(&[0xff]).is_err(),
        "the injected bytes must actually produce a walker error"
    );
    let (events, mut receiver) = mpsc::channel(1);
    let context = DagFetchContext::new(
        "doc".into(),
        "collection".into(),
        "creator".into(),
        PeerId::new("remote-peer".into()),
    );
    let context = if after_car {
        context.with_rooted_provider_discovery()
    } else {
        context
    };
    let outcome = poll_fetch_dag(
        transport.clone(),
        faulty.clone(),
        events,
        root,
        context,
        DagFetchLimiter::new(1),
        diagnostics(),
    )
    .await;
    assert!(matches!(outcome.0, FetchTaskEnd::LocalFailure));
    assert!(
        receiver.try_recv().is_err(),
        "an inconclusive walk must not emit DagReady"
    );
    assert!(faulty.injected_reads.load(Ordering::SeqCst) > allowed);
    if after_car {
        assert_eq!(transport.car_request_count(), 1);
    } else {
        assert_eq!(transport.car_request_count(), 0);
        assert!(!transport.sync_batches().is_empty());
    }
}

#[nuclei::test(start_paused = true)]
async fn walker_parse_error_after_car_never_emits_dag_ready() {
    observe_late_parse_error(true).await;
}

#[nuclei::test(start_paused = true)]
async fn walker_parse_error_at_final_frontier_never_emits_dag_ready() {
    observe_late_parse_error(false).await;
}

#[derive(Debug, thiserror::Error)]
#[error("no merge expected")]
struct NoMergeExpected;
struct NoMerge;
#[async_trait]
impl crate::sync::MergeHandler for NoMerge {
    type Error = NoMergeExpected;
    async fn handle_block(
        &self,
        _: &Cid,
        _: &[u8],
        _: crate::sync::BlockMetadata<'_>,
    ) -> Result<crate::sync::MergeOutcome, Self::Error> {
        Err(NoMergeExpected)
    }
}

/// An acknowledged root remains owned and paced when its local scan fails.
#[test]
fn clock_owned_root_retains_registration_and_backoff_after_child_read_error() {
    use crate::sync::pending_store::{PendingDagStorage, PendingDagStore, PersistedPendingDag};
    use crate::sync::{
        ReplicationConfig, ReplicationLoop, ReplicationResult, SyncConfig, SyncCoordinator,
    };
    edge::testing::paused(|edge| async move {
        let inner = Arc::new(DefraBlockstore::new(
            Arc::new(RegolithStore::in_memory().unwrap()),
            true,
        ));
        let leaf_data = encode_ipld(ipld!({"value": 1}));
        let leaf = make_cid(&leaf_data);
        let child_data = encode_ipld(ipld!({"leaf": leaf}));
        let child = make_cid(&child_data);
        let root_data = encode_ipld(ipld!({"child": child}));
        let root = make_cid(&root_data);
        inner.put(&root, &root_data).await.unwrap();
        inner.put(&child, &child_data).await.unwrap();
        let faulty = Arc::new(FaultyChildStore {
            inner: inner.clone(),
            child,
            fault: ChildReadFault::ErrorAfterHas,
            enabled: AtomicBool::new(false),
            fail_has: AtomicBool::new(false),
            injected_reads: AtomicUsize::new(0),
        });
        let pending = Arc::new(PendingDagStore::new(Arc::new(
            RegolithStore::in_memory().unwrap(),
        )));
        pending
            .put(
                &root,
                &PersistedPendingDag {
                    doc_id: "doc".into(),
                    collection_id: "collection".into(),
                    head_priority: None,
                    creator: "creator".into(),
                    source_peer: Some("remote-peer".into()),
                    alternate_providers: Vec::new(),
                    is_explicit_replicator: false,
                    explicit_replay_authorization: None,
                },
            )
            .await
            .unwrap();
        let transport = TestTransport::new(
            inner,
            root,
            root_data,
            RapidHashMap::new(),
            RapidHashMap::new(),
        );
        let observer = transport.clone();
        let (coordinator, mut events) = SyncCoordinator::new(
            transport,
            faulty.clone(),
            crate::network::Network::on(edge),
            SyncConfig::default(),
        )
        .await
        .unwrap();
        coordinator.install_pending_dag_store(pending.clone()).await;
        assert_eq!(coordinator.restore_pending_dags().await, 1);
        faulty.enabled.store(true, Ordering::SeqCst);
        assert_eq!(
            coordinator.dispatch_due_pending_dag_fetches_for_test(Instant::now()),
            1
        );
        assert!(matches!(
            ReplicationLoop::process_next(
                &coordinator,
                &mut events,
                &NoMerge,
                &ReplicationConfig::default()
            )
            .await,
            ReplicationResult::DagFetchStarted { .. }
        ));
        for _ in 0..100 {
            if faulty.injected_reads.load(Ordering::SeqCst) > 0
                && coordinator.scheduler_diagnostics().reservations_running == 0
            {
                break;
            }
            crate::rt::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(faulty.injected_reads.load(Ordering::SeqCst) > 0);
        assert_eq!(coordinator.scheduler_diagnostics().reservations_running, 0);
        assert_eq!(
            pending.load_all().await.unwrap().len(),
            1,
            "acknowledged root remains durable"
        );
        assert!(!faulty.is_merged(&root).await.unwrap());
        assert!(
            events.try_recv().is_err(),
            "unread graph must not be emitted for merge"
        );
        assert_eq!(observer.car_request_count(), 0);
        assert_eq!(
            coordinator.dispatch_due_pending_dag_fetches_for_test(Instant::now()),
            0,
            "the existing clock owns paced re-drive"
        );
        crate::rt::time::sleep(Duration::from_secs(4)).await;
        assert_eq!(
            coordinator.dispatch_due_pending_dag_fetches_for_test(Instant::now()),
            1,
            "the retained root remains retryable"
        );
        coordinator.shutdown().await;
    });
}

#[nuclei::test]
async fn has_error_is_not_reported_as_missing_cid() {
    let inner = Arc::new(DefraBlockstore::new(
        Arc::new(RegolithStore::in_memory().unwrap()),
        true,
    ));
    let child_data = encode_ipld(ipld!({"value": 1}));
    let child = make_cid(&child_data);
    inner.put(&child, &child_data).await.unwrap();
    let root_data = encode_ipld(ipld!({"child": child}));
    let faulty = FaultyChildStore {
        inner,
        child,
        fault: ChildReadFault::ErrorAfterHas,
        enabled: AtomicBool::new(true),
        fail_has: AtomicBool::new(true),
        injected_reads: AtomicUsize::new(0),
    };
    let result = crate::sync::manager::links::find_all_missing_links(&faulty, &root_data).await;
    assert!(
        matches!(result, Err(crate::error::Error::BlockstoreError(ref reason)) if reason.contains("injected child has error"))
    );
    assert_eq!(faulty.injected_reads.load(Ordering::SeqCst), 0);
}

#[nuclei::test]
async fn has_false_still_reports_the_missing_cid() {
    let inner = Arc::new(DefraBlockstore::new(
        Arc::new(RegolithStore::in_memory().unwrap()),
        true,
    ));
    let child = make_cid(&encode_ipld(ipld!({"value": 1})));
    let root_data = encode_ipld(ipld!({"child": child}));
    assert_eq!(
        crate::sync::manager::links::find_all_missing_links(inner.as_ref(), &root_data)
            .await
            .unwrap(),
        vec![child]
    );
}

#[nuclei::test]
async fn disappearing_child_and_read_error_remain_distinguishable() {
    let inner = Arc::new(DefraBlockstore::new(
        Arc::new(RegolithStore::in_memory().unwrap()),
        true,
    ));
    let child_data = encode_ipld(ipld!({"value": 1}));
    let child = make_cid(&child_data);
    inner.put(&child, &child_data).await.unwrap();
    let root_data = encode_ipld(ipld!({"child": child}));
    let disappearing = FaultyChildStore {
        inner: inner.clone(),
        child,
        fault: ChildReadFault::MissingAfterHas,
        enabled: AtomicBool::new(true),
        fail_has: AtomicBool::new(false),
        injected_reads: AtomicUsize::new(0),
    };
    let failing = FaultyChildStore {
        inner,
        child,
        fault: ChildReadFault::ErrorAfterHas,
        enabled: AtomicBool::new(true),
        fail_has: AtomicBool::new(false),
        injected_reads: AtomicUsize::new(0),
    };
    let vanished = crate::sync::manager::links::find_all_missing_links(&disappearing, &root_data)
        .await
        .expect_err("a disappearing child is inconclusive");
    let read_error = crate::sync::manager::links::find_all_missing_links(&failing, &root_data)
        .await
        .expect_err("a read failure is inconclusive");
    assert!(vanished.to_string().contains("disappeared"));
    assert!(read_error.to_string().contains("injected child read error"));
}

/// A single transient failure may recover within the existing reply-driven
/// attempt owner; no new durable owner or retry loop is introduced.
#[nuclei::test(start_paused = true)]
async fn reply_driven_transient_local_read_error_recovers_in_existing_attempt() {
    let inner = Arc::new(DefraBlockstore::new(
        Arc::new(RegolithStore::in_memory().unwrap()),
        true,
    ));
    let child_data = encode_ipld(ipld!({"value": 1}));
    let child = make_cid(&child_data);
    let root_data = encode_ipld(ipld!({"child": child}));
    let root = make_cid(&root_data);
    inner.put(&child, &child_data).await.unwrap();
    inner.put(&root, &root_data).await.unwrap();
    let faulty = Arc::new(FaultyChildStore {
        inner: inner.clone(),
        child,
        fault: ChildReadFault::ErrorOnce,
        enabled: AtomicBool::new(true),
        fail_has: AtomicBool::new(false),
        injected_reads: AtomicUsize::new(0),
    });
    let transport = TestTransport::new(
        inner,
        root,
        root_data,
        RapidHashMap::new(),
        RapidHashMap::new(),
    );
    let (events, mut receiver) = mpsc::channel(1);
    let outcome = poll_fetch_dag(
        transport,
        faulty.clone(),
        events,
        root,
        DagFetchContext::new(
            "doc".into(),
            "collection".into(),
            "creator".into(),
            PeerId::new("remote-peer".into()),
        ),
        DagFetchLimiter::new(1),
        diagnostics(),
    )
    .await;
    assert!(matches!(outcome.0, FetchTaskEnd::Settled));
    assert!(
        matches!(receiver.try_recv(), Ok(SyncEvent::DagReady { root_cid, .. }) if root_cid == root)
    );
    assert!(faulty.injected_reads.load(Ordering::SeqCst) >= 2);
}

#[test]
fn reply_driven_local_failure_releases_fetch_permits_during_backoff() {
    edge::testing::paused(|_| async move {
        let inner = Arc::new(DefraBlockstore::new(
            Arc::new(RegolithStore::in_memory().unwrap()),
            true,
        ));
        let child_data = encode_ipld(ipld!({"value": 1}));
        let child = make_cid(&child_data);
        let root_data = encode_ipld(ipld!({"child": child}));
        let root = make_cid(&root_data);
        inner.put(&child, &child_data).await.unwrap();
        inner.put(&root, &root_data).await.unwrap();
        let faulty = Arc::new(FaultyChildStore {
            inner: inner.clone(),
            child,
            fault: ChildReadFault::ErrorAfterHas,
            enabled: AtomicBool::new(true),
            fail_has: AtomicBool::new(false),
            injected_reads: AtomicUsize::new(0),
        });
        let transport = TestTransport::new(
            inner,
            root,
            root_data,
            RapidHashMap::new(),
            RapidHashMap::new(),
        );
        let observer = transport.clone();
        let limiter = DagFetchLimiter::new(1);
        let owner_limiter = limiter.clone();
        let (events, _receiver) = mpsc::channel(1);
        let task_store = faulty.clone();
        let evidence = faulty.clone();
        let signal = Arc::new(regolith::sync::Notify::new());
        let task_signal = signal.clone();
        let observed_limiter = limiter.clone();
        let started = Instant::now();
        let task = crate::rt::spawn(async move {
            let mut future = Box::pin(poll_fetch_dag(
                transport,
                task_store,
                events,
                root,
                DagFetchContext::new(
                    "doc".into(),
                    "collection".into(),
                    "creator".into(),
                    PeerId::new("remote-peer".into()),
                ),
                owner_limiter,
                diagnostics(),
            ));
            std::future::poll_fn(|cx| {
                let result = std::future::Future::poll(future.as_mut(), cx);
                if result.is_pending()
                    && evidence.injected_reads.load(Ordering::SeqCst) >= 2
                    && observed_limiter.has_global_permit()
                {
                    task_signal.notify_one();
                }
                result
            })
            .await
        });
        // Observe the real owner returning Pending after its first failed
        // attempt, rather than racing the injected read or timer registration.
        crate::rt::time::timeout(Duration::from_secs(1), signal.notified())
            .await
            .expect("the owner enters backoff");
        let peer_permit = limiter.try_reserve_peer("remote-peer");
        assert_eq!(
            faulty.injected_reads.load(Ordering::SeqCst),
            2,
            "only the first attempt has read the child"
        );
        assert_eq!(Instant::now(), started, "virtual time has not advanced");
        assert!(
            !task.is_finished(),
            "the owner is pending before the backoff elapses"
        );
        for _ in 0..20 {
            crate::rt::yield_now().await;
        }
        assert_eq!(
            faulty.injected_reads.load(Ordering::SeqCst),
            2,
            "the next attempt cannot start before its backoff"
        );
        assert!(!task.is_finished(), "the owner remains pending");
        assert!(
            limiter.has_global_permit(),
            "global permit released during backoff"
        );
        assert!(peer_permit.is_some(), "peer permit released during backoff");
        drop(peer_permit);
        nuclei::time::advance(Duration::from_secs(2) + Duration::from_millis(1)).await;
        for _ in 0..100 {
            if faulty.injected_reads.load(Ordering::SeqCst) >= 3 {
                break;
            }
            crate::rt::time::sleep(Duration::from_millis(1)).await;
        }
        assert!(
            faulty.injected_reads.load(Ordering::SeqCst) >= 3,
            "the next attempt resumes after the backoff: reads={}, elapsed={:?}, finished={}, global_available={}",
            faulty.injected_reads.load(Ordering::SeqCst),
            Instant::now().duration_since(started),
            task.is_finished(),
            limiter.has_global_permit(),
        );
        assert!(
            observer.disconnected_peers().is_empty(),
            "a local fault is not a provider fault"
        );
        task.abort();
    });
}

#[nuclei::test(start_paused = true)]
async fn reply_driven_attempt_recovers_after_transient_local_error() {
    let inner = Arc::new(DefraBlockstore::new(
        Arc::new(RegolithStore::in_memory().unwrap()),
        true,
    ));
    let leaf_data = encode_ipld(ipld!({"value": 1}));
    let leaf = make_cid(&leaf_data);
    let child_data = encode_ipld(ipld!({"leaf": leaf}));
    let child = make_cid(&child_data);
    let root_data = encode_ipld(ipld!({"child": child}));
    let root = make_cid(&root_data);
    inner.put(&child, &child_data).await.unwrap();
    inner.put(&root, &root_data).await.unwrap();
    let faulty = Arc::new(FaultyChildStore {
        inner: inner.clone(),
        child,
        fault: ChildReadFault::ErrorOnceAfterReads(1),
        enabled: AtomicBool::new(true),
        fail_has: AtomicBool::new(false),
        injected_reads: AtomicUsize::new(0),
    });
    let transport = TestTransport::new(
        inner,
        root,
        root_data,
        RapidHashMap::new(),
        RapidHashMap::from_iter([(leaf, leaf_data)]),
    );
    let observer = transport.clone();
    let (events, mut receiver) = mpsc::channel(1);
    let outcome = poll_fetch_dag(
        transport,
        faulty,
        events,
        root,
        DagFetchContext::new(
            "doc".into(),
            "collection".into(),
            "creator".into(),
            PeerId::new("remote-peer".into()),
        ),
        DagFetchLimiter::new(1),
        diagnostics(),
    )
    .await;
    assert!(matches!(outcome.0, FetchTaskEnd::Settled));
    assert!(
        matches!(receiver.try_recv(), Ok(SyncEvent::DagReady { root_cid, .. }) if root_cid == root)
    );
    assert!(observer.disconnected_peers().is_empty());
}

#[nuclei::test(start_paused = true)]
async fn local_attempt_failure_does_not_terminally_exhaust_or_disconnect_after_network_failures() {
    let inner = Arc::new(DefraBlockstore::new(
        Arc::new(RegolithStore::in_memory().unwrap()),
        true,
    ));
    let leaf = make_cid(&encode_ipld(ipld!({"value": 1})));
    let child_data = encode_ipld(ipld!({"leaf": leaf}));
    let child = make_cid(&child_data);
    let root_data = encode_ipld(ipld!({"child": child}));
    let root = make_cid(&root_data);
    inner.put(&child, &child_data).await.unwrap();
    inner.put(&root, &root_data).await.unwrap();
    let faulty = Arc::new(FaultyChildStore {
        inner: inner.clone(),
        child,
        fault: ChildReadFault::ErrorOnceAfterReads(1),
        enabled: AtomicBool::new(true),
        fail_has: AtomicBool::new(false),
        injected_reads: AtomicUsize::new(0),
    });
    let transport = TestTransport::new(
        inner,
        root,
        root_data,
        RapidHashMap::new(),
        RapidHashMap::new(),
    );
    let observer = transport.clone();
    let diag = diagnostics();
    let (events, mut receiver) = mpsc::channel(1);
    let outcome = poll_fetch_dag(
        transport,
        faulty,
        events,
        root,
        DagFetchContext::new(
            "doc".into(),
            "collection".into(),
            "creator".into(),
            PeerId::new("remote-peer".into()),
        ),
        DagFetchLimiter::new(1),
        diag.clone(),
    )
    .await;
    assert!(matches!(outcome.0, FetchTaskEnd::LocalFailure));
    assert!(receiver.try_recv().is_err());
    assert!(
        observer.disconnected_peers().is_empty(),
        "local failure must not cause peer disconnection"
    );
    assert_eq!(diag.snapshot().pending_dag_fetch_exhausted, 0);
}

/// The DocSync reply handler reports its local preflight failure, while the
/// same root is handed to its existing bounded reply-driven fetch owner.
#[test]
fn doc_sync_transient_preflight_error_still_schedules_recoverable_root() {
    edge::testing::paused(|edge| async move {
        let inner = Arc::new(DefraBlockstore::new(
            Arc::new(RegolithStore::in_memory().unwrap()),
            true,
        ));
        let child_data = encode_ipld(ipld!({"value": 1}));
        let child = make_cid(&child_data);
        let root_data = encode_ipld(ipld!({"child": child}));
        let root = make_cid(&root_data);
        inner.put(&child, &child_data).await.unwrap();
        inner.put(&root, &root_data).await.unwrap();
        let faulty = Arc::new(FaultyChildStore {
            inner: inner.clone(),
            child,
            fault: ChildReadFault::ErrorOnce,
            enabled: AtomicBool::new(true),
            fail_has: AtomicBool::new(false),
            injected_reads: AtomicUsize::new(0),
        });
        let transport = TestTransport::new(
            inner,
            root,
            root_data,
            RapidHashMap::new(),
            RapidHashMap::new(),
        );
        let (coordinator, mut events) = crate::sync::SyncCoordinator::new(
            transport,
            faulty,
            crate::network::Network::on(edge),
            crate::sync::SyncConfig::default(),
        )
        .await
        .unwrap();
        let result = coordinator
            .handle_doc_sync_reply(
                PeerId::new("remote-peer".into()),
                DocSyncReply::success(
                    "request",
                    vec![crate::message::DocSyncItem {
                        doc_id: "doc".into(),
                        heads: vec![root.to_bytes()],
                    }],
                ),
            )
            .await;
        assert!(
            result.is_err(),
            "the reply handler reports its local preflight failure"
        );
        let event = crate::rt::time::timeout(Duration::from_secs(10), events.recv())
            .await
            .expect("the existing fetch owner should progress");
        assert!(matches!(event, Some(SyncEvent::DagReady { root_cid, .. }) if root_cid == root));
        coordinator.shutdown().await;
    });
}

/// With no connected provider, a reply-driven preflight error is reported to
/// its caller; recovery needs a later trigger, demonstrated explicitly here.
#[nuclei::test(start_paused = true)]
async fn reply_preflight_error_without_provider_returns_local_failure_until_retriggered() {
    let inner = Arc::new(DefraBlockstore::new(
        Arc::new(RegolithStore::in_memory().unwrap()),
        true,
    ));
    let child_data = encode_ipld(ipld!({"value": 1}));
    let child = make_cid(&child_data);
    let root_data = encode_ipld(ipld!({"child": child}));
    let root = make_cid(&root_data);
    inner.put(&child, &child_data).await.unwrap();
    inner.put(&root, &root_data).await.unwrap();
    let faulty = Arc::new(FaultyChildStore {
        inner: inner.clone(),
        child,
        fault: ChildReadFault::ErrorOnce,
        enabled: AtomicBool::new(true),
        fail_has: AtomicBool::new(false),
        injected_reads: AtomicUsize::new(0),
    });
    let transport = TestTransport::new(
        inner,
        root,
        root_data,
        RapidHashMap::new(),
        RapidHashMap::new(),
    );
    transport.set_connected_peers(Vec::new());
    let (events, mut receiver) = mpsc::channel(1);
    let context = DagFetchContext::new(
        "doc".into(),
        "collection".into(),
        "creator".into(),
        PeerId::new("remote-peer".into()),
    );
    let outcome = poll_fetch_dag(
        transport.clone(),
        faulty.clone(),
        events,
        root,
        context.clone(),
        DagFetchLimiter::new(1),
        diagnostics(),
    )
    .await;
    assert!(matches!(outcome.0, FetchTaskEnd::LocalFailure));
    assert!(receiver.try_recv().is_err());
    assert_eq!(faulty.injected_reads.load(Ordering::SeqCst), 1);
    assert_eq!(transport.car_request_count(), 0);
    let (events, mut receiver) = mpsc::channel(1);
    let outcome = poll_fetch_dag(
        transport,
        faulty,
        events,
        root,
        context,
        DagFetchLimiter::new(1),
        diagnostics(),
    )
    .await;
    assert!(matches!(outcome.0, FetchTaskEnd::Settled));
    assert!(
        matches!(receiver.try_recv(), Ok(SyncEvent::DagReady { root_cid, .. }) if root_cid == root)
    );
}

#[derive(Clone, Copy)]
enum LaterExit {
    Unservable,
    Deferred,
    Busy,
}

async fn assert_local_failure_survives_later_exit(exit: LaterExit) {
    let inner = Arc::new(DefraBlockstore::new(
        Arc::new(RegolithStore::in_memory().unwrap()),
        true,
    ));
    let leaf = make_cid(&encode_ipld(ipld!({"value": 1})));
    let child_data = encode_ipld(ipld!({"leaf": leaf}));
    let child = make_cid(&child_data);
    let root_data = encode_ipld(ipld!({"child": child}));
    let root = make_cid(&root_data);
    inner.put(&child, &child_data).await.unwrap();
    inner.put(&root, &root_data).await.unwrap();
    let faulty = Arc::new(FaultyChildStore {
        inner: inner.clone(),
        child,
        fault: ChildReadFault::ErrorOnceAfterReads(1),
        enabled: AtomicBool::new(true),
        fail_has: AtomicBool::new(false),
        injected_reads: AtomicUsize::new(0),
    });
    let transport = TestTransport::new(
        inner,
        root,
        root_data,
        RapidHashMap::new(),
        RapidHashMap::new(),
    );
    transport.set_force_rooted_sync();
    let completion = crate::sync::manager::BlockSyncCompletionTracker::default();
    match exit {
        LaterExit::Unservable => {
            transport
                .size_limited_providers
                .insert("remote-peer".into(), (leaf, completion.clone()));
        }
        LaterExit::Deferred => transport.set_early_deferred_completion(completion.clone()),
        LaterExit::Busy => transport.set_busy_from_batch(0, completion.clone()),
    }
    let observer = transport.clone();
    let diag = diagnostics();
    let (events, mut receiver) = mpsc::channel(1);
    let context = DagFetchContext::new(
        "doc".into(),
        "collection".into(),
        "creator".into(),
        PeerId::new("remote-peer".into()),
    )
    .with_block_sync_completions(completion);
    let outcome = poll_fetch_dag(
        transport,
        faulty,
        events,
        root,
        context,
        DagFetchLimiter::new(1),
        diag.clone(),
    )
    .await;
    assert!(
        !observer.sync_batches().is_empty(),
        "the later transport outcome was reached"
    );
    assert!(
        matches!(outcome.0, FetchTaskEnd::LocalFailure),
        "the earlier local fault remains visible"
    );
    assert!(receiver.try_recv().is_err());
    assert!(observer.disconnected_peers().is_empty());
    let snapshot = diag.snapshot();
    assert_eq!(snapshot.pending_dag_fetch_exhausted, 0);
    match exit {
        LaterExit::Deferred => assert_eq!(snapshot.pending_dag_fetch_deferred_contention, 1),
        LaterExit::Busy => assert_eq!(snapshot.pending_dag_fetch_deferred_busy, 1),
        // The Unservable branch is covered by its independently verified latch mutation.
        LaterExit::Unservable => {}
    }
}

#[nuclei::test(start_paused = true)]
async fn reply_local_failure_survives_later_unservable() {
    assert_local_failure_survives_later_exit(LaterExit::Unservable).await;
}
#[nuclei::test(start_paused = true)]
async fn reply_local_failure_survives_later_storage_deferral() {
    assert_local_failure_survives_later_exit(LaterExit::Deferred).await;
}
#[nuclei::test(start_paused = true)]
async fn reply_local_failure_survives_later_provider_busy() {
    assert_local_failure_survives_later_exit(LaterExit::Busy).await;
}

#[nuclei::test]
async fn child_read_transaction_conflict_retains_retriable_type() {
    let inner = Arc::new(DefraBlockstore::new(
        Arc::new(RegolithStore::in_memory().unwrap()),
        true,
    ));
    let child_data = encode_ipld(ipld!({"value": 1}));
    let child = make_cid(&child_data);
    inner.put(&child, &child_data).await.unwrap();
    let root_data = encode_ipld(ipld!({"child": child}));
    let faulty = FaultyChildStore {
        inner,
        child,
        fault: ChildReadFault::TransactionConflict,
        enabled: AtomicBool::new(true),
        fail_has: AtomicBool::new(false),
        injected_reads: AtomicUsize::new(0),
    };
    let result = crate::sync::manager::links::find_all_missing_links(&faulty, &root_data).await;
    assert!(matches!(
        result,
        Err(crate::error::Error::BlockstoreTxnConflict)
    ));
}
