use async_trait::async_trait;
use blockstore::DefraBlockstore;
use db::{merge::merge_handler::DbMergeHandler, DB};
use defra_core::merge::{BlockMetadata, MergeHandler};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use storage::corekv::{
    private::Sealed, AsyncTxnCallback, IterOptions, Iterator, Reader, Result, Store, Txn,
    TxnCallback, Writer,
};
use storage::RegolithStore;

struct FailingHeadStore {
    inner: Arc<RegolithStore>,
    fail: Arc<AtomicBool>,
}

impl Sealed for FailingHeadStore {}

#[async_trait]
impl Store for FailingHeadStore {
    async fn new_txn(&self, readonly: bool) -> Result<Box<dyn Txn>> {
        Ok(Box::new(FailingHeadTxn {
            inner: self.inner.new_txn(readonly).await?,
            fail: self.fail.clone(),
        }))
    }
    async fn close(&self) -> Result<()> {
        self.inner.close().await
    }
}

struct FailingHeadTxn {
    inner: Box<dyn Txn>,
    fail: Arc<AtomicBool>,
}
impl Sealed for FailingHeadTxn {}

#[async_trait]
impl Reader for FailingHeadTxn {
    async fn get(&self, key: &[u8]) -> Result<Option<bytes::Bytes>> {
        self.inner.get(key).await
    }
    async fn has(&self, key: &[u8]) -> Result<bool> {
        self.inner.has(key).await
    }
    async fn has_for_update(&self, key: &[u8]) -> Result<bool> {
        self.inner.has_for_update(key).await
    }
    async fn get_size(&self, key: &[u8]) -> Result<Option<usize>> {
        self.inner.get_size(key).await
    }
    async fn iterator(&self, opts: IterOptions) -> Result<Box<dyn Iterator>> {
        self.inner.iterator(opts).await
    }
}

#[async_trait]
impl Writer for FailingHeadTxn {
    async fn set(&mut self, key: &[u8], value: &[u8]) -> Result<()> {
        if self.fail.load(Ordering::SeqCst) && key.first() == Some(&b'h') {
            if let Some(head) = storage::keys::headstore::HeadstoreDocKey::parse(&key[1..]) {
                if head.field_id == "C" {
                    return Err(storage::Error::Other(
                        "injected composite head failure".into(),
                    ));
                }
            }
        }
        self.inner.set(key, value).await
    }
    async fn delete(&mut self, key: &[u8]) -> Result<()> {
        self.inner.delete(key).await
    }
}

#[async_trait]
impl Txn for FailingHeadTxn {
    async fn commit(self: Box<Self>) -> Result<()> {
        self.inner.commit().await
    }
    fn discard(self: Box<Self>) {
        self.inner.discard()
    }
    fn on_success(&mut self, callback: TxnCallback) {
        self.inner.on_success(callback)
    }
    fn on_success_async(&mut self, callback: AsyncTxnCallback) {
        self.inner.on_success_async(callback)
    }
    fn on_error(&mut self, callback: TxnCallback) {
        self.inner.on_error(callback)
    }
    fn on_error_async(&mut self, callback: AsyncTxnCallback) {
        self.inner.on_error_async(callback)
    }
    fn on_discard(&mut self, callback: TxnCallback) {
        self.inner.on_discard(callback)
    }
    fn on_discard_async(&mut self, callback: AsyncTxnCallback) {
        self.inner.on_discard_async(callback)
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

async fn fixture() -> (
    DbMergeHandler<FailingHeadStore, DefraBlockstore<RegolithStore>>,
    Arc<DefraBlockstore<RegolithStore>>,
) {
    let inner = Arc::new(RegolithStore::in_memory().unwrap());
    let fail = Arc::new(AtomicBool::new(false));
    let db = Arc::new(
        DB::new(FailingHeadStore {
            inner: inner.clone(),
            fail: fail.clone(),
        })
        .unwrap(),
    );
    for schema in query::parse_sdl("type Users { name: String age: Int }").unwrap() {
        let mut schema = schema;
        schema.version_id = "v1".to_string();
        schema.collection_id = "col-users".to_string();
        db.create_collection(schema).await.unwrap();
    }
    let blockstore = Arc::new(DefraBlockstore::new(inner, false));
    let handler = DbMergeHandler::new(db, blockstore.clone());
    fail.store(true, Ordering::SeqCst);
    (handler, blockstore)
}

async fn assert_no_document(
    handler: &DbMergeHandler<FailingHeadStore, DefraBlockstore<RegolithStore>>,
) {
    let txn = handler.db().new_txn(true).await.unwrap();
    let store = txn.datastore().unwrap();
    let mut iter = store.iterator(IterOptions::default()).await.unwrap();
    assert!(
        iter.next().await.unwrap().is_none(),
        "failed merge must leave no document or field writes"
    );
    iter.close().await.unwrap();
    drop(iter);
    drop(store);
    txn.discard().unwrap();
}

#[tokio::test]
async fn composite_head_write_failure_aborts_standalone_merge() {
    let (handler, blocks) = fixture().await;
    let block = super::merge_handler_tests::build_merge_block(&blocks, "Alice", 30).await;
    let result = handler
        .handle_block(
            &block.cid,
            &block.block_data,
            BlockMetadata::normal(
                &block.doc_id,
                &block.collection_id,
                &block.creator,
                None,
                false,
            ),
        )
        .await;
    assert!(
        result.is_err(),
        "head write failure must fail the merge: {result:?}"
    );
    assert_no_document(&handler).await;
}

#[tokio::test]
async fn composite_head_write_failure_aborts_batch_merge() {
    let (handler, blocks) = fixture().await;
    let first = super::merge_handler_tests::build_merge_block(&blocks, "Alice", 30).await;
    let second = super::merge_handler_tests::build_merge_block(&blocks, "Bob", 31).await;
    let results = handler.handle_block_batch(&[first, second]).await;
    assert!(
        results.iter().all(|result| result.is_err()),
        "head write failure must fail the batch: {results:?}"
    );
    assert_no_document(&handler).await;
}

#[tokio::test]
async fn test_update_keeps_doc_id_without_composite_heads() {
    use storage::corekv::Key;
    let (handler, _, _) = super::merge_handler_tests::make_handler_with_schema_and_bus().await;
    let collection = handler.db().get_collection("Users").unwrap().unwrap();
    let mut doc = document::Document::new();
    doc.set("name", "Alice");
    doc.set("age", 30_i64);
    let (doc_id, short_id, created) =
        super::merge_handler_tests::create_doc_locally(&handler, &collection, &mut doc, "v1").await;
    let txn = handler.db().new_txn(false).await.unwrap();
    let headstore = txn.headstore().unwrap();
    let blockstore = txn.blockstore().unwrap();
    headstore
        .delete(&storage::keys::headstore::HeadstoreDocKey::new(short_id, "C", created.cid).bytes())
        .await
        .unwrap();
    doc.set("name", "Bob");
    let changed = ["name".to_string()].into_iter().collect();
    let updated = db::block::builder::write_document_blocks(
        &blockstore,
        &headstore,
        &doc,
        "v1",
        db::block::builder::DocStorageIdentity::new(collection.resolved_root_id(), short_id),
        Some(&changed),
        None,
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(updated.doc_id, doc_id.to_string());
    drop(headstore);
    drop(blockstore);
    txn.discard().unwrap();
}
