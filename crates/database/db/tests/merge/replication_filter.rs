//! The embedder's replication filter (`defra.replication_filter`):
//! given the values a replicated commit writes, it decides before anything of
//! the commit is stored.

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

use blockstore::{Blockstore as _, DefraBlockstore};
use db::database::{DbOptions, DB};
use db::merge::merge_handler::DbMergeHandler;
use db::replication_filter::{add_field_values, FieldValues, ReplicationFilter};
use defra_core::block::{Block, CompositeDeltaPayload, CrdtDelta, DAGLink, LwwDeltaPayload};
use defra_core::merge::{MergeBlock, MergeHandler, MergeOutcome};
use regolith::sync::Atom;
use storage::RegolithStore;

use super::replicated::{log, logs, merge, stored, Handler};

/// Keeps a Log only above `cutoff`, and every Log with no `blockNumber`; it
/// remembers the last collection and fields it was given.
struct Cutoff {
    cutoff: AtomicI64,
    seen: Atom<Option<FieldValues>>,
    collection: Atom<Option<(String, String)>>,
}

impl ReplicationFilter for Cutoff {
    fn allow_replication(
        &self,
        collection_id: &str,
        collection_name: &str,
        _doc_id: &str,
        fields: &FieldValues,
    ) -> bool {
        self.collection
            .store(Some((collection_id.to_owned(), collection_name.to_owned())));
        self.seen.store(Some(fields.clone()));
        match fields
            .get("blockNumber")
            .and_then(|value| value.as_integer())
        {
            Some(height) => i128::from(height) > i128::from(self.cutoff.load(Ordering::SeqCst)),
            None => true,
        }
    }
}

async fn node(
    schema: schema::CollectionVersion,
    cutoff: i64,
) -> (Handler, Arc<DefraBlockstore<RegolithStore>>, Arc<Cutoff>) {
    let filter = Arc::new(Cutoff {
        cutoff: AtomicI64::new(cutoff),
        seen: Atom::new(None),
        collection: Atom::new(None),
    });
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let options = DbOptions::new().with_replication_filter(filter.clone());
    let db = Arc::new(DB::from_arc_with_options(store.clone(), options).unwrap());
    db.create_collection(schema).await.unwrap();
    let blockstore = Arc::new(DefraBlockstore::new(store, false));
    (
        DbMergeHandler::new(db, blockstore.clone()),
        blockstore,
        filter,
    )
}

fn refused() -> MergeOutcome {
    MergeOutcome::terminal_skip("refused by the replication filter")
}

#[test]
#[mantra_macros::req_test("defra.replication_filter.field_values")]
fn the_filter_is_given_the_values_the_commit_writes() {
    workers::block_on(async {
        let schema = logs("col-log");
        let (handler, blockstore, filter) = node(schema.clone(), -1).await;
        let block = log(&blockstore, &schema, "0xabc", Some(25951718)).await;

        assert_eq!(merge(&handler, &block).await, MergeOutcome::Merged);

        let seen = (*filter.seen.load()).clone().expect("the filter was asked");
        assert_eq!(seen.len(), 2);
        assert_eq!(
            seen["blockNumber"],
            ciborium::Value::Integer(25951718u64.into())
        );
        assert_eq!(seen["address"], ciborium::Value::Text("0xabc".into()));
        assert_eq!(
            (*filter.collection.load()).clone(),
            Some(("col-log".to_owned(), "Log".to_owned())),
            "the filter is given the collection's ID and the name a Go filter keys on"
        );
    })
    .unwrap()
}

fn field_block(data: Vec<u8>) -> Block {
    Block::new(
        CrdtDelta::Lww(LwwDeltaPayload {
            field_name: "blockNumber".into(),
            priority: 1,
            collection_version_id: "v1".into(),
            data,
        }),
        vec![],
        vec![],
    )
}

#[test]
#[mantra_macros::req_test("defra.replication_filter.field_values")]
fn an_encrypted_or_undecodable_value_is_left_out() {
    let mut encoded = Vec::new();
    ciborium::into_writer(&7u64, &mut encoded).unwrap();
    let mut encrypted = field_block(encoded.clone());
    encrypted.encryption = Some(encrypted.generate_cid().unwrap());
    let mut fields = FieldValues::default();

    add_field_values(&mut fields, &encrypted);
    add_field_values(&mut fields, &field_block(vec![0xff]));
    assert!(fields.is_empty());

    add_field_values(&mut fields, &field_block(encoded));
    assert_eq!(fields["blockNumber"], ciborium::Value::Integer(7u64.into()));
}

#[test]
#[mantra_macros::req_test("defra.replication_filter.refusal")]
fn a_refused_document_is_not_stored() {
    workers::block_on(async {
        let schema = logs("col-log");
        let (handler, blockstore, filter) = node(schema.clone(), 100).await;
        let at = log(&blockstore, &schema, "at", Some(100)).await;
        assert_eq!(merge(&handler, &at).await, refused());
        assert!(!stored(&handler, "col-log", &at.doc_id).await);

        filter.cutoff.store(99, Ordering::SeqCst);
        let above = log(&blockstore, &schema, "above", Some(100)).await;
        assert_eq!(merge(&handler, &above).await, MergeOutcome::Merged);
        assert!(stored(&handler, "col-log", &above.doc_id).await);

        let unset = log(&blockstore, &schema, "unset", None).await;
        assert_eq!(merge(&handler, &unset).await, MergeOutcome::Merged);
    })
    .unwrap()
}

#[test]
#[mantra_macros::req_test("defra.replication_filter.refusal")]
fn a_refusal_in_a_batch_refuses_only_that_document() {
    workers::block_on(async {
        let schema = logs("col-log");
        let (handler, blockstore, _) = node(schema.clone(), 100).await;
        let blocks = [
            log(&blockstore, &schema, "a", Some(101)).await,
            log(&blockstore, &schema, "b", Some(100)).await,
            log(&blockstore, &schema, "c", Some(150)).await,
        ];

        let outcomes: Vec<MergeOutcome> = handler
            .handle_block_batch(&blocks)
            .await
            .into_iter()
            .map(Result::unwrap)
            .collect();

        assert_eq!(
            outcomes,
            [MergeOutcome::Merged, refused(), MergeOutcome::Merged]
        );
        assert!(stored(&handler, "col-log", &blocks[0].doc_id).await);
        assert!(!stored(&handler, "col-log", &blocks[1].doc_id).await);
        assert!(stored(&handler, "col-log", &blocks[2].doc_id).await);
    })
    .unwrap()
}

#[test]
#[mantra_macros::req_test("defra.replication_filter.branchable")]
fn a_branchable_collection_is_not_filtered() {
    workers::block_on(async {
        let schema = logs("col-log").as_branchable();
        let (handler, blockstore, _) = node(schema.clone(), 100).await;
        let block = log(&blockstore, &schema, "low", Some(1)).await;
        assert_eq!(merge(&handler, &block).await, MergeOutcome::Merged);
    })
    .unwrap()
}

/// A replicated update of `parent`'s document that sets only `address`,
/// so the filter alone would keep it.
async fn readdress(
    blockstore: &Arc<DefraBlockstore<RegolithStore>>,
    parent: &MergeBlock,
    address: &str,
) -> MergeBlock {
    let parent_block = Block::from_dag_cbor(&parent.block_data).unwrap();
    let previous = parent_block
        .links
        .iter()
        .flatten()
        .find(|link| link.name == "address")
        .unwrap()
        .link;
    let mut data = Vec::new();
    ciborium::into_writer(address, &mut data).unwrap();
    let field = Block::new(
        CrdtDelta::Lww(LwwDeltaPayload {
            field_name: "address".into(),
            priority: 2,
            collection_version_id: "v1".into(),
            data,
        }),
        vec![previous],
        vec![],
    );
    let field_cid = field.generate_cid().unwrap();
    blockstore
        .put(&field_cid, &field.to_dag_cbor().unwrap())
        .await
        .unwrap();
    let composite = Block::new(
        CrdtDelta::Composite(CompositeDeltaPayload {
            collection_version_id: "v1".into(),
            priority: 2,
            status: 1,
        }),
        vec![parent.cid],
        vec![DAGLink::new("address", field_cid)],
    );
    let mut update = parent.clone();
    update.cid = composite.generate_cid().unwrap();
    update.block_data = composite.to_dag_cbor().unwrap().into();
    blockstore
        .put(&update.cid, &update.block_data)
        .await
        .unwrap();
    update
}

#[test]
#[mantra_macros::req_test("defra.replication_filter.refusal")]
fn a_commit_after_a_refused_one_is_refused_in_the_same_walk() {
    workers::block_on(async {
        let schema = logs("col-log");
        let (handler, blockstore, _) = node(schema.clone(), 100).await;
        let create = log(&blockstore, &schema, "low", Some(1)).await;
        let update = readdress(&blockstore, &create, "moved").await;

        assert_eq!(merge(&handler, &update).await, refused());
        assert!(!stored(&handler, "col-log", &update.doc_id).await);
    })
    .unwrap()
}

#[test]
#[mantra_macros::req_test("defra.replication_filter.refusal")]
fn a_commit_after_a_refused_one_is_refused_when_pushed_later() {
    workers::block_on(async {
        let schema = logs("col-log");
        let (handler, blockstore, _) = node(schema.clone(), 100).await;
        let create = log(&blockstore, &schema, "low", Some(1)).await;
        assert_eq!(merge(&handler, &create).await, refused());

        let update = readdress(&blockstore, &create, "moved").await;
        assert_eq!(merge(&handler, &update).await, refused());
        assert!(!stored(&handler, "col-log", &update.doc_id).await);
    })
    .unwrap()
}

#[test]
#[mantra_macros::req_test("defra.replication_filter.refusal")]
fn a_commit_after_a_kept_one_is_merged() {
    workers::block_on(async {
        let schema = logs("col-log");
        let (handler, blockstore, _) = node(schema.clone(), 100).await;
        let create = log(&blockstore, &schema, "high", Some(101)).await;
        assert_eq!(merge(&handler, &create).await, MergeOutcome::Merged);

        let update = readdress(&blockstore, &create, "moved").await;
        assert_eq!(merge(&handler, &update).await, MergeOutcome::Merged);
        assert!(stored(&handler, "col-log", &update.doc_id).await);
    })
    .unwrap()
}
