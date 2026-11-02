use crate::common::schema::users_schema;
use async_trait::async_trait;
use bytes::Bytes;
use db::AutoCommitMutator;
use db::DB;
use futures::StreamExt;
use kovan::AtomOption;
use kovan_map::HopscotchMap;
use lens::LensConfig;
use lens::LensDocResultStream;
use lens::LensDocStream;
use lens::LensModule;
use lens::TransformId;
use lens::TransformStore;
use query::txn::TransactionRegistry;
use query::DocFetcher;
use query::DocMutator;
use query::QueryExecutor;
use query::QueryRequest;
use rapidhash::fast::RandomState;
use rapidhash::RapidHashSet;
use rapidhash::{HashMapExt, RapidHashMap};
use regolith::sync::Notify;
use schema::CollectionVersion;
use schema::FieldDescription;
use schema::FieldKind;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use storage::corekv::IterOptions;
use storage::index::IndexIterator;
use storage::RegolithStore;

mod json_frontier;
mod paged_reindex;
mod text_lens;
mod unique_conflicts;

#[derive(Default)]
struct SetVerifiedStore {
    transforms: TransformIdSet,
    transform_calls: AtomicUsize,
    /// The transform call (counting from one) that fails, as a lens does
    /// when it cannot run; zero for none.
    fail_call: AtomicUsize,
    verified_value: Option<serde_json::Value>,
    before_transform: AtomOption<Box<dyn Fn() + Send + Sync>>,
}

/// The set of transform ids a test double has registered.
struct TransformIdSet(HopscotchMap<TransformId, (), RandomState>);

impl Default for TransformIdSet {
    fn default() -> Self {
        Self(HopscotchMap::with_hasher(RandomState::default()))
    }
}

impl TransformIdSet {
    fn insert(&self, id: TransformId) {
        self.0.insert(id, ());
    }

    fn contains(&self, id: &TransformId) -> bool {
        self.0.contains_key(id)
    }

    fn remove(&self, id: &TransformId) {
        self.0.remove(id);
    }
}

#[async_trait]
impl TransformStore for SetVerifiedStore {
    fn new_isolated_store(&self) -> Arc<dyn TransformStore> {
        Arc::new(Self::default())
    }

    async fn add(&self, config: LensConfig) -> lens::Result<TransformId> {
        let id = TransformId::new(format!("test-transform-{}", config.lenses.len()));
        self.transforms.insert(id.clone());
        Ok(id)
    }

    async fn add_with_id(&self, id: TransformId, _config: LensConfig) -> lens::Result<()> {
        self.transforms.insert(id);
        Ok(())
    }

    async fn list(&self) -> lens::Result<RapidHashMap<String, Vec<LensModule>>> {
        Ok(RapidHashMap::new())
    }

    fn transform(
        &self,
        id: &TransformId,
        docs: LensDocStream,
    ) -> lens::Result<LensDocResultStream> {
        if !self.has_transform(id) {
            return Err(lens::Error::TransformNotFound(id.to_string()));
        }
        let call = self.transform_calls.fetch_add(1, Ordering::SeqCst) + 1;
        if self.fail_call.load(Ordering::SeqCst) == call {
            return Err(lens::Error::Pipeline("the lens cannot run".to_string()));
        }
        if let Some(hook) = self.before_transform.take() {
            (*hook)();
        }
        let value = self
            .verified_value
            .clone()
            .unwrap_or(serde_json::Value::Bool(true));
        Ok(Box::pin(docs.map(move |mut doc| {
            doc.insert("verified".to_string(), value.clone());
            Ok(doc)
        })))
    }

    fn inverse(&self, id: &TransformId, docs: LensDocStream) -> lens::Result<LensDocResultStream> {
        self.transform(id, docs)
    }

    fn has_transform(&self, id: &TransformId) -> bool {
        self.transforms.contains(id)
    }

    async fn remove(&self, id: &TransformId) -> lens::Result<()> {
        self.transforms.remove(id);
        Ok(())
    }
}

#[derive(Default)]
struct BlockingVerifiedStore {
    transforms: TransformIdSet,
    transform_calls: AtomicUsize,
    block_on_call: AtomicUsize,
    entered: Arc<Notify>,
    release: Arc<Notify>,
}

impl BlockingVerifiedStore {
    fn arm(&self) {
        self.arm_on_call(self.transform_calls.load(Ordering::SeqCst) + 1);
    }

    fn arm_on_call(&self, call: usize) {
        self.block_on_call.store(call, Ordering::SeqCst);
    }
}

#[async_trait]
impl TransformStore for BlockingVerifiedStore {
    fn new_isolated_store(&self) -> Arc<dyn TransformStore> {
        Arc::new(Self::default())
    }

    async fn add(&self, config: LensConfig) -> lens::Result<TransformId> {
        let id = TransformId::new(format!("blocking-transform-{}", config.lenses.len()));
        self.transforms.insert(id.clone());
        Ok(id)
    }

    async fn add_with_id(&self, id: TransformId, _config: LensConfig) -> lens::Result<()> {
        self.transforms.insert(id);
        Ok(())
    }

    async fn list(&self) -> lens::Result<RapidHashMap<String, Vec<LensModule>>> {
        Ok(RapidHashMap::new())
    }

    fn transform(
        &self,
        id: &TransformId,
        docs: LensDocStream,
    ) -> lens::Result<LensDocResultStream> {
        if !self.has_transform(id) {
            return Err(lens::Error::TransformNotFound(id.to_string()));
        }
        let call = self.transform_calls.fetch_add(1, Ordering::SeqCst) + 1;
        let should_block = self.block_on_call.load(Ordering::SeqCst) == call;
        let entered = self.entered.clone();
        let release = self.release.clone();
        Ok(Box::pin(docs.then(move |mut doc| {
            let entered = entered.clone();
            let release = release.clone();
            async move {
                if should_block {
                    entered.notify_one();
                    release.notified().await;
                }
                doc.insert("verified".to_string(), serde_json::Value::Bool(true));
                Ok(doc)
            }
        })))
    }

    fn inverse(&self, id: &TransformId, docs: LensDocStream) -> lens::Result<LensDocResultStream> {
        self.transform(id, docs)
    }

    fn has_transform(&self, id: &TransformId) -> bool {
        self.transforms.contains(id)
    }

    async fn remove(&self, id: &TransformId) -> lens::Result<()> {
        self.transforms.remove(id);
        Ok(())
    }
}

struct StepTransformStore {
    next_id: AtomicUsize,
    fields: HopscotchMap<TransformId, String, RandomState>,
}

impl Default for StepTransformStore {
    fn default() -> Self {
        Self {
            next_id: AtomicUsize::new(0),
            fields: HopscotchMap::with_hasher(RandomState::default()),
        }
    }
}

#[async_trait]
impl TransformStore for StepTransformStore {
    fn new_isolated_store(&self) -> Arc<dyn TransformStore> {
        Arc::new(Self::default())
    }

    async fn add(&self, _config: LensConfig) -> lens::Result<TransformId> {
        let sequence = self.next_id.fetch_add(1, Ordering::SeqCst);
        let id = TransformId::new(format!("step-transform-{sequence}"));
        let field = if sequence == 0 { "latest" } else { "middle" };
        self.fields.insert(id.clone(), field.to_string());
        Ok(id)
    }

    async fn add_with_id(&self, id: TransformId, _config: LensConfig) -> lens::Result<()> {
        self.fields.insert(id, "restored".to_string());
        Ok(())
    }

    async fn list(&self) -> lens::Result<RapidHashMap<String, Vec<LensModule>>> {
        Ok(RapidHashMap::new())
    }

    fn transform(
        &self,
        id: &TransformId,
        docs: LensDocStream,
    ) -> lens::Result<LensDocResultStream> {
        let field = self
            .fields
            .get(id)
            .ok_or_else(|| lens::Error::TransformNotFound(id.to_string()))?;
        Ok(Box::pin(docs.map(move |mut doc| {
            doc.insert(field.clone(), serde_json::Value::String("set".to_string()));
            Ok(doc)
        })))
    }

    fn inverse(&self, id: &TransformId, docs: LensDocStream) -> lens::Result<LensDocResultStream> {
        self.transform(id, docs)
    }

    fn has_transform(&self, id: &TransformId) -> bool {
        self.fields.contains_key(id)
    }

    async fn remove(&self, id: &TransformId) -> lens::Result<()> {
        self.fields.remove(id);
        Ok(())
    }
}

fn indexed_users_schema() -> CollectionVersion {
    let mut schema = CollectionVersion::new(
        "Users",
        "users-v1",
        "users-collection",
        vec![
            FieldDescription::new(
                "1",
                "_docID",
                FieldKind::doc_id(),
                schema::CType::LwwRegister,
            ),
            FieldDescription::new("2", "name", FieldKind::string(), schema::CType::LwwRegister),
            FieldDescription::new(
                "3",
                "verified",
                FieldKind::bool(),
                schema::CType::LwwRegister,
            ),
        ],
    );
    schema.indexes =
        vec![schema::IndexDescription::new("idx_verified").with_field("verified", false)];
    schema.is_materialized = true;
    schema
}

async fn seed_user(db: &Arc<DB<RegolithStore>>) -> document::DocID {
    let users = db.get_collection("Users").unwrap().unwrap().schema_arc();
    let mut doc = document::Document::new(users);
    doc.set("name", document::NormalValue::String("Alice".to_string()))
        .unwrap();
    AutoCommitMutator::new(db.clone())
        .create("Users", doc)
        .await
        .unwrap()
        .doc_id
}

async fn add_verified_version(db: &Arc<DB<RegolithStore>>) -> String {
    db.patch_collection(
        "Users",
        r#"[{"op":"add","path":"/Users/Fields/-","value":{"Name":"verified","Kind":"Boolean"}}]"#,
        None,
    )
    .await
    .unwrap()
    .version_id
}

async fn add_placeholder_version(db: &Arc<DB<RegolithStore>>, field_name: &str) -> String {
    db.patch_collection(
        "Users",
        &format!(
            r#"[{{"op":"add","path":"/Users/Fields/-","value":{{"Name":"{field_name}","Kind":"String"}}}}]"#
        ),
        None,
    )
    .await
    .unwrap()
    .version_id
}

async fn seed_old_user(
    db: &Arc<DB<RegolithStore>>,
    version_id: &str,
    name: &str,
) -> document::DocID {
    let collection = db.get_collection("Users").unwrap().unwrap();
    let index_manager = db::index::IndexManager::from_collection(
        collection.resolved_root_id(),
        collection.schema(),
    )
    .unwrap();
    let mut doc = document::Document::new(collection.schema_arc());
    let seed = format!("late-{name}");
    let doc_id = document::DocID::new_v0_from_seed(&seed);
    doc.set_id(doc_id.clone());
    doc.set("name", document::NormalValue::String(name.to_string()))
        .unwrap();
    doc.set_collection_version_id(version_id);

    let txn = db.new_txn(false).await.unwrap();
    let datastore = txn.datastore().unwrap();
    let systemstore = txn.systemstore().unwrap();
    let doc_short_id = db::docid::map::next_doc_short_id(&systemstore)
        .await
        .unwrap();
    db::docid::map::set_doc_id_mapping(
        &systemstore,
        collection.resolved_root_id(),
        doc_short_id,
        &doc_id.to_string(),
    )
    .await
    .unwrap();
    collection
        .create_with_indexes(&datastore, &doc, doc_short_id, &index_manager)
        .await
        .unwrap();
    collection
        .save_with_datastore(&datastore, &doc, doc_short_id)
        .await
        .unwrap();
    drop(datastore);
    drop(systemstore);
    txn.commit().await.unwrap();
    doc_id
}

async fn verified_index_count(db: &Arc<DB<RegolithStore>>) -> usize {
    let collection = db.get_collection("Users").unwrap().unwrap();
    let index_manager = db::index::IndexManager::from_collection(
        collection.resolved_root_id(),
        collection.schema(),
    )
    .unwrap();
    let index = index_manager.get_index("idx_verified").unwrap();
    let txn = db.new_txn(true).await.unwrap();
    let datastore = txn.datastore().unwrap();
    let mut iter = index
        .get(&datastore, &[document::NormalValue::Bool(true)])
        .await
        .unwrap();
    let entries = iter.collect_all().await.unwrap();
    drop(datastore);
    txn.discard().unwrap();
    entries.len()
}

async fn load_user(db: &Arc<DB<RegolithStore>>, doc_id: &document::DocID) -> document::Document {
    let collection = db.get_collection("Users").unwrap().unwrap();
    let txn = db.new_txn(true).await.unwrap();
    let doc = collection
        .get_by_doc_id(
            &txn.datastore().unwrap(),
            &txn.systemstore().unwrap(),
            doc_id,
        )
        .await
        .unwrap()
        .unwrap();
    txn.discard().unwrap();
    doc
}

async fn load_user_blob(db: &Arc<DB<RegolithStore>>, doc_id: &document::DocID) -> Bytes {
    let collection = db.get_collection("Users").unwrap().unwrap();
    let txn = db.new_txn(true).await.unwrap();
    let datastore = txn.datastore().unwrap();
    let systemstore = txn.systemstore().unwrap();
    let doc_short_id = collection
        .resolve_doc_short_id(&systemstore, doc_id)
        .await
        .unwrap()
        .unwrap();
    let blob = datastore
        .get(&collection.doc_key(doc_short_id))
        .await
        .unwrap()
        .unwrap();
    drop(datastore);
    drop(systemstore);
    txn.discard().unwrap();
    blob
}

async fn namespace_counts(db: &Arc<DB<RegolithStore>>) -> (usize, usize, usize) {
    async fn count(view: datastore::NamespaceView) -> usize {
        let mut iter = view.iterator(IterOptions::new()).await.unwrap();
        let mut count = 0;
        while iter.next().await.unwrap().is_some() {
            count += 1;
        }
        iter.close().await.unwrap();
        count
    }

    let txn = db.new_txn(true).await.unwrap();
    let counts = (
        count(txn.blockstore().unwrap()).await,
        count(txn.headstore().unwrap()).await,
        count(txn.peerstore().unwrap()).await,
    );
    txn.discard().unwrap();
    counts
}

#[test]
fn lazy_lensed_read_leaves_storage_unchanged() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let transform_store = Arc::new(SetVerifiedStore::default());
        let mut raw_db = DB::from_arc(store.clone()).unwrap();
        raw_db.set_lens_store(transform_store.clone());
        let db = Arc::new(raw_db);

        db.create_collection(users_schema()).await.unwrap();
        let v1 = db
            .get_collection("Users")
            .unwrap()
            .unwrap()
            .version_id()
            .to_string();
        let doc_id = seed_user(&db).await;
        let v2 = add_verified_version(&db).await;
        let fetcher = db::LensedAutoCommitFetcher::new(db.clone());
        let before_migration = fetcher.get_all("Users").await.unwrap();
        assert_eq!(before_migration[0].get("verified"), None);

        db.set_migration(
            LensConfig::new(
                &v1,
                &v2,
                LensModule::from_bytes(b"\0asm\x01\0\0\0".to_vec()),
            ),
            None,
        )
        .await
        .unwrap();

        let before_counts = namespace_counts(&db).await;
        let docs = fetcher.get_all("Users").await.unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(
            docs[0].get("verified"),
            Some(&document::NormalValue::Bool(true))
        );
        assert_eq!(docs[0].collection_version_id(), Some(v2.as_str()));
        assert_eq!(transform_store.transform_calls.load(Ordering::SeqCst), 1);

        let persisted = load_user(&db, &doc_id).await;
        assert_eq!(persisted.get("verified"), None);
        assert_eq!(persisted.collection_version_id(), Some(v1.as_str()));
        assert_eq!(namespace_counts(&db).await, before_counts);

        fetcher.get_all("Users").await.unwrap();
        assert_eq!(
            transform_store.transform_calls.load(Ordering::SeqCst),
            2,
            "read-only transformations do not persist a cached version"
        );

        drop(fetcher);
        drop(db);
        let reopened = Arc::new(DB::open_from_arc(store).await.unwrap());
        let persisted_after_restart = load_user(&reopened, &doc_id).await;
        assert_eq!(persisted_after_restart.get("verified"), None);
        assert_eq!(
            persisted_after_restart.collection_version_id(),
            Some(v1.as_str())
        );
    })
    .unwrap()
}

#[test]
fn eager_materialize_restamps_transformless_paths() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let db = Arc::new(DB::from_arc(store).unwrap());

        db.create_collection(users_schema()).await.unwrap();
        let doc_id = seed_user(&db).await;
        let v2 = add_verified_version(&db).await;
        assert_ne!(
            load_user(&db, &doc_id).await.collection_version_id(),
            Some(v2.as_str())
        );

        let before_counts = namespace_counts(&db).await;
        let before_blob = load_user_blob(&db, &doc_id).await;
        assert_eq!(db.materialize_collection("Users").await.unwrap(), 1);
        assert_eq!(
            load_user(&db, &doc_id).await.collection_version_id(),
            Some(v2.as_str())
        );
        assert_ne!(
            load_user_blob(&db, &doc_id).await,
            before_blob,
            "materialization must encode the active version's part IDs"
        );
        assert_eq!(db.materialize_collection("Users").await.unwrap(), 0);
        assert_eq!(namespace_counts(&db).await, before_counts);
        let users = db.get_collection("Users").unwrap().unwrap().schema_arc();
        let mut update = document::Document::new(users);
        update.set_id(doc_id.clone());
        update
            .set("name", document::NormalValue::String("Bob".into()))
            .unwrap();
        update
            .set("verified", document::NormalValue::Bool(true))
            .unwrap();
        AutoCommitMutator::new(db.clone())
            .update(
                "Users",
                update,
                RapidHashSet::from_iter(["name".to_string(), "verified".to_string()]),
            )
            .await
            .unwrap();
        let updated = load_user(&db, &doc_id).await;
        assert_eq!(
            updated.get("name"),
            Some(&document::NormalValue::String("Bob".into()))
        );
        assert_eq!(
            updated.get("verified"),
            Some(&document::NormalValue::Bool(true))
        );
    })
    .unwrap()
}

#[test]
fn migration_context_cache_invalidates_when_graph_changes_without_version_change() {
    workers::block_on(async {
        let transform_store = Arc::new(StepTransformStore::default());
        let mut raw_db = DB::new(RegolithStore::in_memory().unwrap()).unwrap();
        raw_db.set_lens_store(transform_store);
        let db = Arc::new(raw_db);

        db.create_collection(users_schema()).await.unwrap();
        let v1 = db
            .get_collection("Users")
            .unwrap()
            .unwrap()
            .version_id()
            .to_string();
        let v2 = add_placeholder_version(&db, "middle").await;
        let v3 = add_placeholder_version(&db, "latest").await;

        db.set_migration(
            LensConfig::new(
                &v2,
                &v3,
                LensModule::from_bytes(b"\0asm\x01\0\0\0".to_vec()),
            ),
            None,
        )
        .await
        .unwrap();

        let fetcher = db::LensedAutoCommitFetcher::new(db.clone());
        assert!(fetcher.get_all("Users").await.unwrap().is_empty());

        db.set_migration(
            LensConfig::new(
                &v1,
                &v2,
                LensModule::from_bytes(b"\0asm\x01\0\0\0".to_vec()),
            ),
            None,
        )
        .await
        .unwrap();

        let doc_id = seed_old_user(&db, &v1, "CachedGraph").await;
        let docs = fetcher
            .get_by_ids("Users", &[doc_id.to_string()])
            .await
            .unwrap()
            .into_docs();
        assert_eq!(
            docs[0].get("middle"),
            Some(&document::NormalValue::String("set".to_string())),
            "the newly registered v1→v2 transform must be present in refreshed history"
        );
        assert_eq!(
            docs[0].get("latest"),
            Some(&document::NormalValue::String("set".to_string())),
            "the previously cached v2→v3 transform must remain in the complete history"
        );
    })
    .unwrap()
}

#[test]
fn lazy_read_leaves_secondary_indexes_unchanged() {
    workers::block_on(async {
        let transform_store = Arc::new(SetVerifiedStore::default());
        let mut raw_db = DB::new(RegolithStore::in_memory().unwrap()).unwrap();
        raw_db.set_lens_store(transform_store);
        let db = Arc::new(raw_db);

        db.create_collection(indexed_users_schema()).await.unwrap();
        let v1 = db
            .get_collection("Users")
            .unwrap()
            .unwrap()
            .version_id()
            .to_string();
        let v2 = add_placeholder_version(&db, "placeholder").await;
        db.set_migration(
            LensConfig::new(
                &v1,
                &v2,
                LensModule::from_bytes(b"\0asm\x01\0\0\0".to_vec()),
            ),
            None,
        )
        .await
        .unwrap();

        let doc_id = seed_old_user(&db, &v1, "Late").await;
        assert_eq!(verified_index_count(&db).await, 0);

        let fetcher = db::LensedAutoCommitFetcher::new(db.clone());
        let docs = fetcher
            .get_by_ids("Users", &[doc_id.to_string()])
            .await
            .unwrap()
            .into_docs();
        assert_eq!(
            docs[0].get("verified"),
            Some(&document::NormalValue::Bool(true))
        );
        assert_eq!(
            verified_index_count(&db).await,
            0,
            "read-only transformation must not replace index entries"
        );
    })
    .unwrap()
}

#[test]
fn lazy_full_scan_is_write_free_even_when_paused() {
    workers::block_on(async {
        let transform_store = Arc::new(BlockingVerifiedStore::default());
        let mut raw_db = DB::new(RegolithStore::in_memory().unwrap()).unwrap();
        raw_db.set_lens_store(transform_store.clone());
        let db = Arc::new(raw_db);

        db.create_collection(indexed_users_schema()).await.unwrap();
        let v1 = db
            .get_collection("Users")
            .unwrap()
            .unwrap()
            .version_id()
            .to_string();
        let v2 = add_placeholder_version(&db, "placeholder").await;
        db.set_migration(
            LensConfig::new(
                &v1,
                &v2,
                LensModule::from_bytes(b"\0asm\x01\0\0\0".to_vec()),
            ),
            None,
        )
        .await
        .unwrap();

        let mut doc_ids = Vec::new();
        for name in ["Batch-1", "Batch-2", "Batch-3"] {
            doc_ids.push(seed_old_user(&db, &v1, name).await);
        }

        transform_store.arm_on_call(3);
        let fetch_db = db.clone();
        let fetch = workers::spawn(
            Some(crate::common::worker_io::pool()),
            "test",
            move || async move {
                db::LensedAutoCommitFetcher::new(fetch_db)
                    .get_all("Users")
                    .await
            },
        )
        .unwrap();

        workers::timeout(
            std::time::Duration::from_secs(2),
            transform_store.entered.notified(),
        )
        .await
        .expect("third migration reached the transform");

        assert_eq!(
            verified_index_count(&db).await,
            0,
            "a partial read must not write index entries"
        );

        transform_store.release.notify_one();
        let docs = fetch.await.unwrap().unwrap();
        assert_eq!(docs.len(), 3);
        assert!(docs
            .iter()
            .all(|doc| doc.collection_version_id() == Some(v2.as_str())));
        assert_eq!(verified_index_count(&db).await, 0);
        for doc_id in doc_ids {
            assert_eq!(
                load_user(&db, &doc_id).await.collection_version_id(),
                Some(v1.as_str())
            );
        }
    })
    .unwrap()
}

#[test]
fn implicit_full_scan_transforms_without_writes() {
    workers::block_on(async {
        let transform_store = Arc::new(SetVerifiedStore::default());
        let mut raw_db = DB::new(RegolithStore::in_memory().unwrap()).unwrap();
        raw_db.set_lens_store(transform_store.clone());
        let db = Arc::new(raw_db);

        db.create_collection(indexed_users_schema()).await.unwrap();
        let v1 = db
            .get_collection("Users")
            .unwrap()
            .unwrap()
            .version_id()
            .to_string();
        let v2 = add_placeholder_version(&db, "placeholder").await;
        db.set_migration(
            LensConfig::new(
                &v1,
                &v2,
                LensModule::from_bytes(b"\0asm\x01\0\0\0".to_vec()),
            ),
            None,
        )
        .await
        .unwrap();

        let mut source_ids = Vec::new();
        for name in ["Deferred-1", "Deferred-2", "Deferred-3"] {
            source_ids.push(seed_old_user(&db, &v1, name).await);
        }

        let txn = db.new_txn(true).await.unwrap();
        let fetcher = db::LensedDocFetcher::new(db.clone(), txn, transform_store);
        let docs = fetcher.get_all("Users").await.unwrap();
        assert_eq!(docs.len(), 3);

        fetcher.take_txn().await.unwrap().discard().unwrap();
        for doc_id in source_ids {
            let stored = load_user(&db, &doc_id).await;
            assert_eq!(stored.collection_version_id(), Some(v1.as_str()));
            assert_eq!(stored.get("verified"), None);
        }
        assert_eq!(verified_index_count(&db).await, 0);
    })
    .unwrap()
}

#[test]
fn implicit_query_leaves_storage_unchanged_after_snapshot_closes() {
    workers::block_on(async {
        let transform_store = Arc::new(SetVerifiedStore::default());
        let mut raw_db = DB::new(RegolithStore::in_memory().unwrap()).unwrap();
        raw_db.set_lens_store(transform_store);
        let db = Arc::new(raw_db);

        db.create_collection(indexed_users_schema()).await.unwrap();
        let v1 = db
            .get_collection("Users")
            .unwrap()
            .unwrap()
            .version_id()
            .to_string();
        let v2 = add_placeholder_version(&db, "placeholder").await;
        db.set_migration(
            LensConfig::new(
                &v1,
                &v2,
                LensModule::from_bytes(b"\0asm\x01\0\0\0".to_vec()),
            ),
            None,
        )
        .await
        .unwrap();
        let doc_id = seed_old_user(&db, &v1, "Implicit").await;

        let registry = Arc::new(db::DbTransactionRegistry::new(db.clone()));

        // A user-managed read-only transaction remains side-effect free.
        let explicit = registry.begin(true).await.unwrap();
        let ids = [doc_id.to_string()];
        let explicit_docs = crate::common::owned::in_txn(&registry, &explicit, move |context| {
            Box::pin(async move {
                context
                    .doc_fetcher()
                    .get_by_ids("Users", &ids)
                    .await
                    .unwrap()
                    .into_docs()
            })
        })
        .await;
        assert_eq!(
            explicit_docs[0].get("verified"),
            Some(&document::NormalValue::Bool(true))
        );
        registry.discard(&explicit).await.unwrap();
        assert_eq!(verified_index_count(&db).await, 0);

        let fetcher = db::LensedAutoCommitFetcher::new(db.clone());
        let provider = db::DbCollectionProvider::new_arc(db.clone());
        let runner =
            query::QueryRunner::with_arc_registry_and_provider(fetcher, provider, registry);

        let response = runner
            .execute(QueryRequest::new(
                "query { Users { name verified } }".to_string(),
            ))
            .await;
        assert!(response.errors.is_empty(), "{:?}", response.errors);
        assert_eq!(verified_index_count(&db).await, 0);

        let indexed_response = runner
            .execute(QueryRequest::new(
                "query { Users(filter: {verified: {_eq: true}}) { name verified } }".to_string(),
            ))
            .await;
        assert!(
            indexed_response.errors.is_empty(),
            "{:?}",
            indexed_response.errors
        );
        assert_eq!(
            indexed_response.data,
            Some(serde_json::json!({
                "Users": [{"name": "Implicit", "verified": true}]
            }))
        );
        let ordered = runner
            .execute(QueryRequest::new(
                "query { Users(order: {verified: DESC}, limit: 1) { name verified } }".to_string(),
            ))
            .await;
        assert!(ordered.errors.is_empty(), "{:?}", ordered.errors);
        assert_eq!(ordered.data, indexed_response.data);
        assert_eq!(
            load_user(&db, &doc_id).await.collection_version_id(),
            Some(v1.as_str())
        );
    })
    .unwrap()
}

#[test]
fn field_value_fetch_filters_after_lens_transform() {
    workers::block_on(async {
        let transform_store = Arc::new(StepTransformStore::default());
        let mut raw_db = DB::new(RegolithStore::in_memory().unwrap()).unwrap();
        raw_db.set_lens_store(transform_store);
        let db = Arc::new(raw_db);

        db.create_collection(users_schema()).await.unwrap();
        let v1 = db
            .get_collection("Users")
            .unwrap()
            .unwrap()
            .version_id()
            .to_string();
        let v2 = add_placeholder_version(&db, "latest").await;
        db.set_migration(
            LensConfig::new(
                &v1,
                &v2,
                LensModule::from_bytes(b"\0asm\x01\0\0\0".to_vec()),
            ),
            None,
        )
        .await
        .unwrap();
        let doc_id = seed_old_user(&db, &v1, "Filter").await;

        let docs = db::LensedAutoCommitFetcher::new(db)
            .get_by_field_value("Users", "latest", "set")
            .await
            .unwrap();

        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].id(), Some(&doc_id));
    })
    .unwrap()
}

#[test]
fn concurrent_lazy_reads_of_same_document_are_idempotent() {
    workers::block_on(async {
        let transform_store = Arc::new(BlockingVerifiedStore::default());
        let mut raw_db = DB::new(RegolithStore::in_memory().unwrap()).unwrap();
        raw_db.set_lens_store(transform_store.clone());
        let db = Arc::new(raw_db);

        db.create_collection(indexed_users_schema()).await.unwrap();
        let v1 = db
            .get_collection("Users")
            .unwrap()
            .unwrap()
            .version_id()
            .to_string();
        let v2 = add_placeholder_version(&db, "placeholder").await;
        db.set_migration(
            LensConfig::new(
                &v1,
                &v2,
                LensModule::from_bytes(b"\0asm\x01\0\0\0".to_vec()),
            ),
            None,
        )
        .await
        .unwrap();
        let doc_id = seed_old_user(&db, &v1, "Concurrent").await;

        transform_store.arm();
        let first_db = db.clone();
        let first_doc_id = doc_id.to_string();
        let first = workers::spawn(
            Some(crate::common::worker_io::pool()),
            "test",
            move || async move {
                db::LensedAutoCommitFetcher::new(first_db)
                    .get_by_ids("Users", &[first_doc_id])
                    .await
            },
        )
        .unwrap();

        workers::timeout(
            std::time::Duration::from_secs(2),
            transform_store.entered.notified(),
        )
        .await
        .expect("first lensed read reached the transform");

        let second_docs = workers::timeout(
            std::time::Duration::from_secs(2),
            db::LensedAutoCommitFetcher::new(db.clone()).get_by_ids("Users", &[doc_id.to_string()]),
        )
        .await
        .expect("second lensed read completed while the first was paused")
        .unwrap()
        .into_docs();

        transform_store.release.notify_one();
        let first_docs = first.await.unwrap().unwrap().into_docs();

        for docs in [&first_docs, &second_docs] {
            assert_eq!(
                docs[0].get("verified"),
                Some(&document::NormalValue::Bool(true))
            );
        }

        let persisted = load_user(&db, &doc_id).await;
        assert_eq!(persisted.get("verified"), None);
        assert_eq!(persisted.collection_version_id(), Some(v1.as_str()));
        assert_eq!(
            verified_index_count(&db).await,
            0,
            "concurrent reads must leave the index unchanged"
        );
    })
    .unwrap()
}

#[test]
fn concurrent_update_wins_over_stale_lazy_write_back() {
    workers::block_on(async {
        let transform_store = Arc::new(BlockingVerifiedStore::default());
        let mut raw_db = DB::new(RegolithStore::in_memory().unwrap()).unwrap();
        raw_db.set_lens_store(transform_store.clone());
        let db = Arc::new(raw_db);

        db.create_collection(indexed_users_schema()).await.unwrap();
        let v1 = db
            .get_collection("Users")
            .unwrap()
            .unwrap()
            .version_id()
            .to_string();
        let v2 = add_placeholder_version(&db, "placeholder").await;
        db.set_migration(
            LensConfig::new(
                &v1,
                &v2,
                LensModule::from_bytes(b"\0asm\x01\0\0\0".to_vec()),
            ),
            None,
        )
        .await
        .unwrap();
        let doc_id = seed_old_user(&db, &v1, "Alice").await;

        transform_store.arm();
        let query_db = db.clone();
        let query_doc_id = doc_id.to_string();
        let query = workers::spawn(
            Some(crate::common::worker_io::pool()),
            "test",
            move || async move {
                db::LensedAutoCommitFetcher::new(query_db)
                    .get_by_ids("Users", &[query_doc_id])
                    .await
            },
        )
        .unwrap();

        workers::timeout(
            std::time::Duration::from_secs(2),
            transform_store.entered.notified(),
        )
        .await
        .expect("lensed read reached the transform");

        let users = db.get_collection("Users").unwrap().unwrap().schema_arc();
        let mut update = document::Document::new(users);
        update.set_id(doc_id.clone());
        update
            .set("name", document::NormalValue::String("Bob".to_string()))
            .unwrap();
        AutoCommitMutator::new(db.clone())
            .update(
                "Users",
                update,
                RapidHashSet::from_iter(["name".to_string()]),
            )
            .await
            .unwrap();

        transform_store.release.notify_one();
        query.await.unwrap().unwrap();

        let persisted = load_user(&db, &doc_id).await;
        assert_eq!(
            persisted.get("name"),
            Some(&document::NormalValue::String("Bob".to_string())),
            "lazy write-back must not overwrite a mutation committed after its read snapshot"
        );
        assert_eq!(
            persisted.get("verified"),
            Some(&document::NormalValue::Bool(true))
        );
        assert_eq!(persisted.collection_version_id(), Some(v2.as_str()));
    })
    .unwrap()
}

/// Streaming transformations leave stored documents and indexes unchanged.
#[test]
fn streaming_lensed_read_keeps_source_documents_unchanged() {
    workers::block_on(async {
        let transform_store = Arc::new(SetVerifiedStore::default());
        let mut raw_db = DB::new(RegolithStore::in_memory().unwrap()).unwrap();
        raw_db.set_lens_store(transform_store.clone());
        let db = Arc::new(raw_db);

        db.create_collection(indexed_users_schema()).await.unwrap();
        let v1 = db
            .get_collection("Users")
            .unwrap()
            .unwrap()
            .version_id()
            .to_string();
        let v2 = add_placeholder_version(&db, "placeholder").await;
        db.set_migration(
            LensConfig::new(
                &v1,
                &v2,
                LensModule::from_bytes(b"\0asm\x01\0\0\0".to_vec()),
            ),
            None,
        )
        .await
        .unwrap();

        let mut source_ids = Vec::new();
        for name in ["Deferred-1", "Deferred-2", "Deferred-3"] {
            source_ids.push(seed_old_user(&db, &v1, name).await);
        }

        let txn = db.new_txn(true).await.unwrap();
        let fetcher = db::LensedDocFetcher::new(db.clone(), txn, transform_store);

        let mut stream = fetcher
            .stream_all_with_deleted("Users", false)
            .await
            .unwrap();
        let mut docs = Vec::new();
        while let Some((doc, _is_deleted)) = stream.next().await.unwrap() {
            docs.push(doc);
        }
        assert_eq!(docs.len(), 3);
        assert!(docs
            .iter()
            .all(|doc| doc.get("verified") == Some(&document::NormalValue::Bool(true))));
        drop(stream);

        fetcher.take_txn().await.unwrap().discard().unwrap();
        for doc_id in source_ids {
            let stored = load_user(&db, &doc_id).await;
            assert_eq!(stored.collection_version_id(), Some(v1.as_str()));
            assert_eq!(stored.get("verified"), None);
        }
        assert_eq!(verified_index_count(&db).await, 0);
    })
    .unwrap()
}

/// Streaming reads transform values without persisting them.
#[test]
fn streaming_auto_commit_read_transforms_without_persisting() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let transform_store = Arc::new(SetVerifiedStore::default());
        let mut raw_db = DB::from_arc(store.clone()).unwrap();
        raw_db.set_lens_store(transform_store.clone());
        let db = Arc::new(raw_db);

        db.create_collection(users_schema()).await.unwrap();
        let v1 = db
            .get_collection("Users")
            .unwrap()
            .unwrap()
            .version_id()
            .to_string();
        let doc_id = seed_user(&db).await;
        let v2 = add_verified_version(&db).await;

        db.set_migration(
            LensConfig::new(
                &v1,
                &v2,
                LensModule::from_bytes(b"\0asm\x01\0\0\0".to_vec()),
            ),
            None,
        )
        .await
        .unwrap();

        let fetcher = db::LensedAutoCommitFetcher::new(db.clone());
        let mut stream = fetcher
            .stream_all_with_deleted("Users", false)
            .await
            .unwrap();
        let mut docs = Vec::new();
        while let Some((doc, _is_deleted)) = stream.next().await.unwrap() {
            docs.push(doc);
        }
        drop(stream);

        assert_eq!(docs.len(), 1);
        assert_eq!(
            docs[0].get("verified"),
            Some(&document::NormalValue::Bool(true))
        );
        assert_eq!(docs[0].collection_version_id(), Some(v2.as_str()));
        assert_eq!(transform_store.transform_calls.load(Ordering::SeqCst), 1);

        let persisted = load_user(&db, &doc_id).await;
        assert_eq!(persisted.get("verified"), None);
        assert_eq!(persisted.collection_version_id(), Some(v1.as_str()));
    })
    .unwrap()
}

/// Closing a partially consumed stream must also remain write-free.
#[test]
fn streaming_auto_commit_read_is_write_free_after_partial_consumption() {
    workers::block_on(async {
        let transform_store = Arc::new(SetVerifiedStore::default());
        let mut raw_db = DB::new(RegolithStore::in_memory().unwrap()).unwrap();
        raw_db.set_lens_store(transform_store.clone());
        let db = Arc::new(raw_db);

        db.create_collection(users_schema()).await.unwrap();
        let v1 = db
            .get_collection("Users")
            .unwrap()
            .unwrap()
            .version_id()
            .to_string();
        let v2 = add_verified_version(&db).await;

        db.set_migration(
            LensConfig::new(
                &v1,
                &v2,
                LensModule::from_bytes(b"\0asm\x01\0\0\0".to_vec()),
            ),
            None,
        )
        .await
        .unwrap();

        for name in ["Partial-1", "Partial-2", "Partial-3"] {
            seed_old_user(&db, &v1, name).await;
        }

        let fetcher = db::LensedAutoCommitFetcher::new(db.clone());
        let mut stream = fetcher
            .stream_all_with_deleted("Users", false)
            .await
            .unwrap();

        // Stop after one document out of three.
        let (migrated, _is_deleted) = stream
            .next()
            .await
            .unwrap()
            .expect("stream yields the first document");
        assert_eq!(
            migrated.get("verified"),
            Some(&document::NormalValue::Bool(true))
        );

        stream.close().await.unwrap();
        drop(stream);

        let doc_id = migrated.id().expect("migrated document keeps its DocID");
        let persisted = load_user(&db, doc_id).await;
        assert_eq!(
            persisted.get("verified"),
            None,
            "a partially consumed read must not persist its transformations"
        );
        assert_eq!(persisted.collection_version_id(), Some(v1.as_str()));
    })
    .unwrap()
}

#[test]
fn readonly_snapshot_history_reuse_preserves_migrated_values() {
    workers::block_on(async {
        let mut raw_db = DB::new(RegolithStore::in_memory().unwrap()).unwrap();
        raw_db.set_lens_store(Arc::new(SetVerifiedStore::default()));
        let db = Arc::new(raw_db);
        db.create_collection(users_schema()).await.unwrap();
        let v1 = db
            .get_collection("Users")
            .unwrap()
            .unwrap()
            .version_id()
            .to_owned();
        let doc_id = seed_user(&db).await;
        let v2 = add_verified_version(&db).await;
        db.set_migration(
            LensConfig::new(
                &v1,
                &v2,
                LensModule::from_bytes(b"\0asm\x01\0\0\0".to_vec()),
            ),
            None,
        )
        .await
        .unwrap();
        let fetcher = db::read::lensed::fetcher::LensedDocFetcher::new(
            db.clone(),
            db.new_txn(true).await.unwrap(),
            db.lens_store().clone(),
        );
        for _ in 0..2 {
            let docs = fetcher
                .get_by_ids("Users", &[doc_id.to_string()])
                .await
                .unwrap()
                .into_docs();
            assert_eq!(docs.len(), 1);
            assert_eq!(
                docs[0].get("verified"),
                Some(&document::NormalValue::Bool(true))
            );
            assert_eq!(docs[0].collection_version_id(), Some(v2.as_str()));
        }
        let _ = fetcher.take_txn().await.unwrap().discard();
        assert_eq!(
            load_user(&db, &doc_id).await.collection_version_id(),
            Some(v1.as_str())
        );
    })
    .unwrap()
}

#[test]
fn eager_materialization_retries_from_the_concurrent_document() {
    workers::block_on(async {
        let transforms = Arc::new(SetVerifiedStore::default());
        let mut raw_db = DB::from_arc(Arc::new(RegolithStore::in_memory().unwrap())).unwrap();
        raw_db.set_lens_store(transforms.clone());
        let db = Arc::new(raw_db);
        db.create_collection(users_schema()).await.unwrap();
        let v1 = db
            .get_collection("Users")
            .unwrap()
            .unwrap()
            .version_id()
            .to_owned();
        let doc_id = seed_user(&db).await;
        let v2 = add_verified_version(&db).await;
        db.set_migration(
            LensConfig::new(
                &v1,
                &v2,
                LensModule::from_bytes(b"\0asm\x01\0\0\0".to_vec()),
            ),
            None,
        )
        .await
        .unwrap();
        let writer_db = db.clone();
        let writer_id = doc_id.clone();
        transforms.before_transform.store_some(Box::new(move || {
            // Commit from another thread after the materializer read the old
            // document, but before it writes the transformed value.
            let writer_db = writer_db.clone();
            let writer_id = writer_id.clone();
            std::thread::spawn(move || {
                futures::executor::block_on(async move {
                    let collection = writer_db.get_collection("Users").unwrap().unwrap();
                    let txn = writer_db.new_txn(false).await.unwrap();
                    let data = txn.datastore().unwrap();
                    let system = txn.systemstore().unwrap();
                    let short = collection
                        .resolve_doc_short_id(&system, &writer_id)
                        .await
                        .unwrap()
                        .unwrap();
                    let mut doc = collection
                        .get_by_doc_id(&data, &system, &writer_id)
                        .await
                        .unwrap()
                        .unwrap();
                    doc.set("name", document::NormalValue::String("Concurrent".into()))
                        .unwrap();
                    collection
                        .save_with_datastore(&data, &doc, short)
                        .await
                        .unwrap();
                    drop(data);
                    drop(system);
                    txn.commit().await.unwrap();
                })
            })
            .join()
            .unwrap();
        }));
        assert_eq!(db.materialize_collection("Users").await.unwrap(), 1);
        let persisted = load_user(&db, &doc_id).await;
        assert_eq!(
            persisted.get("name"),
            Some(&document::NormalValue::String("Concurrent".into()))
        );
        assert_eq!(
            persisted.get("verified"),
            Some(&document::NormalValue::Bool(true))
        );
        assert_eq!(persisted.collection_version_id(), Some(v2.as_str()));
        assert_eq!(transforms.transform_calls.load(Ordering::SeqCst), 2);
    })
    .unwrap()
}
