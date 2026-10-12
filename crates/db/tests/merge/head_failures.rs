use async_trait::async_trait;
use blockstore::{Blockstore as _, DefraBlockstore};
use cid::Cid;
use db::{merge::merge_handler::DbMergeHandler, DB};
use defra_core::block::{
    generate_cid_from_bytes, Block, CompositeDeltaPayload, CrdtDelta, DAGLink, LwwDeltaPayload,
};
use defra_core::merge::{BlockMetadata, MergeBlock, MergeHandler};
use std::sync::{Arc, Mutex};
use storage::corekv::{
    private::Sealed, AsyncTxnCallback, IterOptions, Iterator, Reader, Result, Store, Txn,
    TxnCallback, Writer,
};
use storage::RegolithStore;

/// One headstore write, identified by the CID it records, that the store refuses.
#[derive(Clone, Copy, Debug)]
enum FailPoint {
    SetHead(Cid),
    DeleteHead(Cid),
    SetPriority(Cid),
}

impl FailPoint {
    fn refuses(self, delete: bool, key: &[u8]) -> bool {
        let Some(key) = key.strip_prefix(b"h") else {
            return false;
        };
        let head = storage::keys::headstore::HeadstoreDocKey::parse(key);
        match self {
            Self::SetHead(cid) => !delete && head.is_some_and(|head| head.cid == cid),
            Self::DeleteHead(cid) => delete && head.is_some_and(|head| head.cid == cid),
            Self::SetPriority(cid) => {
                !delete && key.starts_with(b"/p/") && key.ends_with(&cid.to_bytes())
            }
        }
    }
}

type FailSwitch = Arc<Mutex<Vec<FailPoint>>>;

struct FailingHeadStore {
    inner: Arc<RegolithStore>,
    fail: FailSwitch,
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
    fail: FailSwitch,
}
impl Sealed for FailingHeadTxn {}

impl FailingHeadTxn {
    fn check(&self, delete: bool, key: &[u8]) -> Result<()> {
        match self
            .fail
            .lock()
            .unwrap()
            .iter()
            .find(|point| point.refuses(delete, key))
        {
            Some(point) => Err(storage::Error::Other(format!(
                "injected head write failure at {point:?}"
            ))),
            None => Ok(()),
        }
    }
}

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
        self.check(false, key)?;
        self.inner.set(key, value).await
    }
    async fn delete(&mut self, key: &[u8]) -> Result<()> {
        self.check(true, key)?;
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

type Handler = DbMergeHandler<FailingHeadStore, DefraBlockstore<RegolithStore>>;

struct Fixture {
    handler: Handler,
    blocks: Arc<DefraBlockstore<RegolithStore>>,
    fail: FailSwitch,
}

async fn fixture() -> Fixture {
    let inner = Arc::new(RegolithStore::in_memory().unwrap());
    let fail = FailSwitch::default();
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
    let blocks = Arc::new(DefraBlockstore::new(inner, false));
    let handler = DbMergeHandler::new(db, blocks.clone());
    Fixture {
        handler,
        blocks,
        fail,
    }
}

impl Fixture {
    fn fail_at(&self, points: &[FailPoint]) {
        *self.fail.lock().unwrap() = points.to_vec();
    }

    async fn merge(&self, block: &MergeBlock) -> std::result::Result<(), String> {
        self.handler
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
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    /// Every datastore and headstore entry, so a failed merge can be shown to
    /// leave both exactly as they were.
    async fn snapshot(&self) -> Vec<(Vec<u8>, Vec<u8>)> {
        let txn = self.handler.db().new_txn(true).await.unwrap();
        let mut entries = Vec::new();
        for store in [txn.datastore().unwrap(), txn.headstore().unwrap()] {
            let mut iter = store.iterator(IterOptions::default()).await.unwrap();
            while let Some(pair) = iter.next().await.unwrap() {
                entries.push((pair.key_bytes().to_vec(), pair.value_bytes().to_vec()));
            }
            iter.close().await.unwrap();
        }
        txn.discard().unwrap();
        entries
    }
}

/// An update of `name` on top of `genesis`: one LWW field block superseding the
/// genesis `name` block, under a composite block superseding the genesis composite.
struct Update {
    block: MergeBlock,
    genesis: Cid,
    genesis_name: Cid,
    name: Cid,
}

async fn update_of(blocks: &DefraBlockstore<RegolithStore>, genesis: &MergeBlock) -> Update {
    let genesis_name = Block::from_dag_cbor(&genesis.block_data)
        .unwrap()
        .links
        .unwrap()
        .into_iter()
        .find(|link| link.name == "name")
        .unwrap()
        .link;
    let field = Block::new(
        CrdtDelta::Lww(LwwDeltaPayload {
            field_name: "name".to_string(),
            priority: 2,
            schema_version_id: "v1".to_string(),
            data: db::block::builder::encode_value_as_cbor(&document::NormalValue::String(
                "Bob".to_string(),
            ))
            .unwrap(),
        }),
        vec![genesis_name],
        vec![],
    )
    .to_dag_cbor()
    .unwrap();
    let name = generate_cid_from_bytes(&field).unwrap();
    blocks.put(&name, &field).await.unwrap();
    let composite = Block::new(
        CrdtDelta::Composite(CompositeDeltaPayload {
            schema_version_id: "v1".to_string(),
            priority: 2,
            status: 1,
        }),
        vec![genesis.cid],
        vec![DAGLink::new("name", name)],
    )
    .to_dag_cbor()
    .unwrap();
    Update {
        block: MergeBlock {
            cid: generate_cid_from_bytes(&composite).unwrap(),
            block_data: composite.into(),
            ..genesis.clone()
        },
        genesis: genesis.cid,
        genesis_name,
        name,
    }
}

#[tokio::test]
async fn composite_head_write_failure_aborts_standalone_merge() {
    let fixture = fixture().await;
    let block = super::merge_handler_tests::build_merge_block(&fixture.blocks, "Alice", 30).await;
    let before = fixture.snapshot().await;
    fixture.fail_at(&[FailPoint::SetHead(block.cid)]);
    let result = fixture.merge(&block).await;
    assert!(
        result.is_err(),
        "head write failure must fail the merge: {result:?}"
    );
    assert_eq!(
        fixture.snapshot().await,
        before,
        "failed merge must leave no document, field or head writes"
    );
}

#[tokio::test]
async fn composite_head_write_failure_aborts_batch_merge() {
    let fixture = fixture().await;
    let first = super::merge_handler_tests::build_merge_block(&fixture.blocks, "Alice", 30).await;
    let second = super::merge_handler_tests::build_merge_block(&fixture.blocks, "Bob", 31).await;
    let before = fixture.snapshot().await;
    fixture.fail_at(&[
        FailPoint::SetHead(first.cid),
        FailPoint::SetHead(second.cid),
    ]);
    let results = fixture.handler.handle_block_batch(&[first, second]).await;
    assert!(
        results.iter().all(|result| result.is_err()),
        "head write failure must fail the batch: {results:?}"
    );
    assert_eq!(
        fixture.snapshot().await,
        before,
        "failed batch must leave no document, field or head writes"
    );
}

async fn assert_update_rolls_back(point: impl FnOnce(&Update) -> FailPoint) {
    let fixture = fixture().await;
    let genesis = super::merge_handler_tests::build_merge_block(&fixture.blocks, "Alice", 30).await;
    fixture.merge(&genesis).await.unwrap();
    let update = update_of(&fixture.blocks, &genesis).await;
    let before = fixture.snapshot().await;
    let point = point(&update);
    fixture.fail_at(&[point]);
    let result = fixture.merge(&update.block).await;
    assert!(result.is_err(), "{point:?} must fail the merge: {result:?}");
    assert_eq!(
        fixture.snapshot().await,
        before,
        "{point:?} must roll back the head writes that preceded it"
    );
    fixture.fail_at(&[]);
    fixture.merge(&update.block).await.unwrap();
    assert_ne!(
        fixture.snapshot().await,
        before,
        "the update must apply once {point:?} succeeds"
    );
}

#[tokio::test]
async fn composite_parent_delete_failure_aborts_update_merge() {
    assert_update_rolls_back(|update| FailPoint::DeleteHead(update.genesis)).await;
}

#[tokio::test]
async fn composite_head_write_failure_aborts_update_merge() {
    assert_update_rolls_back(|update| FailPoint::SetHead(update.block.cid)).await;
}

#[tokio::test]
async fn composite_priority_write_failure_aborts_update_merge() {
    assert_update_rolls_back(|update| FailPoint::SetPriority(update.block.cid)).await;
}

#[tokio::test]
async fn field_parent_delete_failure_aborts_update_merge() {
    assert_update_rolls_back(|update| FailPoint::DeleteHead(update.genesis_name)).await;
}

#[tokio::test]
async fn field_head_write_failure_aborts_update_merge() {
    assert_update_rolls_back(|update| FailPoint::SetHead(update.name)).await;
}

#[tokio::test]
async fn field_priority_write_failure_aborts_update_merge() {
    assert_update_rolls_back(|update| FailPoint::SetPriority(update.name)).await;
}
