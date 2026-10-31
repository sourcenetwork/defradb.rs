//! An index on a custom type whose writes are blind increments follows the
//! merged total, not the increment a write carries, locally and when a
//! peer's increments merge in.

use std::sync::Arc;

use blockstore::{Blockstore as _, DefraBlockstore};
use bytes::Bytes;
use cid::Cid;
use crdt::stored::{Part, StoredField, ValueKind};
use crdt::{Crdts, DeltaParts, FieldCrdt, FieldCrdtError, FieldWrite, MergeResult};
use db::merge::merge_handler::DbMergeHandler;
use db::{AutoCommitMutator, LensedAutoCommitFetcher, DB};
use defra_core::block::{Block, CrdtDelta, CustomDeltaPayload};
use defra_core::merge::{BlockMetadata, MergeBlock, MergeHandler};
use document::{Document, FieldOp, NormalValue};
use query::runner::DocFetcher;
use query::DocMutator;
use rapidhash::{HashSetExt, RapidHashSet};
use schema::{
    CType, CollectionVersion, CrdtName, FieldDescription, FieldKind, IndexKind,
    IndexedFieldDescription, OrderedIndexDescription,
};
use storage::keys::datastore::{IndexDataStoreKey, IndexedField};
use storage::{RegolithStore, RegolithStoreOptions};

use super::create::index_entries;

const NAME: &str = "tally";
const COLLECTION: &str = "Tallies";
const COLLECTION_ID: &str = "col-tallies";

fn name() -> CrdtName {
    CrdtName::new(NAME).unwrap()
}

/// An integer that only grows by increments, under a custom name.
struct Tally;

fn total(state: Part<'_>) -> Option<i64> {
    (state.kind == ValueKind::Custom(name()))
        .then(|| {
            if state.value.is_empty() {
                Some(0)
            } else {
                Some(i64::from_be_bytes(state.value.try_into().ok()?))
            }
        })
        .flatten()
}

impl FieldCrdt for Tally {
    fn ctype(&self) -> CType {
        CType::Custom(name())
    }

    fn replicates_ops(&self) -> bool {
        true
    }

    fn op(&self, input: NormalValue, _creating: bool) -> Result<FieldOp, FieldCrdtError> {
        Ok(FieldOp::Increment(input))
    }

    fn named_op(&self, op: FieldOp) -> Result<FieldOp, FieldCrdtError> {
        Ok(op)
    }

    fn apply(
        &self,
        _field: &FieldDescription,
        current: Option<&NormalValue>,
        op: &FieldOp,
    ) -> Result<NormalValue, FieldCrdtError> {
        let current = current.and_then(NormalValue::as_int).unwrap_or(0);
        match op {
            FieldOp::Increment(by) | FieldOp::Set(by) => Ok(NormalValue::Int(
                current.wrapping_add(by.as_int().unwrap_or(0)),
            )),
            op => Err(FieldCrdtError::UnsupportedOp {
                crdt: self.ctype(),
                op: op.name(),
            }),
        }
    }

    fn write<'value>(
        &self,
        value: &'value NormalValue,
        op: Option<&'value FieldOp>,
    ) -> FieldWrite<'value> {
        match op {
            Some(FieldOp::Increment(by)) => FieldWrite {
                data: by,
                merges: true,
            },
            _ => FieldWrite {
                data: value,
                merges: false,
            },
        }
    }

    fn decode(&self, field: &FieldDescription, data: &[u8]) -> Result<NormalValue, FieldCrdtError> {
        ciborium::from_reader(data)
            .map(NormalValue::Int)
            .map_err(|error| FieldCrdtError::Decode {
                field: field.name.clone(),
                message: error.to_string(),
            })
    }

    fn delta(&self, parts: DeltaParts<'_>) -> CrdtDelta {
        CrdtDelta::Custom(CustomDeltaPayload {
            crdt: NAME.to_owned(),
            field_name: parts.field_name.to_owned(),
            priority: parts.priority,
            nonce: Some(parts.nonce),
            collection_version_id: parts.collection_version_id.to_owned(),
            data: parts.data,
        })
    }

    fn state(
        &self,
        field: &FieldDescription,
        priority: u64,
        value: &NormalValue,
    ) -> Result<StoredField, FieldCrdtError> {
        Ok(StoredField {
            name: field.name.clone(),
            kind: ValueKind::Custom(name()),
            priority,
            value: value.as_int().unwrap_or(0).to_be_bytes().to_vec(),
        })
    }

    fn materialize(&self, state: Part<'_>) -> Option<NormalValue> {
        total(state).map(NormalValue::Int)
    }

    fn merge(&self, state: &mut StoredField, incoming: StoredField) -> Option<MergeResult> {
        let sum = total(state.part())?.wrapping_add(total(incoming.part())?);
        state.value = sum.to_be_bytes().to_vec();
        state.priority = state.priority.max(incoming.priority);
        Some(MergeResult::Applied)
    }

    fn folds_exactly(&self, _state: Part<'_>) -> bool {
        true
    }
}

fn collection() -> CollectionVersion {
    CollectionVersion::new(
        COLLECTION,
        "v1",
        COLLECTION_ID,
        vec![
            FieldDescription::new("1", "_docID", FieldKind::doc_id(), CType::LwwRegister),
            FieldDescription::new("2", "count", FieldKind::int(), CType::Custom(name())),
        ],
    )
}

/// How many index entries hold `value`.
async fn entries_at(db: &DB<RegolithStore>, index_id: u32, value: i64) -> usize {
    let short_id = db
        .require_collection(COLLECTION)
        .unwrap()
        .schema()
        .resolved_root_id();
    let prefix = IndexDataStoreKey::new(
        short_id,
        index_id,
        vec![IndexedField::ascending(NormalValue::Int(value))],
    )
    .try_bytes()
    .unwrap();
    let txn = db.new_txn(true).await.unwrap();
    let datastore = txn.datastore().unwrap();
    let mut iter = storage::corekv::Reader::iterator(
        &datastore,
        storage::corekv::IterOptions::new().with_prefix(prefix),
    )
    .await
    .unwrap();
    let mut count = 0;
    while storage::corekv::Iterator::next(&mut iter)
        .await
        .unwrap()
        .is_some()
    {
        count += 1;
    }
    count
}

/// A node with the custom type, a collection of it and an index on it.
struct Node {
    db: Arc<DB<RegolithStore>>,
    blocks: Arc<DefraBlockstore<RegolithStore>>,
    handler: DbMergeHandler<RegolithStore, DefraBlockstore<RegolithStore>>,
    index_id: u32,
}

impl Node {
    async fn open() -> Self {
        let crdts = Crdts::new([crdt::Registration::new(Tally)]).unwrap();
        let options = RegolithStoreOptions::memory().with_crdts(crdts);
        let store = Arc::new(RegolithStore::open_with_options("regolith-memory", options).unwrap());
        let db = Arc::new(DB::from_arc(store.clone()).unwrap());
        db.create_collection(collection()).await.unwrap();
        let index = db
            .create_index(
                COLLECTION,
                Some("by_count"),
                vec![IndexedFieldDescription {
                    name: "count".to_string(),
                    descending: false,
                }],
                IndexKind::Ordered(OrderedIndexDescription { unique: false }),
            )
            .await
            .unwrap();
        let blocks = Arc::new(DefraBlockstore::new(store, false));
        let handler = DbMergeHandler::new(db.clone(), blocks.clone());
        Self {
            db,
            blocks,
            handler,
            index_id: index.id,
        }
    }

    async fn create(&self, count: i64) -> document::DocID {
        let schema = self.db.require_collection(COLLECTION).unwrap().schema_arc();
        let mut doc = Document::new(schema);
        doc.set("count", NormalValue::Int(count)).unwrap();
        AutoCommitMutator::new(self.db.clone())
            .create(COLLECTION, doc)
            .await
            .unwrap()
            .doc_id
    }

    async fn increment(&self, doc_id: &document::DocID, by: i64) {
        let schema = self.db.require_collection(COLLECTION).unwrap().schema_arc();
        let mut doc = Document::new(schema);
        doc.set_id(doc_id.clone());
        doc.edit("count", FieldOp::Increment(NormalValue::Int(by)))
            .unwrap();
        let mut changed = RapidHashSet::new();
        changed.insert("count".to_string());
        AutoCommitMutator::new(self.db.clone())
            .update(COLLECTION, doc, changed)
            .await
            .unwrap();
    }

    /// The document's newest composite block.
    async fn head(&self, doc_id: &document::DocID) -> (Cid, Bytes) {
        let mut heads =
            db::merge::replication::load_document_head_blocks(&self.db, &doc_id.to_string())
                .await
                .unwrap();
        assert_eq!(heads.len(), 1, "one head");
        heads.remove(0)
    }

    /// Copy the composite `head` and the field blocks it links from `from`.
    async fn copy(&self, from: &Node, (cid, bytes): &(Cid, Bytes)) {
        for link in Block::from_dag_cbor(bytes).unwrap().links.iter().flatten() {
            let data = from.blocks.get(&link.link).await.unwrap().unwrap();
            self.blocks.put(&link.link, &data).await.unwrap();
        }
        self.blocks.put(cid, bytes).await.unwrap();
    }

    async fn receive(&self, from: &Node, doc_id: &document::DocID, head: &(Cid, Bytes)) {
        self.copy(from, head).await;
        let doc_id = doc_id.to_string();
        let metadata = BlockMetadata::normal(
            &doc_id,
            COLLECTION_ID,
            "did:key:z6MkrMergedCrdt",
            Some("peer"),
            false,
        );
        let outcome = self
            .handler
            .handle_block(&head.0, &head.1, metadata)
            .await
            .unwrap();
        assert!(outcome.is_merged(), "got {outcome:?}");
    }

    async fn receive_batch(&self, from: &Node, doc_id: &document::DocID, heads: &[(Cid, Bytes)]) {
        let mut blocks = Vec::new();
        for head in heads {
            self.copy(from, head).await;
            blocks.push(MergeBlock {
                cid: head.0,
                block_data: head.1.clone(),
                doc_id: doc_id.to_string(),
                collection_id: COLLECTION_ID.to_string(),
                creator: "did:key:z6MkrMergedCrdt".to_string(),
                sender_peer: Some("peer".to_string()),
                is_explicit_replicator: false,
                explicit_replay_authorization: None,
                verified_creator: None,
            });
        }
        for outcome in self.handler.try_batch_merge(&blocks).await.unwrap() {
            assert!(outcome.unwrap().is_merged());
        }
    }

    async fn count(&self) -> Option<NormalValue> {
        LensedAutoCommitFetcher::new(self.db.clone())
            .get_all(COLLECTION)
            .await
            .unwrap()
            .first()
            .and_then(|doc| doc.get("count").cloned())
    }

    /// The index's entry count and how many sit at each of `values`.
    async fn entries<const N: usize>(&self, values: [i64; N]) -> (usize, [(i64, usize); N]) {
        let mut found = values.map(|value| (value, 0));
        for (value, count) in &mut found {
            *count = entries_at(&self.db, self.index_id, *value).await;
        }
        (
            index_entries(&self.db, COLLECTION, self.index_id).await,
            found,
        )
    }
}

#[test]
fn an_index_on_a_custom_increment_type_follows_the_total() {
    workers::block_on(async {
        let node = Node::open().await;
        let doc_id = node.create(1).await;
        node.increment(&doc_id, 2).await;
        node.increment(&doc_id, 2).await;

        assert_eq!(node.count().await, Some(NormalValue::Int(5)));
        assert_eq!(
            node.entries([1, 2, 3, 5]).await,
            (1, [(1, 0), (2, 0), (3, 0), (5, 1)]),
            "one entry, at the total"
        );
    })
    .unwrap()
}

#[test]
fn concurrent_increments_leave_one_entry_at_the_total() {
    workers::block_on(async {
        let node = Node::open().await;
        let doc_id = node.create(1).await;
        futures::future::join_all((0..8).map(|_| node.increment(&doc_id, 2))).await;

        assert_eq!(
            node.entries([17]).await,
            (1, [(17, 1)]),
            "one entry, at the total"
        );
    })
    .unwrap()
}

#[test]
fn merged_increments_move_the_entry_to_the_total() {
    workers::block_on(async {
        let (alice, bob) = (Node::open().await, Node::open().await);
        let doc_id = alice.create(1).await;
        bob.receive(&alice, &doc_id, &alice.head(&doc_id).await)
            .await;
        for _ in 0..2 {
            alice.increment(&doc_id, 2).await;
            bob.receive(&alice, &doc_id, &alice.head(&doc_id).await)
                .await;
        }

        assert_eq!(bob.count().await, Some(NormalValue::Int(5)));
        assert_eq!(
            bob.entries([1, 3, 5]).await,
            (1, [(1, 0), (3, 0), (5, 1)]),
            "one entry, at the merged total"
        );
    })
    .unwrap()
}

#[test]
fn a_batch_of_merged_increments_moves_the_entry_to_the_total() {
    workers::block_on(async {
        let (alice, bob) = (Node::open().await, Node::open().await);
        let doc_id = alice.create(1).await;
        let mut heads = vec![alice.head(&doc_id).await];
        for _ in 0..2 {
            alice.increment(&doc_id, 2).await;
            heads.push(alice.head(&doc_id).await);
        }
        bob.receive_batch(&alice, &doc_id, &heads).await;

        assert_eq!(bob.count().await, Some(NormalValue::Int(5)));
        assert_eq!(
            bob.entries([1, 3, 5]).await,
            (1, [(1, 0), (3, 0), (5, 1)]),
            "one entry, at the merged total"
        );
    })
    .unwrap()
}
