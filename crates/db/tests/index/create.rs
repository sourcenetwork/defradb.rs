//! `DB::create_index`: schema reads and index definitions share the transaction
//! validation path, while backfill remains restartable and exact.

use rapidhash::RapidHashSet;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use async_trait::async_trait;
use db::index::backfill::{encode_progress, BACKFILL_BATCH_DOCS};
use db::{AutoCommitMutator, BackfillSource, Collection, IndexManager, DB};
use document::{DocID, Document, NormalValue};
use query::DocMutator;
use schema::{
    CollectionVersion, DistanceMetric, FieldDescription, FieldKind, IndexKind,
    IndexedFieldDescription, OrderedIndexDescription, VectorAlgorithm, VectorIndexDescription,
};
use storage::corekv::{
    IterOptions, Iterator, Key, Reader, Result as StorageResult, Store, Txn, Writer,
};
use storage::index::IndexIterator;
use storage::keys::datastore::IndexDataStoreKey;
use storage::keys::systemstore::{ActionProgressKey, CollectionKey, CollectionNameKey};
use storage::RegolithStore;

use crate::common::schema::docs_schema;

pub(super) fn users_schema(name: &str, version_id: &str, collection_id: &str) -> CollectionVersion {
    CollectionVersion::new(
        name,
        version_id,
        collection_id,
        vec![
            FieldDescription::new(
                "1",
                "_docID",
                FieldKind::doc_id(),
                schema::CType::LwwRegister,
            ),
            FieldDescription::new("2", "name", FieldKind::string(), schema::CType::LwwRegister),
        ],
    )
}

pub(super) fn by_name() -> (Vec<IndexedFieldDescription>, IndexKind) {
    (
        vec![IndexedFieldDescription {
            name: "name".to_string(),
            descending: false,
        }],
        IndexKind::Ordered(OrderedIndexDescription { unique: false }),
    )
}

pub(super) async fn open_with_collection(
    name: &str,
    version_id: &str,
    collection_id: &str,
) -> (Arc<RegolithStore>, Arc<DB<RegolithStore>>) {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let db = Arc::new(DB::from_arc(store.clone()).unwrap());
    db.create_collection(users_schema(name, version_id, collection_id))
        .await
        .unwrap();
    (store, db)
}

pub(super) async fn create_user<S>(
    mutator: &AutoCommitMutator<S>,
    collection: &str,
    name: &str,
) -> DocID
where
    S: Store + 'static,
{
    let mut doc = Document::new(users_schema(collection, "v1", "col-users"));
    doc.set("name", NormalValue::String(name.to_owned()))
        .unwrap();
    mutator.create(collection, doc).await.unwrap().doc_id
}

/// Run `attempt` until it succeeds or fails for a reason other than a
/// transaction conflict, which a write racing a backfill batch may hit once.
async fn retrying<T, F, Fut>(mut attempt: F) -> T
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = query::error::Result<T>>,
{
    let mut tries = 0;
    loop {
        match attempt().await {
            Ok(value) => return value,
            Err(error) if error.to_string().contains("transaction conflict") && tries < 32 => {
                tries += 1
            }
            Err(error) => panic!("write failed for a reason other than a conflict: {error}"),
        }
    }
}

pub(super) async fn index_entries<S>(db: &DB<S>, collection: &str, index_id: u32) -> usize
where
    S: Store + 'static,
{
    let short_id = db
        .require_collection(collection)
        .unwrap()
        .schema()
        .resolved_root_id();
    let txn = db.new_txn(true).await.unwrap();
    let datastore = txn.datastore().unwrap();
    let mut iter = datastore
        .iterator(
            IterOptions::new().with_prefix(IndexDataStoreKey::index_prefix(short_id, index_id)),
        )
        .await
        .unwrap();
    let mut count = 0;
    while iter.next().await.unwrap().is_some() {
        count += 1;
    }
    count
}

pub(super) async fn entries_for<S>(db: &DB<S>, collection: &str, name: &str) -> usize
where
    S: Store + 'static,
{
    let collection = db.require_collection(collection).unwrap();
    let manager = IndexManager::from_collection(
        collection.schema().resolved_root_id(),
        collection.schema(),
        &crdt::Crdts::default(),
    )
    .unwrap();
    let txn = db.new_txn(true).await.unwrap();
    let datastore = txn.datastore().unwrap();
    let mut entries = manager
        .get_index("by_name")
        .unwrap()
        .get(&datastore, &[NormalValue::String(name.to_string())])
        .await
        .unwrap();
    let mut count = 0;
    while entries.next().await.unwrap().is_some() {
        count += 1;
    }
    count
}

async fn progress_recorded(db: &DB<RegolithStore>, collection_id: &str, index_id: u32) -> bool {
    let txn = db.new_txn(true).await.unwrap();
    let systemstore = txn.systemstore().unwrap();
    systemstore
        .has(
            &ActionProgressKey::new(
                collection_id,
                defra_core::Action::BACKFILL_INDEX,
                index_id.to_string(),
            )
            .bytes(),
        )
        .await
        .unwrap()
}

#[test]
fn create_index_indexes_the_documents_that_already_exist() {
    workers::block_on(async {
        let (_store, db) =
            open_with_collection("Backfilled", "backfilled-v1", "col-index-backfill").await;
        let mutator = AutoCommitMutator::new(db.clone());
        for name in ["ada", "grace", "linus"] {
            create_user(&mutator, "Backfilled", name).await;
        }

        let (fields, kind) = by_name();
        let index = db
            .create_index("Backfilled", Some("by_name"), fields, kind)
            .await
            .unwrap();

        // A completed backfill clears its action; only an errored or running one
        // is listed.
        let actions = db.list_index_actions("col-index-backfill").await.unwrap();
        assert!(
            !actions.contains_key(&index.id),
            "the backfill did not complete: {:?}",
            actions.get(&index.id)
        );
        assert!(!progress_recorded(&db, "col-index-backfill", index.id).await);
        assert_eq!(index_entries(&db, "Backfilled", index.id).await, 3);
        assert!(
            db.require_collection("Backfilled")
                .unwrap()
                .get_indexes()
                .iter()
                .any(|existing| existing.name == "by_name"),
            "the definition must be on the cached collection"
        );
    })
    .unwrap()
}

/// The index API must refuse what the schema refuses, or a vector index the
/// SDL would reject can still be created through the CLI or HTTP.
#[test]
fn create_index_refuses_vector_indexes_the_schema_refuses() {
    workers::block_on(async {
        let cases = [
            (
                VectorIndexDescription::with_defaults(
                    VectorAlgorithm::Hnsw,
                    DistanceMetric::Cosine,
                    0,
                ),
                "must set dimensions greater than zero",
            ),
            (
                VectorIndexDescription::with_defaults(
                    VectorAlgorithm::IvfFlat,
                    DistanceMetric::Dot,
                    4,
                ),
                "cannot rank by DOT",
            ),
        ];
        for (vector, expected) in cases {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let db = Arc::new(DB::from_arc(store).unwrap());
            db.create_collection(docs_schema()).await.unwrap();
            let fields = vec![IndexedFieldDescription {
                name: "embedding".to_string(),
                descending: false,
            }];

            let err = db
                .create_index(
                    "docs",
                    Some("by_embedding"),
                    fields,
                    IndexKind::Vector(vector),
                )
                .await
                .expect_err("the index must be refused");

            assert!(
                err.to_string().contains(expected),
                "expected {expected:?}, got: {err}"
            );
            assert!(
                db.require_collection("docs")
                    .unwrap()
                    .get_indexes()
                    .is_empty(),
                "a refused index must not be defined"
            );
        }
    })
    .unwrap()
}

#[test]
fn create_index_backfills_more_documents_than_a_batch_holds() {
    workers::block_on(async {
        let total = 2 * BACKFILL_BATCH_DOCS + 37;
        let (_store, db) = open_with_collection("Batched", "batched-v1", "col-index-batched").await;
        let mutator = AutoCommitMutator::new(db.clone());
        for i in 0..total {
            create_user(&mutator, "Batched", &format!("user-{i}")).await;
        }

        let (fields, kind) = by_name();
        let index = db
            .create_index("Batched", Some("by_name"), fields, kind)
            .await
            .unwrap();

        assert_eq!(index_entries(&db, "Batched", index.id).await, total);
        assert!(!db
            .list_index_actions("col-index-batched")
            .await
            .unwrap()
            .contains_key(&index.id));
        assert!(!progress_recorded(&db, "col-index-batched", index.id).await);
        assert_eq!(entries_for(&db, "Batched", "user-0").await, 1);
        assert_eq!(
            entries_for(&db, "Batched", &format!("user-{}", total - 1)).await,
            1
        );
    })
    .unwrap()
}

#[test]
fn create_index_commits_schema_and_publishes_cache() {
    workers::block_on(async {
        let (_store, db) = open_with_collection("Guarded", "guarded-v1", "col-index-guard").await;
        create_user(&AutoCommitMutator::new(db.clone()), "Guarded", "ada").await;

        let (fields, kind) = by_name();
        let index = db
            .create_index("Guarded", Some("by_name"), fields, kind)
            .await
            .unwrap();
        assert!(db
            .require_collection("Guarded")
            .unwrap()
            .get_indexes()
            .iter()
            .any(|cached| cached.id == index.id));
        assert_eq!(index_entries(&db, "Guarded", index.id).await, 1);
    })
    .unwrap()
}

/// Writers run through the whole backfill: creates, updates of documents the
/// batches have not reached yet and of ones they have, deletes. Afterwards
/// every live document has exactly one entry, for its current value, and
/// no other entry exists.
#[test]
fn autocommit_writes_refresh_index_state_from_the_attempt_snapshot() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let stale_db = Arc::new(DB::from_arc(store.clone()).unwrap());
        stale_db
            .create_collection(users_schema("Racing", "racing-v1", "col-index-racing"))
            .await
            .unwrap();
        let mutator = AutoCommitMutator::new(stale_db.clone());
        let updated_id = create_user(&mutator, "Racing", "before-update").await;
        let deleted_id = create_user(&mutator, "Racing", "before-delete").await;

        // This second DB handle commits the new index without publishing its cache
        // into `stale_db`. The autocommit mutator therefore keeps the pre-index
        // Collection while each engine attempt sees the post-index snapshot.
        let index_db = Arc::new(DB::open_from_arc(store).await.unwrap());
        let (fields, kind) = by_name();
        let index = index_db
            .create_index("Racing", Some("by_name"), fields, kind)
            .await
            .unwrap();

        create_user(&mutator, "Racing", "created-after-index").await;
        let mut update = Document::new(users_schema("Racing", "v1", "col-racing"));
        update.set_id(updated_id);
        update
            .set(
                "name",
                NormalValue::String("updated-after-index".to_string()),
            )
            .unwrap();
        mutator
            .update(
                "Racing",
                update,
                RapidHashSet::from_iter(["name".to_string()]),
            )
            .await
            .unwrap();
        mutator.delete("Racing", &deleted_id).await.unwrap();

        assert_eq!(index_entries(&index_db, "Racing", index.id).await, 2);
        assert_eq!(
            entries_for(&index_db, "Racing", "created-after-index").await,
            1
        );
        assert_eq!(
            entries_for(&index_db, "Racing", "updated-after-index").await,
            1
        );
        assert_eq!(entries_for(&index_db, "Racing", "before-delete").await, 0);
    })
    .unwrap()
}

#[test]
fn create_index_keeps_pace_with_concurrent_writers() {
    workers::block_on(async {
        let existing = 3 * BACKFILL_BATCH_DOCS;
        let (_store, db) = open_with_collection("Racing", "racing-v1", "col-index-racing").await;
        let mutator = Arc::new(AutoCommitMutator::new(db.clone()));
        let mut ids = Vec::with_capacity(existing);
        for i in 0..existing {
            ids.push(create_user(&mutator, "Racing", &format!("pre-{i}")).await);
        }
        let updated = 0..200;
        let deleted = 500..600;
        let created = 0..200;

        let creator = db.clone();
        let index_task = workers::spawn(
            Some(crate::common::worker_io::pool()),
            "test",
            move || async move {
                let (fields, kind) = by_name();
                creator
                    .create_index("Racing", Some("by_name"), fields, kind)
                    .await
            },
        )
        .unwrap();
        let updater = {
            let mutator = mutator.clone();
            let ids = ids.clone();
            let updated = updated.clone();
            workers::spawn(
                Some(crate::common::worker_io::pool()),
                "test",
                move || async move {
                    for i in updated {
                        retrying(|| async {
                            let mut doc = Document::new(users_schema("Racing", "v1", "col-racing"));
                            doc.set_id(ids[i].clone());
                            doc.set("name", NormalValue::String(format!("upd-{i}")))
                                .unwrap();
                            mutator
                                .update(
                                    "Racing",
                                    doc,
                                    RapidHashSet::from_iter(["name".to_string()]),
                                )
                                .await
                        })
                        .await;
                    }
                },
            )
            .unwrap()
        };
        let deleter = {
            let mutator = mutator.clone();
            let ids = ids.clone();
            let deleted = deleted.clone();
            workers::spawn(
                Some(crate::common::worker_io::pool()),
                "test",
                move || async move {
                    for i in deleted {
                        retrying(|| mutator.delete("Racing", &ids[i])).await;
                    }
                },
            )
            .unwrap()
        };
        let inserter = {
            let mutator = mutator.clone();
            let created = created.clone();
            workers::spawn(
                Some(crate::common::worker_io::pool()),
                "test",
                move || async move {
                    for i in created {
                        retrying(|| async {
                            let mut doc = Document::new(users_schema("Racing", "v1", "col-racing"));
                            doc.set("name", NormalValue::String(format!("new-{i}")))
                                .unwrap();
                            mutator.create("Racing", doc).await
                        })
                        .await;
                    }
                },
            )
            .unwrap()
        };
        let index = index_task.await.unwrap().unwrap();
        updater.await.unwrap();
        deleter.await.unwrap();
        inserter.await.unwrap();

        let actions = db.list_index_actions("col-index-racing").await.unwrap();
        assert!(
            !actions.contains_key(&index.id),
            "the backfill did not complete: {:?}",
            actions.get(&index.id)
        );
        for i in 0..existing {
            let (expect_pre, expect_upd) = if deleted.contains(&i) {
                (0, 0)
            } else if updated.contains(&i) {
                (0, 1)
            } else {
                (1, 0)
            };
            assert_eq!(
                entries_for(&db, "Racing", &format!("pre-{i}")).await,
                expect_pre,
                "pre-{i}"
            );
            assert_eq!(
                entries_for(&db, "Racing", &format!("upd-{i}")).await,
                expect_upd,
                "upd-{i}"
            );
        }
        for i in created.clone() {
            assert_eq!(
                entries_for(&db, "Racing", &format!("new-{i}")).await,
                1,
                "new-{i}"
            );
        }
        assert_eq!(
            index_entries(&db, "Racing", index.id).await,
            existing - deleted.len() + created.len()
        );
        assert!(!db
            .list_index_actions("col-index-racing")
            .await
            .unwrap()
            .contains_key(&index.id));
    })
    .unwrap()
}

#[test]
fn create_index_fence_includes_old_schema_writes_before_definition_commit() {
    workers::block_on(async {
        let inner = Arc::new(RegolithStore::in_memory().unwrap());
        let gate = Arc::new(DefinitionCommitGate::default());
        let store = Arc::new(PauseDefinitionCommitStore {
            inner,
            gate: Arc::clone(&gate),
        });
        let db = Arc::new(DB::from_arc(store).unwrap());
        db.create_collection(users_schema("Racing", "racing-v1", "col-index-gate"))
            .await
            .unwrap();
        let mutator = Arc::new(AutoCommitMutator::new(db.clone()));
        create_user(&mutator, "Racing", "before").await;

        gate.armed.store(true, Ordering::SeqCst);
        let creator = db.clone();
        let index_task = workers::spawn(
            Some(crate::common::worker_io::pool()),
            "test",
            move || async move {
                let (fields, kind) = by_name();
                creator
                    .create_index("Racing", Some("by_name"), fields, kind)
                    .await
            },
        )
        .unwrap();

        // The index definition has been staged, but its transaction has not
        // published the new collection version or backfill action yet.
        gate.entered.notified().await;
        create_user(&mutator, "Racing", "during-definition-commit").await;
        gate.release.notify_one();

        let index = index_task.await.unwrap().unwrap();
        assert_eq!(
            entries_for(&db, "Racing", "during-definition-commit").await,
            1,
            "a pre-publication write above the fence must be included in backfill"
        );
        assert_eq!(index_entries(&db, "Racing", index.id).await, 2);
    })
    .unwrap()
}

#[test]
fn create_index_fence_covers_other_handles_reserved_id_range() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let index_db = Arc::new(DB::from_arc(store.clone()).unwrap());
        index_db
            .create_collection(users_schema("Racing", "racing-v1", "col-index-handles"))
            .await
            .unwrap();

        // Keep an early range open on the index creator. A later handle reserves
        // and uses a higher range, which the creator's local `peek` cannot see.
        index_db.next_doc_short_id().await.unwrap();
        let writer_db = Arc::new(DB::open_from_arc(store).await.unwrap());
        let writer = AutoCommitMutator::new(writer_db);
        create_user(&writer, "Racing", "other-handle-document").await;

        let (fields, kind) = by_name();
        let index = index_db
            .create_index("Racing", Some("by_name"), fields, kind)
            .await
            .unwrap();

        assert_eq!(
            entries_for(&index_db, "Racing", "other-handle-document").await,
            1
        );
        assert_eq!(index_entries(&index_db, "Racing", index.id).await, 1);
    })
    .unwrap()
}

#[derive(Default)]
struct DefinitionCommitGate {
    armed: AtomicBool,
    entered: regolith::sync::Notify,
    release: regolith::sync::Notify,
}

struct PauseDefinitionCommitStore {
    inner: Arc<RegolithStore>,
    gate: Arc<DefinitionCommitGate>,
}

impl storage::corekv::private::Sealed for PauseDefinitionCommitStore {}

#[async_trait]
impl Store for PauseDefinitionCommitStore {
    fn crdts(&self) -> &storage::Crdts {
        self.inner.crdts()
    }

    async fn allocate(&self, key: &[u8], count: u64) -> StorageResult<std::ops::Range<u64>> {
        self.inner.allocate(key, count).await
    }

    async fn new_txn(&self, readonly: bool) -> StorageResult<Box<dyn Txn>> {
        Ok(Box::new(PauseDefinitionCommitTxn {
            inner: self.inner.new_txn(readonly).await?,
            gate: Arc::clone(&self.gate),
        }))
    }

    async fn transact_retry(
        &self,
        options: storage::corekv::RetryOptions,
        attempt: storage::corekv::RetryAttempt,
    ) -> std::result::Result<storage::corekv::RetryAttemptValue, storage::corekv::RetryAttemptError>
    {
        self.inner.transact_retry(options, attempt).await
    }

    async fn close(&self) -> StorageResult<()> {
        self.inner.close().await
    }
}

struct PauseDefinitionCommitTxn {
    inner: Box<dyn Txn>,
    gate: Arc<DefinitionCommitGate>,
}

impl storage::corekv::private::Sealed for PauseDefinitionCommitTxn {}

#[async_trait]
impl Reader for PauseDefinitionCommitTxn {
    async fn get(&self, key: &[u8]) -> StorageResult<Option<bytes::Bytes>> {
        self.inner.get(key).await
    }

    async fn get_parts(&self, key: &[u8], parts: &[u32]) -> StorageResult<Option<bytes::Bytes>> {
        self.inner.get_parts(key, parts).await
    }

    async fn has(&self, key: &[u8]) -> StorageResult<bool> {
        self.inner.has(key).await
    }

    async fn get_size(&self, key: &[u8]) -> StorageResult<Option<usize>> {
        self.inner.get_size(key).await
    }

    async fn iterator(
        &self,
        opts: IterOptions,
    ) -> StorageResult<Box<dyn storage::corekv::Iterator>> {
        self.inner.iterator(opts).await
    }
}

#[async_trait]
impl Writer for PauseDefinitionCommitTxn {
    async fn merge_shared(&self, key: &[u8], operand: &[u8]) -> StorageResult<()> {
        self.inner.merge_shared(key, operand).await
    }

    async fn append_shared(
        &self,
        log: &storage::corekv::AppendLog,
        entry: &[u8],
        once_key: Option<&[u8]>,
    ) -> StorageResult<()> {
        self.inner.append_shared(log, entry, once_key).await
    }

    async fn set_shared(&self, key: &[u8], value: &[u8]) -> StorageResult<()> {
        self.inner.set_shared(key, value).await
    }

    async fn delete_shared(&self, key: &[u8]) -> StorageResult<()> {
        self.inner.delete_shared(key).await
    }
}

#[async_trait]
impl Txn for PauseDefinitionCommitTxn {
    async fn commit(self: Box<Self>) -> StorageResult<()> {
        if !self.inner.is_readonly() && self.gate.armed.swap(false, Ordering::SeqCst) {
            self.gate.entered.notify_one();
            self.gate.release.notified().await;
        }
        self.inner.commit().await
    }

    fn discard(self: Box<Self>) {
        self.inner.discard()
    }

    fn on_commit(&mut self, callback: storage::corekv::OnCommit) -> StorageResult<()> {
        self.inner.on_commit(callback)
    }

    fn on_abort(&mut self, callback: storage::corekv::OnAbort) {
        self.inner.on_abort(callback)
    }

    fn before_commit(&mut self, callback: storage::corekv::BeforeCommit) {
        self.inner.before_commit(callback)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn is_readonly(&self) -> bool {
        self.inner.is_readonly()
    }

    fn callback_count(&self) -> usize {
        self.inner.callback_count()
    }
}

/// The batch scanned the document and observed it, so the commit sees the
/// update that landed in between and refuses rather than keep an entry for
/// a value the document no longer has.
#[test]
fn backfill_batch_aborts_when_a_document_it_read_changes() {
    workers::block_on(async {
        let (_store, db) =
            open_with_collection("Observed", "observed-v1", "col-index-observed").await;
        let mutator = AutoCommitMutator::new(db.clone());
        let mut ids = Vec::new();
        for name in ["ada", "grace", "linus"] {
            ids.push(create_user(&mutator, "Observed", name).await);
        }
        let (fields, kind) = by_name();
        db.create_index("Observed", Some("by_name"), fields, kind)
            .await
            .unwrap();

        let collection = db.require_collection("Observed").unwrap();
        let manager = IndexManager::from_collection(
            collection.schema().resolved_root_id(),
            collection.schema(),
            &crdt::Crdts::default(),
        )
        .unwrap();
        let txn = db.new_txn(false).await.unwrap();
        {
            let datastore = txn.datastore().unwrap();
            let systemstore = txn.systemstore().unwrap();
            let mut source = BackfillSource::open_range(
                collection.clone(),
                datastore.clone(),
                systemstore.clone(),
                None,
                None,
            )
            .await
            .unwrap();
            let batch = manager
                .index_batch_from(
                    &datastore,
                    "by_name",
                    &mut source,
                    collection.schema(),
                    usize::MAX,
                )
                .await
                .unwrap();
            assert_eq!(batch.indexed, 3);
            assert!(batch.exhausted);
        }

        let mut doc = Document::new(users_schema("Observed", "v1", "col-observed"));
        doc.set_id(ids[1].clone());
        doc.set("name", NormalValue::String("grace-hopper".to_string()))
            .unwrap();
        mutator
            .update(
                "Observed",
                doc,
                RapidHashSet::from_iter(["name".to_string()]),
            )
            .await
            .unwrap();

        let error = txn
            .commit()
            .await
            .expect_err("the batch read a document that changed underneath");
        assert!(error.is_txn_conflict(), "{error}");
        assert_eq!(entries_for(&db, "Observed", "grace").await, 0);
        assert_eq!(entries_for(&db, "Observed", "grace-hopper").await, 1);
    })
    .unwrap()
}

/// The definition, its in-progress action and one committed batch survive
/// the process; opening the store again finishes the backfill from the
/// batch after the durable one and clears the action.
#[test]
fn index_backfill_resumes_after_a_restart() {
    workers::block_on(async {
        const COLLECTION_ID: &str = "col-index-resumed";
        let total = 300;
        let first_batch = 100;
        let (store, db) = open_with_collection("Resumed", "resumed-v1", COLLECTION_ID).await;
        let mutator = AutoCommitMutator::new(db.clone());
        for i in 0..total {
            create_user(&mutator, "Resumed", &format!("user-{i}")).await;
        }
        drop(mutator);

        let collection = db.require_collection("Resumed").unwrap();
        let mut schema = collection.schema().clone();
        let (fields, kind) = by_name();
        let index = {
            let txn = db.new_txn(false).await.unwrap();
            let index = {
                let datastore = txn.datastore().unwrap();
                let systemstore = txn.systemstore().unwrap();
                let mut manager = IndexManager::from_collection(
                    schema.resolved_root_id(),
                    &schema,
                    &crdt::Crdts::default(),
                )
                .unwrap();
                let index = manager
                    .create_index_of_kind(
                        &datastore,
                        "Resumed",
                        "by_name".to_string(),
                        fields,
                        kind,
                    )
                    .await
                    .unwrap();
                schema.indexes.push(index.clone());
                systemstore
                    .set(
                        &CollectionKey::new(&schema.version_id).bytes(),
                        &serde_json::to_vec(&schema).unwrap(),
                    )
                    .await
                    .unwrap();
                systemstore
                    .set(
                        &CollectionNameKey::new("Resumed").bytes(),
                        schema.version_id.as_bytes(),
                    )
                    .await
                    .unwrap();
                let lease = db
                    .stage_action(
                        &systemstore,
                        COLLECTION_ID,
                        defra_core::Action::BACKFILL_INDEX,
                        &index.id.to_string(),
                    )
                    .await
                    .unwrap();
                drop(lease);
                index
            };
            txn.commit().await.unwrap();
            index
        };
        // Every existing document sits below the fence; the id itself is spent.
        let fence = db.next_doc_short_id().await.unwrap();
        let progress_key = ActionProgressKey::new(
            COLLECTION_ID,
            defra_core::Action::BACKFILL_INDEX,
            index.id.to_string(),
        )
        .bytes();

        let indexed = Collection::new(schema.clone(), crdt::Crdts::default());
        let manager = IndexManager::from_collection(
            schema.resolved_root_id(),
            &schema,
            &crdt::Crdts::default(),
        )
        .unwrap();
        let txn = db.new_txn(false).await.unwrap();
        {
            let datastore = txn.datastore().unwrap();
            let systemstore = txn.systemstore().unwrap();
            let mut source = BackfillSource::open_range(
                indexed.clone(),
                datastore.clone(),
                systemstore.clone(),
                None,
                Some(fence),
            )
            .await
            .unwrap();
            let batch = manager
                .index_batch_from(&datastore, "by_name", &mut source, &schema, first_batch)
                .await
                .unwrap();
            assert_eq!(batch.indexed, first_batch);
            assert!(!batch.exhausted);
            systemstore
                .set(
                    &progress_key,
                    &encode_progress(fence, batch.last_doc_short_id.unwrap()),
                )
                .await
                .unwrap();
        }
        txn.commit().await.unwrap();
        assert_eq!(index_entries(&db, "Resumed", index.id).await, first_batch);
        drop(db);

        let db = DB::open_from_arc(store).await.unwrap();
        assert_eq!(index_entries(&db, "Resumed", index.id).await, total);
        assert_eq!(entries_for(&db, "Resumed", "user-0").await, 1);
        assert_eq!(entries_for(&db, "Resumed", "user-299").await, 1);
        assert!(!db
            .list_index_actions(COLLECTION_ID)
            .await
            .unwrap()
            .contains_key(&index.id));
        assert!(!progress_recorded(&db, COLLECTION_ID, index.id).await);
        assert!(db
            .require_collection("Resumed")
            .unwrap()
            .get_indexes()
            .iter()
            .any(|existing| existing.name == "by_name"));
    })
    .unwrap()
}

#[test]
fn ensure_ordinary_index_is_idempotent_and_preserves_collection_identity() {
    workers::block_on(async {
        let (_store, db) =
            open_with_collection("Reconciled", "reconciled-v1", "col-reconciled").await;
        let mutator = AutoCommitMutator::new(Arc::clone(&db));
        let doc = create_user(&mutator, "Reconciled", "Ada").await;
        let before = db
            .require_collection("Reconciled")
            .unwrap()
            .schema()
            .clone();
        let (fields, _) = by_name();
        let first = db
            .ensure_ordinary_index("Reconciled", "by_name", fields.clone())
            .await
            .unwrap();
        let second = db
            .ensure_ordinary_index("Reconciled", "by_name", fields)
            .await
            .unwrap();
        assert_eq!(first, second);
        let after = db
            .require_collection("Reconciled")
            .unwrap()
            .schema()
            .clone();
        assert_eq!(before.version_id, after.version_id);
        assert_eq!(before.collection_id, after.collection_id);
        assert_eq!(before.fields, after.fields);
        assert_eq!(index_entries(&db, "Reconciled", first.id).await, 1);
        assert_eq!(entries_for(&db, "Reconciled", "Ada").await, 1);
        let stored = mutator
            .get_for_update("Reconciled", &doc)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.get("name"), Some(&NormalValue::String("Ada".into())));
        let mut wrong = first.fields;
        wrong[0].descending = true;
        assert!(db
            .ensure_ordinary_index("Reconciled", "by_name", wrong)
            .await
            .is_err());
    })
    .unwrap()
}

#[test]
fn ensure_ordinary_index_reports_existing_backfill_and_failure() {
    workers::block_on(async {
        let (_store, db) = open_with_collection("Readiness", "readiness-v1", "col-readiness").await;
        let (fields, _) = by_name();
        let index = db
            .ensure_ordinary_index("Readiness", "by_name", fields.clone())
            .await
            .unwrap();
        let lease = db
            .register_action_with_subject(
                "col-readiness",
                defra_core::Action::BACKFILL_INDEX,
                &index.id.to_string(),
            )
            .await
            .unwrap();
        assert!(matches!(
            db.ensure_ordinary_index("Readiness", "by_name", fields.clone())
                .await,
            Err(db::Error::ActionInProgress { .. })
        ));
        db.fail_action(lease, "backfill failed").await.unwrap();
        let error = db
            .ensure_ordinary_index("Readiness", "by_name", fields)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("backfill failed"));
    })
    .unwrap()
}

#[test]
fn concurrent_ordinary_index_ensure_creates_one_definition() {
    workers::block_on(async {
        let (_store, db) =
            open_with_collection("ConcurrentEnsure", "concurrent-v1", "col-concurrent").await;
        let (fields, _) = by_name();
        let (left, right) = futures::join!(
            db.ensure_ordinary_index("ConcurrentEnsure", "by_name", fields.clone()),
            db.ensure_ordinary_index("ConcurrentEnsure", "by_name", fields.clone()),
        );
        let mut ready = Vec::new();
        for outcome in [left, right] {
            match outcome {
                Ok(index) => ready.push(index),
                Err(db::Error::ActionInProgress { .. }) => {}
                Err(other) => panic!("unexpected ensure failure: {other}"),
            }
        }
        assert!(!ready.is_empty());
        let final_index = db
            .ensure_ordinary_index("ConcurrentEnsure", "by_name", fields)
            .await
            .unwrap();
        assert!(ready.iter().all(|index| index == &final_index));
        assert_eq!(
            db.require_collection("ConcurrentEnsure")
                .unwrap()
                .schema()
                .indexes
                .iter()
                .filter(|index| index.name == "by_name")
                .count(),
            1
        );
    })
    .unwrap()
}
