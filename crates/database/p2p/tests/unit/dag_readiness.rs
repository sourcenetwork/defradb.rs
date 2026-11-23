//! A DAG is ready only when every reachable unmerged block was read (#2600).
//!
//! Each test plants one local blockstore fault on a stored child: a read that
//! comes back empty although the block was linked and present (a delete inside
//! one walk), a read error, a storage conflict or a hash mismatch. No client or
//! testo action can make a local blockstore read fail, so these drive the walk,
//! the fetcher, the pushlog admission, the pending retry and the DocSync
//! re-merge directly, on the production code paths.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use bytes::Bytes;
use defra_core::{
    Block, CollectionDeltaPayload, CompositeDeltaPayload, CrdtDelta, DAGLink, LwwDeltaPayload,
};
use kovan::AtomOption;
use rapidhash::{HashMapExt, RapidHashMap};

use super::*;
use crate::error::Error;
use crate::sync::manager::links::{find_all_missing_links, frontier_or_unknown};
use crate::sync::manager::SyncManager;

/// What a planted fault makes a `get` of its block return.
#[derive(Clone, Copy, Debug)]
enum Fault {
    /// The block is deleted as it is read, so it reads as absent: a delete
    /// that lands after the walk saw the block stored.
    Gone,
    /// A storage read error.
    Storage,
    /// A storage transaction conflict.
    Conflict,
}

/// A fault on one CID. Only gets that find the block stored count: the
/// `skip` first of them read normally, the next `times` fail.
#[derive(Clone, Copy)]
struct Plan {
    cid: Cid,
    fault: Fault,
    skip: usize,
    times: usize,
}

/// `DefraBlockstore` with one fault that can be planted on a stored block,
/// and counters of every `get` and `has`.
struct FaultyBlockstore {
    inner: Arc<DefraBlockstore<RegolithStore>>,
    plan: AtomOption<Plan>,
    present_gets: AtomicUsize,
    gets: AtomicUsize,
    hases: AtomicUsize,
}

impl FaultyBlockstore {
    fn new() -> Arc<Self> {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        Arc::new(Self {
            inner: Arc::new(DefraBlockstore::new(store, true)),
            plan: AtomOption::none(),
            present_gets: AtomicUsize::new(0),
            gets: AtomicUsize::new(0),
            hases: AtomicUsize::new(0),
        })
    }

    /// Fail every get of `cid` that finds it stored.
    fn fail(&self, cid: Cid, fault: Fault) {
        self.fail_some(cid, fault, 0, usize::MAX);
    }

    /// Read `cid` normally `skip` times, then fail the next `times` gets.
    fn fail_some(&self, cid: Cid, fault: Fault, skip: usize, times: usize) {
        self.present_gets.store(0, Ordering::SeqCst);
        self.plan.store_some(Plan {
            cid,
            fault,
            skip,
            times,
        });
    }

    fn heal(&self) {
        self.plan.store_none();
    }

    fn reset_counts(&self) {
        self.gets.store(0, Ordering::SeqCst);
        self.hases.store(0, Ordering::SeqCst);
    }

    fn counts(&self) -> (usize, usize) {
        (
            self.gets.load(Ordering::SeqCst),
            self.hases.load(Ordering::SeqCst),
        )
    }
}

#[async_trait]
impl Blockstore for FaultyBlockstore {
    async fn get(&self, cid: &Cid) -> blockstore::Result<Option<Bytes>> {
        self.gets.fetch_add(1, Ordering::SeqCst);
        let data = self.inner.get(cid).await?;
        let plan = self.plan.load().map(|plan| *plan);
        let Some(plan) = plan.filter(|plan| plan.cid == *cid && data.is_some()) else {
            return Ok(data);
        };
        let n = self.present_gets.fetch_add(1, Ordering::SeqCst);
        if n < plan.skip || n - plan.skip >= plan.times {
            return Ok(data);
        }
        match plan.fault {
            Fault::Gone => {
                self.inner.delete(cid).await?;
                Ok(None)
            }
            Fault::Storage => Err(blockstore::Error::Internal(
                "injected storage read fault".into(),
            )),
            Fault::Conflict => Err(blockstore::Error::Storage(
                storage::corekv::Error::TxnConflict,
            )),
        }
    }

    async fn put(&self, cid: &Cid, data: &[u8]) -> blockstore::Result<()> {
        self.inner.put(cid, data).await
    }

    async fn put_many(&self, blocks: &[(&Cid, &[u8])]) -> blockstore::Result<()> {
        self.inner.put_many(blocks).await
    }

    async fn has(&self, cid: &Cid) -> blockstore::Result<bool> {
        self.hases.fetch_add(1, Ordering::SeqCst);
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
        self.inner.hash_on_read(enabled);
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

/// A root with one child `child` and, when `absent` is set, a second child
/// that is never stored. Root and `child` are stored unmerged.
async fn stored_dag(blockstore: &FaultyBlockstore, absent: bool) -> (Cid, Vec<u8>, Cid, Cid) {
    let (root, root_data, child, child_data) = single_child_dag();
    let gone_data = encode_ipld(ipld!({ "value": "never stored" }));
    let gone = make_cid(&gone_data);
    let (root, root_data) = if absent {
        let data = encode_ipld(ipld!({ "child": child, "gone": gone }));
        (make_cid(&data), data)
    } else {
        (root, root_data)
    };
    blockstore.put(&root, &root_data).await.unwrap();
    blockstore.put(&child, &child_data).await.unwrap();
    (root, root_data, child, gone)
}

// ----- The walk -----

#[test]
fn a_linked_child_that_reads_as_gone_is_missing() {
    workers::block_on(async {
        let blockstore = FaultyBlockstore::new();
        let (_, root_data, child, _) = stored_dag(&blockstore, false).await;
        blockstore.fail(child, Fault::Gone);

        let missing = find_all_missing_links(blockstore.as_ref(), &root_data).await;

        assert_eq!(
            missing.unwrap(),
            vec![child],
            "an unread child was taken as present"
        );
    })
    .unwrap();
}

#[test]
fn a_child_read_error_fails_the_walk() {
    workers::block_on(async {
        let blockstore = FaultyBlockstore::new();
        let (_, root_data, child, _) = stored_dag(&blockstore, false).await;
        blockstore.fail(child, Fault::Storage);

        let walk = find_all_missing_links(blockstore.as_ref(), &root_data).await;

        assert!(
            matches!(walk, Err(Error::BlockstoreError(ref msg)) if msg.contains("injected")),
            "a failed child read must fail the walk, got {walk:?}"
        );
    })
    .unwrap();
}

#[test]
fn a_child_read_conflict_stays_a_typed_retriable_conflict() {
    workers::block_on(async {
        let blockstore = FaultyBlockstore::new();
        let (_, root_data, child, _) = stored_dag(&blockstore, false).await;
        blockstore.fail(child, Fault::Conflict);

        let error = find_all_missing_links(blockstore.as_ref(), &root_data)
            .await
            .unwrap_err();

        assert!(matches!(error, Error::BlockstoreTxnConflict), "{error:?}");
        assert!(error.is_retriable());
    })
    .unwrap();
}

#[test]
fn an_absent_child_is_missing_and_each_child_is_read_once() {
    workers::block_on(async {
        let blockstore = FaultyBlockstore::new();
        let (_, root_data, _, gone) = stored_dag(&blockstore, true).await;
        blockstore.reset_counts();

        let missing = find_all_missing_links(blockstore.as_ref(), &root_data)
            .await
            .unwrap();

        assert_eq!(missing, vec![gone]);
        assert_eq!(
            blockstore.counts(),
            (2, 0),
            "one get per unmerged child, and no has"
        );
    })
    .unwrap();
}

#[test]
fn a_child_whose_bytes_mismatch_its_hash_fails_the_walk() {
    workers::block_on(async {
        let blockstore = FaultyBlockstore::new();
        let (root, root_data, child, _) = single_child_dag();
        blockstore.put(&root, &root_data).await.unwrap();
        blockstore
            .put(&child, &encode_ipld(ipld!({ "value": "corrupt" })))
            .await
            .unwrap();
        blockstore.hash_on_read(true);

        let walk = find_all_missing_links(blockstore.as_ref(), &root_data).await;

        assert!(
            matches!(walk, Err(Error::BlockstoreError(ref msg)) if msg.contains("hash mismatch")),
            "a corrupt child must not be dropped, got {walk:?}"
        );
    })
    .unwrap();
}

#[test]
fn an_unknown_frontier_is_the_root_and_is_counted() {
    let diagnostics = SyncDiagnostics::default();
    let root = make_cid(b"root");

    let frontier = frontier_or_unknown(root, Err(Error::BlockstoreTxnConflict), &diagnostics);

    assert_eq!(frontier, vec![root]);
    assert_eq!(diagnostics.snapshot().missing_link_walk_errors, 1);
    assert!(frontier_or_unknown(root, Ok(Vec::new()), &diagnostics).is_empty());
    assert_eq!(diagnostics.snapshot().missing_link_walk_errors, 1);
}

// ----- The fetcher -----

fn context() -> DagFetchContext {
    DagFetchContext::new(
        "doc-id".to_string(),
        "collection-id".to_string(),
        "creator-id".to_string(),
        PeerId::new("remote-peer".to_string()),
    )
}

fn transport(
    blockstore: &FaultyBlockstore,
    root: Cid,
    root_data: Vec<u8>,
    car: RapidHashMap<Cid, Vec<u8>>,
    selective: RapidHashMap<Cid, Vec<u8>>,
) -> TestTransport {
    TestTransport::new(blockstore.inner.clone(), root, root_data, car, selective)
}

async fn attempt(
    transport: &TestTransport,
    blockstore: &Arc<FaultyBlockstore>,
    root: Cid,
    diagnostics: &SyncDiagnostics,
) -> FetchAttemptOutcome {
    let context = context();
    let mut providers = ProviderRotation::new(context.providers());
    let mut state = ProviderFetchState::new(&mut providers, None, diagnostics);
    fetch_dag_attempt(transport, blockstore, root, &context, &mut state).await
}

/// Every walk inside the selective loop reads the child, and only the final
/// verification walk fails: the attempt must end incomplete.
#[test]
fn a_read_fault_at_final_verification_is_not_complete() {
    workers::block_on(async {
        let blockstore = FaultyBlockstore::new();
        let (root, root_data, child, child_data) = single_child_dag();
        blockstore.put(&root, &root_data).await.unwrap();
        let transport = transport(
            &blockstore,
            root,
            root_data,
            RapidHashMap::new(),
            RapidHashMap::from_iter([(child, child_data)]),
        );
        // The child is fetched in the loop, read by its next walk, then fails.
        blockstore.fail_some(child, Fault::Storage, 1, usize::MAX);
        let diagnostics = SyncDiagnostics::default();

        let outcome = attempt(&transport, &blockstore, root, &diagnostics).await;

        assert!(
            matches!(outcome, FetchAttemptOutcome::Incomplete(..)),
            "final verification fault reported {outcome:?}"
        );
        assert_eq!(transport.sync_batches(), vec![vec![child]]);
        assert_eq!(diagnostics.snapshot().missing_link_walk_errors, 1);
    })
    .unwrap();
}

/// A rooted CAR delivers the DAG and the walk right after it fails: the
/// attempt goes on to the selective walk and ends incomplete.
#[test]
fn a_read_fault_after_rooted_car_ingest_is_not_complete() {
    workers::block_on(async {
        let blockstore = FaultyBlockstore::new();
        let (root, root_data, child, child_data) = single_child_dag();
        let transport = transport(
            &blockstore,
            root,
            root_data,
            RapidHashMap::from_iter([(child, child_data)]),
            RapidHashMap::new(),
        );
        // Neither block is stored before the CAR lands.
        blockstore.fail(child, Fault::Storage);
        let diagnostics = SyncDiagnostics::default();

        let outcome = attempt(&transport, &blockstore, root, &diagnostics).await;

        assert_eq!(transport.car_request_count(), 1);
        assert!(
            matches!(outcome, FetchAttemptOutcome::Incomplete(..)),
            "a fault after the CAR reported {outcome:?}"
        );
        assert!(diagnostics.snapshot().missing_link_walk_errors >= 1);
    })
    .unwrap();
}

/// A child read as gone between two walks is asked for again.
#[test]
fn a_child_gone_between_walks_is_fetched_again() {
    workers::block_on(async {
        let blockstore = FaultyBlockstore::new();
        let (root, root_data, child, _) = stored_dag(&blockstore, false).await;
        let child_data = blockstore
            .inner
            .get(&child)
            .await
            .unwrap()
            .unwrap()
            .to_vec();
        let transport = transport(
            &blockstore,
            root,
            root_data,
            RapidHashMap::new(),
            RapidHashMap::from_iter([(child, child_data)]),
        );
        // Read at the attempt's first walk, deleted as the next one reads it.
        blockstore.fail_some(child, Fault::Gone, 1, 1);
        let diagnostics = SyncDiagnostics::default();

        let outcome = attempt(&transport, &blockstore, root, &diagnostics).await;

        assert!(
            matches!(outcome, FetchAttemptOutcome::Complete(_)),
            "{outcome:?}"
        );
        assert!(
            transport.sync_batches().contains(&vec![child]),
            "a child gone between walks must be fetched again: {:?}",
            transport.sync_batches()
        );
    })
    .unwrap();
}

async fn fetch_events(
    transport: TestTransport,
    blockstore: Arc<FaultyBlockstore>,
    root: Cid,
    context: DagFetchContext,
    limiter: DagFetchLimiter,
    diagnostics: Arc<SyncDiagnostics>,
) -> ((FetchTaskEnd, AttemptSample), Vec<SyncEvent>) {
    let (tx, mut rx) = mpsc::channel(8);
    let end = poll_fetch_dag(
        transport,
        blockstore,
        tx,
        root,
        context,
        limiter,
        diagnostics,
    )
    .await;
    let mut events = Vec::new();
    while let Ok(event) = rx.try_recv() {
        events.push(event);
    }
    (end, events)
}

/// With no provider connected, a local DAG whose child fails to read is not
/// ready: the fetch defers and leaves the root to its clock.
#[test]
fn no_provider_and_a_read_fault_is_not_ready() {
    workers::block_on(async {
        let blockstore = FaultyBlockstore::new();
        let (root, root_data, child, _) = stored_dag(&blockstore, false).await;
        let transport = transport(
            &blockstore,
            root,
            root_data,
            RapidHashMap::new(),
            RapidHashMap::new(),
        );
        transport.set_connected_peers(Vec::new());
        blockstore.fail(child, Fault::Storage);

        let (end, events) = fetch_events(
            transport,
            blockstore,
            root,
            context(),
            DagFetchLimiter::new(1),
            diagnostics(),
        )
        .await;

        assert_eq!(end, (FetchTaskEnd::Settled, AttemptSample::Skipped));
        assert!(
            events.is_empty(),
            "no DagReady for an unread DAG: {events:?}"
        );
    })
    .unwrap();
}

/// A superseded clock fetch over a DAG whose child fails to read ends
/// without DagReady.
#[test]
fn a_cancelled_fetch_with_a_read_fault_is_not_ready() {
    workers::block_on(async {
        let blockstore = FaultyBlockstore::new();
        let (root, root_data, child, _) = stored_dag(&blockstore, false).await;
        let transport = transport(
            &blockstore,
            root,
            root_data,
            RapidHashMap::new(),
            RapidHashMap::new(),
        );
        blockstore.fail(child, Fault::Storage);
        let (manager, _events) = SyncManager::new(
            blockstore.clone(),
            Arc::new(crate::sync::peer_state::PeerStateTracker::new()),
            crate::sync::manager::SyncConfig::default(),
        );
        let context = context().with_pending_lease(manager.pending_dag_lease(root));
        assert!(!context.is_current());

        let (end, events) = fetch_events(
            transport,
            blockstore,
            root,
            context,
            DagFetchLimiter::new(1),
            diagnostics(),
        )
        .await;

        assert_eq!(end.0, FetchTaskEnd::Settled);
        assert!(
            events.is_empty(),
            "no DagReady for an unread DAG: {events:?}"
        );
    })
    .unwrap();
}

/// A storage conflict on every walk leaves the fetch incomplete, retried in
/// place, and its limiter permit free afterwards.
#[test]
fn a_conflicting_read_is_incomplete_and_releases_its_permit() {
    workers::block_on(async {
        let blockstore = FaultyBlockstore::new();
        let (root, root_data, child, _) = stored_dag(&blockstore, false).await;
        let transport = transport(
            &blockstore,
            root,
            root_data,
            RapidHashMap::new(),
            RapidHashMap::new(),
        );
        blockstore.fail(child, Fault::Conflict);
        let limiter = DagFetchLimiter::new(1);
        let diagnostics = diagnostics();

        let (end, events) = fetch_events(
            transport,
            blockstore,
            root,
            context(),
            limiter.clone(),
            Arc::clone(&diagnostics),
        )
        .await;

        assert!(
            matches!(end.1, AttemptSample::Incomplete { .. }),
            "conflict ended {end:?}"
        );
        assert!(
            events.is_empty(),
            "no DagReady for an unread DAG: {events:?}"
        );
        assert_eq!(
            diagnostics.snapshot().missing_link_walk_errors,
            u64::from(MAX_FETCH_ATTEMPTS),
            "each attempt walks again and finds the frontier unknown"
        );
        let peer = PeerId::new("remote-peer".to_string());
        assert!(
            futures::FutureExt::now_or_never(limiter.acquire(&peer, None))
                .flatten()
                .is_some(),
            "the fetch's permit was not released"
        );
    })
    .unwrap();
}

// ----- PushLog admission and the pending retry -----

fn lww(field: &str) -> (Cid, Vec<u8>) {
    let block = Block::new(
        CrdtDelta::Lww(LwwDeltaPayload {
            field_name: field.to_string(),
            priority: 1,
            collection_version_id: "schema1".to_string(),
            data: b"value".to_vec(),
        }),
        vec![],
        vec![],
    );
    (block.generate_cid().unwrap(), block.to_dag_cbor().unwrap())
}

fn composite(priority: u64, links: &[(&str, Cid)]) -> (Cid, Vec<u8>) {
    let block = Block::new(
        CrdtDelta::Composite(CompositeDeltaPayload {
            collection_version_id: "schema1".to_string(),
            priority,
            status: 1,
        }),
        vec![],
        links
            .iter()
            .map(|(name, cid)| DAGLink::new(*name, *cid))
            .collect(),
    );
    (block.generate_cid().unwrap(), block.to_dag_cbor().unwrap())
}

fn collection(links: &[(&str, Cid)]) -> (Cid, Vec<u8>) {
    let block = Block::new(
        CrdtDelta::Collection(CollectionDeltaPayload {
            collection_version_id: "schema1".to_string(),
            priority: 1,
        }),
        vec![],
        links
            .iter()
            .map(|(name, cid)| DAGLink::new(*name, *cid))
            .collect(),
    );
    (block.generate_cid().unwrap(), block.to_dag_cbor().unwrap())
}

fn manager(
    blockstore: &Arc<FaultyBlockstore>,
) -> (
    SyncManager<FaultyBlockstore>,
    crate::sync::manager::SyncEventReceiver,
) {
    SyncManager::new(
        blockstore.clone(),
        Arc::new(crate::sync::peer_state::PeerStateTracker::new()),
        crate::sync::manager::SyncConfig::default(),
    )
}

async fn push(manager: &SyncManager<FaultyBlockstore>, doc: &str, root: Cid, bytes: &[u8]) {
    manager
        .process_pushlog(
            &PushLogBroadcast::new(
                doc.to_string(),
                Bytes::from(root.to_bytes()),
                "collection1".to_string(),
                "creator1".to_string(),
                Bytes::copy_from_slice(bytes),
            ),
            Some("peer-1"),
            true,
            None,
        )
        .await
        .expect("the push is acked");
}

fn dag_ready(events: &mut crate::sync::manager::SyncEventReceiver) -> Vec<Cid> {
    let mut ready = Vec::new();
    while let Ok(event) = events.try_recv() {
        if let SyncEvent::DagReady { root_cid, .. } = event {
            ready.push(root_cid);
        }
    }
    ready
}

/// A push whose stored child fails to read is acked and registered with an
/// unknown frontier; once the read heals, the retry completes it once.
#[test]
fn a_push_over_an_unreadable_child_is_acked_and_owned() {
    workers::block_on(async {
        let blockstore = FaultyBlockstore::new();
        let (manager, mut events) = manager(&blockstore);
        let (field, field_bytes) = lww("name");
        let (root, root_bytes) = composite(1, &[("name", field)]);
        blockstore.put(&field, &field_bytes).await.unwrap();
        blockstore.fail(field, Fault::Storage);

        push(&manager, "doc1", root, &root_bytes).await;

        assert_eq!(manager.pending_dag_missing(&root), vec![root]);
        assert!(manager.diagnostics().snapshot().missing_link_walk_errors >= 1);
        assert!(dag_ready(&mut events).is_empty());

        blockstore.heal();
        assert!(manager.retry_pending_dag(&root).await.unwrap());
        assert_eq!(dag_ready(&mut events), vec![root]);
    })
    .unwrap();
}

/// A pending retry whose child reads as gone, or fails to read, is not
/// ready: the root stays owned, with the child or an unknown frontier.
#[test]
fn a_pending_retry_over_an_unread_child_is_not_ready() {
    for fault in [Fault::Gone, Fault::Storage] {
        workers::block_on(async {
            let blockstore = FaultyBlockstore::new();
            let (manager, mut events) = manager(&blockstore);
            let (field, field_bytes) = lww("name");
            let (root, root_bytes) = composite(1, &[("name", field)]);
            push(&manager, "doc1", root, &root_bytes).await;
            assert_eq!(manager.pending_dag_missing(&root), vec![field]);
            blockstore.put(&field, &field_bytes).await.unwrap();
            blockstore.fail(field, fault);

            let ready = manager.retry_pending_dag(&root).await;

            assert!(!ready.unwrap(), "{fault:?}: an unread child read as ready");
            assert!(dag_ready(&mut events).is_empty(), "{fault:?}");
            let (frontier, errors) = match fault {
                Fault::Gone => (vec![field], 0),
                _ => (vec![root], 1),
            };
            assert_eq!(manager.pending_dag_missing(&root), frontier, "{fault:?}");
            assert_eq!(
                manager.diagnostics().snapshot().missing_link_walk_errors,
                errors,
                "{fault:?}"
            );
        })
        .unwrap();
    }
}

/// Two roots wait on one block; when it lands, one root's walk fails. The
/// other still completes, and the arrival is not an error.
#[test]
fn one_failing_root_does_not_stop_the_waiter_sweep() {
    workers::block_on(async {
        let blockstore = FaultyBlockstore::new();
        let (manager, mut events) = manager(&blockstore);
        let (shared, shared_bytes) = lww("shared");
        let (faulty, faulty_bytes) = lww("faulty");
        blockstore.put(&faulty, &faulty_bytes).await.unwrap();
        let (failing, failing_bytes) = composite(1, &[("shared", shared), ("faulty", faulty)]);
        let (healthy, healthy_bytes) = composite(2, &[("shared", shared)]);
        push(&manager, "doc1", failing, &failing_bytes).await;
        push(&manager, "doc2", healthy, &healthy_bytes).await;
        assert_eq!(manager.pending_dag_missing(&failing), vec![shared]);
        assert_eq!(manager.pending_dag_missing(&healthy), vec![shared]);
        blockstore.put(&shared, &shared_bytes).await.unwrap();
        blockstore.fail(faulty, Fault::Storage);

        let completed = manager.retry_pending_dags_waiting_on(&shared).await;

        assert_eq!(completed.unwrap(), vec![healthy]);
        assert_eq!(dag_ready(&mut events), vec![healthy]);
        assert_eq!(manager.pending_dag_missing(&failing), vec![failing]);
    })
    .unwrap();
}

/// A pushed head completes the frontier of a root waiting on it, whose own
/// walk then fails: the head, already registered, is still acked.
#[test]
fn a_registered_head_is_acked_when_a_root_it_advances_fails_to_read() {
    workers::block_on(async {
        let blockstore = FaultyBlockstore::new();
        let (manager, mut events) = manager(&blockstore);
        let (field, field_bytes) = lww("name");
        let (faulty, faulty_bytes) = lww("faulty");
        blockstore.put(&field, &field_bytes).await.unwrap();
        blockstore.put(&faulty, &faulty_bytes).await.unwrap();
        let (head, head_bytes) = composite(1, &[("name", field)]);
        let (root, root_bytes) = collection(&[("doc1", head), ("faulty", faulty)]);
        push(&manager, "doc1", root, &root_bytes).await;
        assert_eq!(manager.pending_dag_missing(&root), vec![head]);
        blockstore.fail(faulty, Fault::Storage);

        push(&manager, "doc1", head, &head_bytes).await;

        assert!(manager.pending_dag_snapshot(&head).is_some());
        assert_eq!(manager.pending_dag_missing(&root), vec![root]);
        assert!(!dag_ready(&mut events).contains(&root));
    })
    .unwrap();
}

// ----- DocSync re-merge -----

/// A DocSync reply names a local unmerged head whose child fails to read:
/// it is not re-merged as complete, it goes to the fetch list.
#[test]
fn doc_sync_remerge_over_an_unreadable_child_fetches_instead() {
    workers::block_on(async {
        let blockstore = FaultyBlockstore::new();
        let (field, field_bytes) = lww("name");
        let (root, root_bytes) = composite(1, &[("name", field)]);
        blockstore.put(&field, &field_bytes).await.unwrap();
        blockstore.put(&root, &root_bytes).await.unwrap();
        blockstore.fail(field, Fault::Storage);
        let (coordinator, mut events) = crate::sync::coordinator::SyncCoordinator::new(
            crate::sync::coordinator::access_tests::NoopTransport::new(),
            blockstore.clone(),
            crate::network::test_network(),
            crate::sync::manager::SyncConfig::default(),
        )
        .await
        .unwrap();

        coordinator
            .handle_doc_sync_reply(
                PeerId::new("remote-peer".to_string()),
                DocSyncReply {
                    results: vec![crate::message::DocSyncItem {
                        doc_id: "doc1".to_string(),
                        heads: vec![root.to_bytes()],
                    }],
                    ..DocSyncReply::default()
                },
            )
            .await
            .unwrap();

        while let Ok(event) = events.try_recv() {
            assert!(
                !matches!(event, SyncEvent::BlockReceived { cid, .. } if cid == root),
                "an unread DAG was re-merged as complete"
            );
        }
        assert_eq!(
            coordinator
                .manager()
                .diagnostics()
                .snapshot()
                .missing_link_walk_errors,
            1
        );
    })
    .unwrap();
}
