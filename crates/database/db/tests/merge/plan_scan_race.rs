//! An autocommit query plans its index from the cached collection, then scans
//! in a read transaction of its own. A merge that stores a document off the
//! active version across a lens can commit, and owe a reindex, in between.
//! The query must still find that document: the scan reads the owed record
//! again and the select falls back to a collection scan.
//!
//! The fetcher here commits such a merge right before the first index scan
//! it is asked for, so the race happens on every run.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use blockstore::{Blockstore, DefraBlockstore};
use datastore::{Namespace, NamespaceView, SharedTxn};
use db::merge::DbMergeHandler;
use db::{LensedAutoCommitFetcher, DB};
use defra_core::merge::{BlockMetadata, MergeHandler, MergeOutcome};
use document::{Document, NormalValue};
use futures::StreamExt;
use lens::{
    LensConfig, LensDocResultStream, LensDocStream, LensModule, MemoryTransformStore, TransformId,
    TransformStore,
};
use query::doc_stream::DocStream;
use query::error::Result;
use query::planner::index_selection::IndexScanParams;
use query::{DocFetcher, FetchByIdsResult, QueryExecutor, QueryRequest};
use rapidhash::RapidHashMap;
use schema::{CollectionVersion, FieldDescription, FieldKind, IndexDescription};
use storage::{RegolithStore, Store};

const V1: &str = "v1";
const COLLECTION_ID: &str = "col-users";
const CREATOR: &str = "did:key:z6MkrPlanScanRace";
/// The value the lens gives every document's `name`.
const LENSED: &str = "Grace";

fn users() -> CollectionVersion {
    let mut version = CollectionVersion::new(
        "Users",
        V1,
        COLLECTION_ID,
        vec![
            FieldDescription::new(
                "1",
                "_docID",
                FieldKind::doc_id(),
                schema::CType::LwwRegister,
            ),
            FieldDescription::new("2", "name", FieldKind::string(), schema::CType::LwwRegister),
        ],
    );
    version.indexes = vec![IndexDescription::new("idx_name").with_field("name", false)];
    version
}

/// A lens store whose every lens sets `name` to [`LENSED`].
#[derive(Default)]
struct RenamingStore {
    inner: MemoryTransformStore,
}

#[async_trait]
impl TransformStore for RenamingStore {
    fn new_isolated_store(&self) -> Arc<dyn TransformStore> {
        Arc::new(Self::default())
    }

    async fn add(&self, config: LensConfig) -> lens::Result<TransformId> {
        self.inner.add(config).await
    }

    async fn add_with_id(&self, id: TransformId, config: LensConfig) -> lens::Result<()> {
        self.inner.add_with_id(id, config).await
    }

    async fn list(&self) -> lens::Result<RapidHashMap<String, Vec<LensModule>>> {
        self.inner.list().await
    }

    fn transform(
        &self,
        _id: &TransformId,
        docs: LensDocStream,
    ) -> lens::Result<LensDocResultStream> {
        Ok(Box::pin(docs.map(|mut doc| {
            doc.insert("name".to_string(), serde_json::json!(LENSED));
            Ok(doc)
        })))
    }

    fn inverse(&self, id: &TransformId, docs: LensDocStream) -> lens::Result<LensDocResultStream> {
        self.transform(id, docs)
    }

    fn has_transform(&self, id: &TransformId) -> bool {
        self.inner.has_transform(id)
    }

    async fn remove(&self, id: &TransformId) -> lens::Result<()> {
        self.inner.remove(id).await
    }
}

/// Merges one block of a new document named `name`, written at `v1`, into
/// `db`, the way a peer's push lands.
async fn merge_at_v1(db: &Arc<DB<RegolithStore>>, store: &Arc<RegolithStore>, name: &str) {
    let source = RegolithStore::in_memory()
        .unwrap()
        .new_txn(false)
        .await
        .unwrap();
    let shared = SharedTxn::new(source, storage::Crdts::default());
    let source_blocks = NamespaceView::new(shared.clone(), Namespace::Blockstore);
    let source_heads = NamespaceView::new(shared, Namespace::Headstore);
    let mut doc = Document::new(users());
    doc.set("name", NormalValue::String(name.to_string()))
        .unwrap();
    let written = db::block::builder::write_document_blocks(
        &source_blocks,
        &source_heads,
        &doc,
        &users(),
        db::block::builder::DocRef::new(1, 1),
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap();
    let blockstore = Arc::new(DefraBlockstore::new(store.clone(), false));
    for cid in std::iter::once(written.cid).chain(written.field_cids.iter().copied()) {
        let bytes = source_blocks.get(&cid.to_bytes()).await.unwrap().unwrap();
        blockstore.put(&cid, &bytes).await.unwrap();
    }
    let handler = DbMergeHandler::new(db.clone(), blockstore);
    let metadata = BlockMetadata::normal(&written.doc_id, COLLECTION_ID, CREATOR, None, false);
    let outcome = handler
        .handle_block(&written.cid, &written.block, metadata)
        .await
        .unwrap();
    assert!(matches!(outcome, MergeOutcome::Merged), "{outcome:?}");
}

/// The node's autocommit fetcher, which commits an off-version merge right
/// before the first index scan it serves while armed.
struct RacingFetcher {
    inner: LensedAutoCommitFetcher<RegolithStore>,
    db: Arc<DB<RegolithStore>>,
    store: Arc<RegolithStore>,
    armed: Arc<AtomicBool>,
    merged: AtomicUsize,
}

impl RacingFetcher {
    async fn race(&self) {
        if self.armed.swap(false, Ordering::SeqCst) {
            let n = self.merged.fetch_add(1, Ordering::SeqCst);
            merge_at_v1(&self.db, &self.store, &format!("Ada-{n}")).await;
        }
    }
}

#[async_trait]
impl DocFetcher for RacingFetcher {
    async fn get_all(&self, collection_name: &str) -> Result<Vec<Document>> {
        self.inner.get_all(collection_name).await
    }

    async fn get_all_with_deleted(
        &self,
        collection_name: &str,
        show_deleted: bool,
    ) -> Result<Vec<(Document, bool)>> {
        self.inner
            .get_all_with_deleted(collection_name, show_deleted)
            .await
    }

    async fn stream_all_with_deleted(
        &self,
        collection_name: &str,
        show_deleted: bool,
    ) -> Result<Box<dyn DocStream>> {
        self.inner
            .stream_all_with_deleted(collection_name, show_deleted)
            .await
    }

    async fn stream_by_doc_short_ids(
        &self,
        collection_name: &str,
        doc_short_ids: &[u64],
        show_deleted: bool,
    ) -> Result<Box<dyn DocStream>> {
        self.inner
            .stream_by_doc_short_ids(collection_name, doc_short_ids, show_deleted)
            .await
    }

    async fn stream_index_scan(
        &self,
        collection_name: &str,
        params: &IndexScanParams,
        show_deleted: bool,
    ) -> Result<Option<Box<dyn DocStream>>> {
        self.race().await;
        self.inner
            .stream_index_scan(collection_name, params, show_deleted)
            .await
    }

    async fn get_by_index_scan(
        &self,
        collection_name: &str,
        params: &IndexScanParams,
    ) -> Result<query::fetcher::IndexScanResult> {
        self.race().await;
        self.inner.get_by_index_scan(collection_name, params).await
    }

    async fn estimate_index_scan(
        &self,
        collection_name: &str,
        params: &IndexScanParams,
        cap: u64,
    ) -> Result<Option<u64>> {
        self.inner
            .estimate_index_scan(collection_name, params, cap)
            .await
    }

    fn supports_index_queries(&self) -> bool {
        self.inner.supports_index_queries()
    }

    async fn get_by_ids(&self, collection_name: &str, ids: &[String]) -> Result<FetchByIdsResult> {
        self.inner.get_by_ids(collection_name, ids).await
    }

    async fn get_by_field_value(
        &self,
        collection_name: &str,
        field_name: &str,
        value: &str,
    ) -> Result<Vec<Document>> {
        self.inner
            .get_by_field_value(collection_name, field_name, value)
            .await
    }
}

fn names(response: &query::QueryResponse) -> Vec<String> {
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let mut names: Vec<String> = response.data.as_ref().unwrap()["Users"]
        .as_array()
        .unwrap()
        .iter()
        .map(|user| user["_docID"].as_str().unwrap().to_string())
        .collect();
    names.sort();
    names
}

/// Each query shape plans the index while nothing is owed, then a merge
/// stores a new document at `v1` (its index entry holds `Ada-n`, while reads
/// see `Grace` through the lens) before the scan. Every shape must return
/// every document, the raced one included.
#[test]
fn a_merge_between_plan_and_scan_does_not_hide_the_document() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let mut raw = DB::from_arc(store.clone()).unwrap();
        raw.set_lens_store(Arc::new(RenamingStore::default()));
        let db = Arc::new(raw);
        db.create_collection(users()).await.unwrap();
        db.patch_collection(
            &defra_core::RequestContext::default(),
            r#"[{"op":"add","path":"/Users/Fields/-","value":{"Name":"level","Kind":"Int"}}]"#,
            None,
        )
        .await
        .unwrap();
        let v2 = db
            .get_collection("Users")
            .unwrap()
            .unwrap()
            .version_id()
            .to_string();
        db.set_migration(
            LensConfig::new(V1, &v2, LensModule::from_bytes(b"\0asm\x01\0\0\0".to_vec())),
            None,
        )
        .await
        .unwrap();

        let armed = Arc::new(AtomicBool::new(false));
        let fetcher = RacingFetcher {
            inner: LensedAutoCommitFetcher::new(db.clone()),
            db: db.clone(),
            store: store.clone(),
            armed: armed.clone(),
            merged: AtomicUsize::new(0),
        };
        let runner = query::QueryRunner::with_provider(
            fetcher,
            db::DbCollectionProvider::new_arc(db.clone()),
        );
        let shapes = [
            r#"query { Users(filter: {name: {_eq: "Grace"}}) { _docID name } }"#,
            r#"query { Users(filter: {name: {_eq: "Grace"}}, order: {name: ASC}, limit: 10) { _docID name } }"#,
            r#"query { Users(filter: {name: {_in: ["Grace"]}}, order: {name: DESC}, offset: 0, limit: 10) { _docID name } }"#,
        ];
        for (round, shape) in shapes.iter().enumerate() {
            db.settle_owed_reindexes().await.unwrap();
            assert!(db.owed_reindexes().await.unwrap().is_empty());
            armed.store(true, Ordering::SeqCst);

            let raced = runner
                .execute(QueryRequest::new(shape.to_string()))
                .await;
            assert!(
                !armed.load(Ordering::SeqCst),
                "the query never scanned an index, so it raced nothing: {shape}"
            );
            assert!(
                !db.owed_reindexes().await.unwrap().is_empty(),
                "the raced merge owes the reindex"
            );

            let unindexed = runner
                .execute(QueryRequest::new(
                    r#"query { Users { _docID name } }"#.to_string(),
                ))
                .await;
            assert_eq!(names(&unindexed).len(), round + 1);
            assert_eq!(
                names(&raced),
                names(&unindexed),
                "the indexed query missed the document merged after it planned: {shape}"
            );
        }
    })
    .unwrap()
}
