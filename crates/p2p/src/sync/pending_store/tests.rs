use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use multihash_codetable::{Code, MultihashDigest};
use storage::corekv::{
    Key, Reader, RetryAttemptFn, RetryAttemptFuture, RetryAttemptValue, RetryOptions, RetryTxn,
    Store, Txn,
};
use storage::keys::systemstore::P2PPendingDagKey;
use storage::stores::Systemstore;
use storage::RegolithStore;

struct ConflictInjectingStore {
    inner: Arc<RegolithStore>,
    conflict_key: Vec<u8>,
    conflicts_remaining: Arc<AtomicUsize>,
    attempt_calls: Arc<AtomicUsize>,
}

impl storage::corekv::private::Sealed for ConflictInjectingStore {}

#[async_trait]
impl Store for ConflictInjectingStore {
    fn crdts(&self) -> &storage::Crdts {
        self.inner.crdts()
    }

    async fn allocate(
        &self,
        key: &[u8],
        count: u64,
    ) -> storage::corekv::Result<std::ops::Range<u64>> {
        self.inner.allocate(key, count).await
    }

    fn transaction_stats_handle(&self) -> Option<storage::backends::TransactionStatsHandle> {
        self.inner.transaction_stats_handle()
    }

    async fn new_txn(&self, readonly: bool) -> storage::corekv::Result<Box<dyn Txn>> {
        self.inner.new_txn(readonly).await
    }

    async fn transact_retry(
        &self,
        options: RetryOptions,
        attempt: storage::corekv::RetryAttempt,
    ) -> std::result::Result<RetryAttemptValue, storage::corekv::RetryAttemptError> {
        let wrapped = Box::new(InjectCommitConflict {
            inner: attempt,
            store: Arc::clone(&self.inner),
            conflict_key: self.conflict_key.clone(),
            conflicts_remaining: Arc::clone(&self.conflicts_remaining),
            attempt_calls: Arc::clone(&self.attempt_calls),
        });
        self.inner.transact_retry(options, wrapped).await
    }

    async fn close(&self) -> storage::corekv::Result<()> {
        self.inner.close().await
    }
}

/// Runs the attempt, then, while a conflict is still owed, commits a write of
/// the key the attempt wrote, so its commit loses.
struct InjectCommitConflict {
    inner: storage::corekv::RetryAttempt,
    store: Arc<RegolithStore>,
    conflict_key: Vec<u8>,
    conflicts_remaining: Arc<AtomicUsize>,
    attempt_calls: Arc<AtomicUsize>,
}

impl RetryAttemptFn for InjectCommitConflict {
    fn run<'a>(&'a mut self, retry: &'a mut dyn RetryTxn) -> RetryAttemptFuture<'a> {
        Box::pin(async move {
            self.attempt_calls.fetch_add(1, Ordering::SeqCst);
            let output = self.inner.run(retry).await?;
            let owed = self
                .conflicts_remaining
                .try_update(Ordering::SeqCst, Ordering::SeqCst, |left| {
                    left.checked_sub(1)
                })
                .is_ok();
            if owed {
                let mut competitor = self.store.new_txn(false).await?;
                competitor
                    .set(&self.conflict_key, b"competing write")
                    .await?;
                competitor.commit().await?;
            }
            Ok(output)
        })
    }
}

fn conflict_injecting_store(conflicts: usize) -> (Arc<ConflictInjectingStore>, Arc<AtomicUsize>) {
    let inner = Arc::new(RegolithStore::in_memory().unwrap());
    let key = P2PPendingDagKey::new(cid(b"bounded retry test").to_string()).bytes();
    let mut conflict_key = Vec::with_capacity(1 + key.len());
    conflict_key.push(storage::namespace::Namespace::Systemstore.prefix());
    conflict_key.extend_from_slice(&key);
    let attempts = Arc::new(AtomicUsize::new(0));
    let store = Arc::new(ConflictInjectingStore {
        inner,
        conflict_key,
        conflicts_remaining: Arc::new(AtomicUsize::new(conflicts)),
        attempt_calls: Arc::clone(&attempts),
    });
    (store, attempts)
}

fn record(doc: &str) -> PersistedPendingDag {
    PersistedPendingDag {
        doc_id: doc.to_string(),
        collection_id: "collection".to_string(),
        head_priority: None,
        creator: "creator".to_string(),
        source_peer: Some("peer-1".to_string()),
        alternate_providers: Vec::new(),
        is_explicit_replicator: true,
        explicit_replay_authorization: Some(PersistedReplayAuthorization {
            source_peer_id: "peer-1".to_string(),
            target_peer_id: "peer-2".to_string(),
            collection_id: "collection".to_string(),
            authorizer_did: "did:key:z123".to_string(),
            expires_at: 42,
            capability: Some("signed-capability".to_string()),
        }),
    }
}

/// A head of the scope (`peer-1`, `collection`, `doc`) at `priority`.
fn head(priority: u64) -> PersistedPendingDag {
    PersistedPendingDag {
        head_priority: Some(priority),
        ..record("doc")
    }
}

fn cid(seed: &[u8]) -> Cid {
    Cid::new_v1(0x55, Code::Sha2_256.digest(seed))
}

fn memory_store() -> PendingDagStore<RegolithStore> {
    PendingDagStore::new(Arc::new(RegolithStore::in_memory().unwrap()))
}

fn view() -> Arc<PersistedView> {
    Arc::new(PersistedView::default())
}

async fn register(
    store: &PendingDagStore<RegolithStore>,
    view: &Arc<PersistedView>,
    root: Cid,
    record: &PersistedPendingDag,
    capacity: usize,
) -> Admission {
    store
        .register(
            Registration {
                root,
                record,
                capacity: Some(capacity),
            },
            view,
        )
        .await
        .unwrap()
}

fn sorted_roots(records: Vec<(Cid, PersistedPendingDag)>) -> Vec<Cid> {
    let mut roots: Vec<Cid> = records.into_iter().map(|(root, _)| root).collect();
    roots.sort();
    roots
}

#[test]
fn legacy_pending_record_decodes_with_no_alternate_providers() {
    #[derive(Serialize)]
    struct LegacyPendingDag {
        doc_id: String,
        collection_id: String,
        head_priority: Option<u64>,
        creator: String,
        source_peer: Option<String>,
        is_explicit_replicator: bool,
        explicit_replay_authorization: Option<PersistedReplayAuthorization>,
    }

    let bytes = defra_core::cbor::to_vec(&LegacyPendingDag {
        doc_id: "doc".to_string(),
        collection_id: "collection".to_string(),
        head_priority: Some(1),
        creator: "creator".to_string(),
        source_peer: Some("origin".to_string()),
        is_explicit_replicator: true,
        explicit_replay_authorization: None,
    })
    .expect("encode legacy record");

    let decoded = PersistedPendingDag::from_bytes(&bytes).expect("decode legacy record");
    assert_eq!(decoded.source_peer.as_deref(), Some("origin"));
    assert!(decoded.alternate_providers.is_empty());
}

#[tokio::test]
async fn pending_store_retries_a_commit_conflict_once_then_persists_value() {
    let (root, attempts) = conflict_injecting_store(1);
    let store = PendingDagStore::new(root);
    let root_cid = cid(b"bounded retry test");

    store.put(&root_cid, &record("doc-a")).await.unwrap();

    assert_eq!(attempts.load(Ordering::SeqCst), 2);
    let loaded = store.load_all().await.unwrap();
    assert_eq!(loaded, vec![(root_cid, record("doc-a"))]);
}

#[tokio::test]
async fn pending_store_conflicts_run_under_the_configured_retry_budget() {
    let (root, attempts) = conflict_injecting_store(2);
    let store = PendingDagStore::new(Arc::clone(&root)).with_max_txn_retries(1);
    let root_cid = cid(b"bounded retry test");

    let error = store.put(&root_cid, &record("doc-a")).await.unwrap_err();

    assert_eq!(attempts.load(Ordering::SeqCst), 2);
    assert!(
        matches!(error, Error::Storage(ref message) if message.starts_with("reached maximum transaction reties")),
        "unexpected terminal error: {error:?}"
    );
    let key = P2PPendingDagKey::new(root_cid.to_string());
    let txn = Systemstore::new(Arc::clone(&root.inner))
        .new_txn(true)
        .await
        .unwrap();
    assert_eq!(
        txn.get(&key.bytes()).await.unwrap(),
        Some(bytes::Bytes::from_static(b"competing write"))
    );
    txn.discard();
}

/// A record written with its slot still reads as a registration, so a build
/// that predates slots reads it too.
#[tokio::test]
async fn a_registered_record_reads_as_the_registration_alone() {
    let inner = Arc::new(RegolithStore::in_memory().unwrap());
    let store = PendingDagStore::new(Arc::clone(&inner));
    let root = cid(b"slotted");
    assert_eq!(
        register(&store, &view(), root, &record("doc"), 4).await,
        Admission::Registered
    );

    let txn = Systemstore::new(inner).new_txn(true).await.unwrap();
    let bytes = txn
        .get(&P2PPendingDagKey::new(root.to_string()).bytes())
        .await
        .unwrap()
        .expect("the record");
    txn.discard();
    assert_eq!(
        PersistedPendingDag::from_bytes(&bytes).unwrap(),
        record("doc")
    );
    assert_eq!(store.load_all().await.unwrap(), vec![(root, record("doc"))]);
}

/// The cap is hard: with every slot held a new root is refused and nothing
/// is written; a discharge frees its slot for the next root.
#[tokio::test]
async fn the_cap_refuses_a_new_root_until_a_slot_frees() {
    let store = memory_store();
    let view = view();
    let (a, b, c) = (cid(b"a"), cid(b"b"), cid(b"c"));
    assert_eq!(
        register(&store, &view, a, &record("doc-a"), 2).await,
        Admission::Registered
    );
    assert_eq!(
        register(&store, &view, b, &record("doc-b"), 2).await,
        Admission::Registered
    );
    assert_eq!(
        register(&store, &view, c, &record("doc-c"), 2).await,
        Admission::AtCapacity
    );
    assert_eq!(sorted_roots(store.load_all().await.unwrap()), {
        let mut roots = vec![a, b];
        roots.sort();
        roots
    });
    assert!(!view.contains(&c));

    assert_eq!(
        register(&store, &view, a, &record("doc-a"), 2).await,
        Admission::Registered,
        "a root that holds a record keeps its slot"
    );

    assert!(store.remove(&a, &view).await.unwrap());
    assert!(!store.remove(&a, &view).await.unwrap(), "already gone");
    assert_eq!(
        register(&store, &view, c, &record("doc-c"), 2).await,
        Admission::Registered
    );
    assert!(view.contains(&b) && view.contains(&c) && !view.contains(&a));
    assert_eq!(view.len(), 2);
}

/// Concurrent registrations of new roots never hold more slots than the cap:
/// each slot read is validated, so two that claim one slot do not both
/// commit.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_registrations_never_exceed_the_cap() {
    let store = Arc::new(memory_store().with_max_txn_retries(64));
    let view = view();
    let mut tasks = Vec::new();
    for index in 0..16u32 {
        let store = Arc::clone(&store);
        let view = Arc::clone(&view);
        tasks.push(tokio::spawn(async move {
            let root = cid(&index.to_be_bytes());
            let record = record(&format!("doc-{index}"));
            store
                .register(
                    Registration {
                        root,
                        record: &record,
                        capacity: Some(4),
                    },
                    &view,
                )
                .await
                .unwrap()
        }));
    }
    let mut registered = 0;
    for task in tasks {
        match task.await.unwrap() {
            Admission::Registered => registered += 1,
            Admission::AtCapacity => {}
            Admission::Covered => panic!("no root belongs to a scope"),
        }
    }
    assert_eq!(registered, 4);
    assert_eq!(store.load_all().await.unwrap().len(), 4);
    assert_eq!(view.len(), 4);
}

/// Within a sender scope the newer head retires the older one, takes over
/// its slot even at the cap, and an older head that comes later is covered.
#[tokio::test]
async fn a_newer_head_retires_the_older_one_and_takes_its_slot() {
    let store = memory_store();
    let view = view();
    let (old, new) = (cid(b"old"), cid(b"new"));
    assert_eq!(
        register(&store, &view, old, &head(1), 1).await,
        Admission::Registered
    );
    assert_eq!(
        register(&store, &view, new, &head(2), 1).await,
        Admission::Registered
    );
    assert_eq!(store.load_all().await.unwrap(), vec![(new, head(2))]);
    assert!(view.contains(&new) && !view.contains(&old));

    assert_eq!(
        register(&store, &view, old, &head(1), 1).await,
        Admission::Covered
    );
    assert_eq!(store.load_all().await.unwrap(), vec![(new, head(2))]);

    assert!(store.remove(&new, &view).await.unwrap());
    assert_eq!(
        register(&store, &view, old, &head(1), 1).await,
        Admission::Registered,
        "a discharged head leaves its scope without a current head"
    );
}

/// Concurrent heads of one sender scope settle to the newest: the scope's
/// durable head is read and written by every registration of the scope, so
/// the engine orders them and the store never keeps an older head beside it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_heads_of_one_scope_leave_the_newest_alone() {
    let store = Arc::new(memory_store().with_max_txn_retries(64));
    let view = view();
    assert_eq!(
        register(&store, &view, cid(b"head-1"), &head(1), 8).await,
        Admission::Registered
    );
    let mut tasks = Vec::new();
    for priority in 2..10u64 {
        let store = Arc::clone(&store);
        let view = Arc::clone(&view);
        tasks.push(tokio::spawn(async move {
            let root = cid(format!("head-{priority}").as_bytes());
            let record = head(priority);
            store
                .register(
                    Registration {
                        root,
                        record: &record,
                        capacity: Some(8),
                    },
                    &view,
                )
                .await
                .unwrap()
        }));
    }
    for task in tasks {
        assert_ne!(task.await.unwrap(), Admission::AtCapacity);
    }
    let newest = cid(b"head-9");
    assert_eq!(store.load_all().await.unwrap(), vec![(newest, head(9))]);
    assert_eq!(view.roots(), vec![newest]);
}

/// A quarantine moves the root out of the live keyspace in one transaction,
/// frees its slot, and counts once however often it is repeated.
#[tokio::test]
async fn quarantine_moves_the_root_and_counts_it_once() {
    let store = memory_store();
    let view = view();
    let root = cid(b"rejected");
    assert_eq!(
        register(&store, &view, root, &record("doc"), 1).await,
        Admission::Registered
    );

    let entry = quarantined("doc", "unique constraint violation");
    assert!(!store.quarantine(&root, &entry, &view).await.unwrap());
    assert!(store.quarantine(&root, &entry, &view).await.unwrap());

    assert!(store.load_all().await.unwrap().is_empty());
    assert!(store.is_quarantined(&root).await.unwrap());
    assert!(!view.contains(&root));
    assert_eq!(view.quarantined(), 1);
    assert_eq!(
        register(&store, &view, cid(b"next"), &record("doc-next"), 1).await,
        Admission::Registered,
        "the quarantined root's slot is free"
    );
}

/// A sweep rebuilds the view from the records it read: a record the view
/// never saw appears, and a root the view kept without a record goes.
#[tokio::test]
async fn a_sweep_makes_the_view_the_records() {
    let store = memory_store();
    let view = view();
    let (kept, orphan) = (cid(b"kept"), cid(b"orphan"));
    assert_eq!(
        register(&store, &view, kept, &record("doc-kept"), 4).await,
        Admission::Registered
    );
    store.put(&orphan, &record("doc-orphan")).await.unwrap();
    assert!(store
        .remove(&kept, &Arc::new(PersistedView::default()))
        .await
        .unwrap());
    assert!(view.contains(&kept) && !view.contains(&orphan));

    let records = store.sweep(&view).await.unwrap();

    assert_eq!(records, vec![(orphan, record("doc-orphan"))]);
    assert!(view.contains(&orphan) && !view.contains(&kept));
    assert_eq!(view.len(), 1);
}

#[tokio::test]
async fn put_load_remove_roundtrip() {
    let store = memory_store();
    let view = view();
    let root_a = cid(b"a");
    let root_b = cid(b"b");

    store.put(&root_a, &record("doc-a")).await.unwrap();
    store.put(&root_b, &record("doc-b")).await.unwrap();

    let mut loaded = store.load_all().await.unwrap();
    loaded.sort_by(|(_, x), (_, y)| x.doc_id.cmp(&y.doc_id));
    assert_eq!(loaded.len(), 2);
    assert_eq!(loaded[0].1, record("doc-a"));
    assert_eq!(loaded[0].0, root_a);

    assert!(store.remove(&root_a, &view).await.unwrap());
    let remaining = store.load_all().await.unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].0, root_b);

    // Removing an absent record is a no-op, not an error.
    assert!(!store.remove(&root_a, &view).await.unwrap());
}

#[tokio::test]
async fn put_overwrites_existing_record() {
    let store = memory_store();
    let root = cid(b"a");
    store.put(&root, &record("doc-old")).await.unwrap();
    store.put(&root, &record("doc-new")).await.unwrap();

    let loaded = store.load_all().await.unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].1.doc_id, "doc-new");
}

fn quarantined(doc: &str, reason: &str) -> PersistedQuarantinedDag {
    PersistedQuarantinedDag {
        record: record(doc),
        reason: reason.to_string(),
        quarantined_at_unix_secs: 1_700_000_000,
    }
}

#[tokio::test]
async fn quarantine_load_remove_roundtrip() {
    let store = memory_store();
    let root = cid(b"a");

    assert!(!store.is_quarantined(&root).await.unwrap());

    let entry = quarantined("doc-a", "unique constraint violation");
    store.quarantine(&root, &entry, &view()).await.unwrap();

    assert!(store.is_quarantined(&root).await.unwrap());

    let loaded = store.load_quarantined().await.unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].0, root);
    assert_eq!(loaded[0].1, entry);

    store.remove_quarantined(&root).await.unwrap();
    assert!(!store.is_quarantined(&root).await.unwrap());
    assert!(store.load_quarantined().await.unwrap().is_empty());

    // Removing an absent record is a no-op, not an error.
    store.remove_quarantined(&root).await.unwrap();
}

/// The quarantine keyspace is deliberately outside `/p2p/pending_dag/`:
/// the live resync sweep's prefix scan must never observe a quarantined root,
/// or it would re-drive a merge known to fail every time.
#[tokio::test]
async fn quarantined_root_is_absent_from_live_load_all() {
    let store = memory_store();
    let live_root = cid(b"live");
    let quarantined_root = cid(b"quarantined");

    store.put(&live_root, &record("doc-live")).await.unwrap();
    store
        .quarantine(
            &quarantined_root,
            &quarantined("doc-quarantined", "unique constraint violation"),
            &view(),
        )
        .await
        .unwrap();

    let live = store.load_all().await.unwrap();
    assert_eq!(live.len(), 1);
    assert_eq!(live[0].0, live_root);

    let quarantined_records = store.load_quarantined().await.unwrap();
    assert_eq!(quarantined_records.len(), 1);
    assert_eq!(quarantined_records[0].0, quarantined_root);
}
