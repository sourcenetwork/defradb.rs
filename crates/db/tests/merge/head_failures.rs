use async_trait::async_trait;
use blockstore::{Blockstore as _, DefraBlockstore};
use cid::Cid;
use db::{merge::merge_handler::DbMergeHandler, DB};
use defra_core::block::{
    generate_cid_from_bytes, Block, CompositeDeltaPayload, CrdtDelta, DAGLink, LwwDeltaPayload,
};
use defra_core::merge::{BlockMetadata, MergeBlock, MergeHandler};
use kovan::Atom;
use std::sync::Arc;
use storage::corekv::{
    intercept_retry_writes, private::Sealed, IterOptions, Iterator, Reader, Result, RetryAttempt,
    RetryAttemptError, RetryAttemptValue, RetryOptions, RetryWriteInterceptor, Store, Txn, Writer,
};
use storage::RegolithStore;

mod collection;

/// One headstore write, identified by the CID it records, that the store
/// refuses. A head set is written by one merge per set, so the points that
/// name a document's head, its parent or a collection's head or parent refuse
/// the merge that carries that CID.
#[derive(Clone, Copy, Debug)]
enum FailPoint {
    SetHead(Cid),
    SupersedeHead(Cid),
    SetPriority(Cid),
    /// The head set merge that adds a collection block.
    CollectionHead(Cid),
    /// The head set merge that supersedes a parent for this collection block.
    CollectionSupersede(Cid),
}

impl FailPoint {
    fn refuses(self, key: &[u8], value: Option<&[u8]>) -> bool {
        let Some(key) = key.strip_prefix(b"h") else {
            return false;
        };
        let names = |cid: Cid| {
            let text = cid.to_string();
            value.is_some_and(|value| {
                value
                    .windows(text.len())
                    .any(|window| window == text.as_bytes())
            })
        };
        match self {
            Self::SetHead(cid) | Self::SupersedeHead(cid) => key.starts_with(b"/hd/") && names(cid),
            Self::SetPriority(cid) => {
                value.is_some() && key.starts_with(b"/p/") && key.ends_with(&cid.to_bytes())
            }
            Self::CollectionHead(cid) | Self::CollectionSupersede(cid) => {
                key.starts_with(b"/hc/") && names(cid)
            }
        }
    }
}

type FailSwitch = Arc<Atom<Vec<FailPoint>>>;

struct FailingHeadStore {
    inner: Arc<RegolithStore>,
    fail: FailSwitch,
}

impl Sealed for FailingHeadStore {}

#[async_trait]
impl Store for FailingHeadStore {
    fn crdts(&self) -> &storage::Crdts {
        self.inner.crdts()
    }

    async fn allocate(&self, key: &[u8], count: u64) -> storage::Result<std::ops::Range<u64>> {
        self.inner.allocate(key, count).await
    }

    async fn new_txn(&self, readonly: bool) -> Result<Box<dyn Txn>> {
        Ok(Box::new(FailingHeadTxn {
            inner: self.inner.new_txn(readonly).await?,
            fail: self.fail.clone(),
        }))
    }
    async fn transact_retry(
        &self,
        options: RetryOptions,
        attempt: RetryAttempt,
    ) -> std::result::Result<RetryAttemptValue, RetryAttemptError> {
        let fail = self.fail.clone();
        let interceptor: RetryWriteInterceptor = Arc::new(move |key, value| {
            match fail.load().iter().find(|point| point.refuses(key, value)) {
                Some(point) => Err(storage::Error::Other(format!(
                    "injected head write failure at {point:?}"
                ))),
                None => Ok(()),
            }
        });
        self.inner
            .transact_retry(options, intercept_retry_writes(attempt, interceptor))
            .await
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
    fn check(&self, key: &[u8], value: Option<&[u8]>) -> Result<()> {
        match self
            .fail
            .load()
            .iter()
            .find(|point| point.refuses(key, value))
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

    async fn get_size(&self, key: &[u8]) -> Result<Option<usize>> {
        self.inner.get_size(key).await
    }
    async fn iterator(&self, opts: IterOptions) -> Result<Box<dyn Iterator>> {
        self.inner.iterator(opts).await
    }
}

#[async_trait]
impl Writer for FailingHeadTxn {
    async fn merge_shared(&self, key: &[u8], operand: &[u8]) -> Result<()> {
        self.check(key, Some(operand))?;
        self.inner.merge_shared(key, operand).await
    }

    async fn append_shared(
        &self,
        log: &storage::corekv::AppendLog,
        entry: &[u8],
        once_key: Option<&[u8]>,
    ) -> storage::Result<()> {
        self.inner.append_shared(log, entry, once_key).await
    }

    async fn set_shared(&self, key: &[u8], value: &[u8]) -> Result<()> {
        self.check(key, Some(value))?;
        self.inner.set_shared(key, value).await
    }
    async fn delete_shared(&self, key: &[u8]) -> Result<()> {
        self.check(key, None)?;
        self.inner.delete_shared(key).await
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
    fn on_commit(&mut self, callback: storage::corekv::OnCommit) -> Result<()> {
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
        self.fail.store(points.to_vec());
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
            collection_version_id: "v1".to_string(),
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
            collection_version_id: "v1".to_string(),
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

#[test]
fn composite_head_write_failure_aborts_standalone_merge() {
    workers::block_on(async {
        let fixture = fixture().await;
        let block =
            super::merge_handler_tests::build_merge_block(&fixture.blocks, "Alice", 30).await;
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
    })
    .unwrap()
}

#[test]
fn composite_head_write_failure_aborts_batch_merge() {
    workers::block_on(async {
        let fixture = fixture().await;
        let first =
            super::merge_handler_tests::build_merge_block(&fixture.blocks, "Alice", 30).await;
        let second =
            super::merge_handler_tests::build_merge_block(&fixture.blocks, "Bob", 31).await;
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
    })
    .unwrap()
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

#[test]
fn composite_parent_marker_failure_aborts_update_merge() {
    workers::block_on(async {
        assert_update_rolls_back(|update| FailPoint::SupersedeHead(update.genesis)).await;
    })
    .unwrap()
}

#[test]
fn composite_head_write_failure_aborts_update_merge() {
    workers::block_on(async {
        assert_update_rolls_back(|update| FailPoint::SetHead(update.block.cid)).await;
    })
    .unwrap()
}

#[test]
fn composite_priority_write_failure_aborts_update_merge() {
    workers::block_on(async {
        assert_update_rolls_back(|update| FailPoint::SetPriority(update.block.cid)).await;
    })
    .unwrap()
}

#[test]
fn field_parent_marker_failure_aborts_update_merge() {
    workers::block_on(async {
        assert_update_rolls_back(|update| FailPoint::SupersedeHead(update.genesis_name)).await;
    })
    .unwrap()
}

#[test]
fn field_head_write_failure_aborts_update_merge() {
    workers::block_on(async {
        assert_update_rolls_back(|update| FailPoint::SetHead(update.name)).await;
    })
    .unwrap()
}

#[test]
fn field_priority_write_failure_aborts_update_merge() {
    workers::block_on(async {
        assert_update_rolls_back(|update| FailPoint::SetPriority(update.name)).await;
    })
    .unwrap()
}
