//! The retention floor at merge (`go.shinzo.retention`): a replicated
//! document at or below its collection's floor, or with no valid height, is
//! refused; deletes, local writes and branchable collections are not judged.

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

use blockstore::{Blockstore as _, DefraBlockstore};
use db::database::{DbOptions, DB};
use db::merge::merge_handler::DbMergeHandler;
use db::retention::{RetentionFloor, RetentionRule, StaticRetentionRule};
use defra_core::block::{Block, CompositeDeltaPayload, CrdtDelta};
use defra_core::merge::{BlockMetadata, MergeBlock, MergeHandler, MergeOutcome};
use document::{DocID, Document, NormalValue};
use rapidhash::{HashMapExt, RapidHashMap};
use schema::{CollectionVersion, FieldDescription, FieldKind};
use storage::RegolithStore;

pub(super) type Handler = DbMergeHandler<RegolithStore, DefraBlockstore<RegolithStore>>;

const BELOW: &str =
    "replicated document is at or below the retention floor: refused by the retention rule";
const NO_HEIGHT: &str =
    "replicated document has no valid retention height: refused by the retention rule";

pub(super) fn logs(collection_id: &str) -> CollectionVersion {
    CollectionVersion::new(
        "Log",
        "v1",
        collection_id,
        vec![
            FieldDescription::new(
                "1",
                "_docID",
                FieldKind::doc_id(),
                schema::CType::LwwRegister,
            ),
            FieldDescription::new(
                "2",
                "address",
                FieldKind::string(),
                schema::CType::LwwRegister,
            ),
            FieldDescription::new(
                "3",
                "blockNumber",
                FieldKind::int(),
                schema::CType::LwwRegister,
            ),
        ],
    )
}

/// A handler whose node judges `col-log` against `floor` on `blockNumber`.
async fn node(
    schema: CollectionVersion,
    floor: Option<i64>,
) -> (Handler, Arc<DefraBlockstore<RegolithStore>>) {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let mut options = DbOptions::new();
    if let Some(floor) = floor {
        let mut floors = RapidHashMap::new();
        floors.insert(
            "col-log".to_string(),
            RetentionFloor {
                field: "blockNumber".to_string(),
                floor,
            },
        );
        options = options.with_retention_rule(Arc::new(StaticRetentionRule::new(floors)));
    }
    let db = Arc::new(DB::from_arc_with_options(store.clone(), options).unwrap());
    db.create_collection(schema).await.unwrap();
    let blockstore = Arc::new(DefraBlockstore::new(store, false));
    (DbMergeHandler::new(db, blockstore.clone()), blockstore)
}

/// A replicated create of a Log whose `blockNumber` is `height`, or unset.
pub(super) async fn log(
    blockstore: &Arc<DefraBlockstore<RegolithStore>>,
    schema: &CollectionVersion,
    address: &str,
    height: Option<i64>,
) -> MergeBlock {
    let mut doc = Document::new(schema.clone());
    doc.set("address", NormalValue::String(address.to_string()))
        .unwrap();
    if let Some(height) = height {
        doc.set("blockNumber", NormalValue::Int(height)).unwrap();
    }
    let built = db::block::builder::build_blocks_from_document(
        &storage::Crdts::default(),
        &doc,
        schema,
        blockstore,
    )
    .await
    .unwrap();
    MergeBlock {
        cid: built.cid,
        block_data: built.block,
        doc_id: built.doc_id,
        collection_id: schema.collection_id.clone(),
        creator: "did:key:z6MkrRetentionTest".to_string(),
        sender_peer: Some("peer1".to_string()),
        is_explicit_replicator: false,
        explicit_replay_authorization: None,
        verified_creator: None,
    }
}

pub(super) async fn merge(handler: &Handler, block: &MergeBlock) -> MergeOutcome {
    let metadata = BlockMetadata::normal(
        &block.doc_id,
        &block.collection_id,
        &block.creator,
        block.sender_peer.as_deref(),
        false,
    );
    handler
        .handle_block(&block.cid, &block.block_data, metadata)
        .await
        .unwrap()
}

pub(super) async fn stored(handler: &Handler, collection_id: &str, doc_id: &str) -> bool {
    let collection = handler
        .db()
        .find_collection_by_id(collection_id)
        .unwrap()
        .unwrap();
    let txn = handler.db().new_txn(true).await.unwrap();
    let doc = {
        let datastore = txn.datastore().unwrap();
        let systemstore = txn.systemstore().unwrap();
        collection
            .get_by_doc_id(
                &datastore,
                &systemstore,
                &DocID::from_string(doc_id).unwrap(),
            )
            .await
            .unwrap()
    };
    txn.force_discard().unwrap();
    doc.is_some()
}

fn rejected(reason: &str) -> MergeOutcome {
    MergeOutcome::rejected(reason)
}

#[test]
#[mantra_macros::req_test("go.shinzo.retention.rule")]
fn the_merge_refuses_what_the_rule_refuses() {
    workers::block_on(async {
        let schema = logs("col-log");
        let (handler, blockstore) = node(schema.clone(), Some(100)).await;
        for (address, height, outcome) in [
            ("at", Some(100), rejected(BELOW)),
            ("above", Some(101), MergeOutcome::Merged),
            ("unset", None, rejected(NO_HEIGHT)),
            ("negative", Some(-5), rejected(NO_HEIGHT)),
        ] {
            let block = log(&blockstore, &schema, address, height).await;
            assert_eq!(merge(&handler, &block).await, outcome, "{address}");
            assert_eq!(
                stored(&handler, "col-log", &block.doc_id).await,
                outcome == MergeOutcome::Merged,
                "{address}"
            );
        }
    })
    .unwrap()
}

#[test]
#[mantra_macros::req_test("go.shinzo.retention.rule")]
fn a_floor_of_zero_refuses_nothing_by_height() {
    workers::block_on(async {
        let schema = logs("col-log");
        let (handler, blockstore) = node(schema.clone(), Some(0)).await;
        let zero = log(&blockstore, &schema, "zero", Some(0)).await;
        assert_eq!(merge(&handler, &zero).await, MergeOutcome::Merged);
        let unset = log(&blockstore, &schema, "unset", None).await;
        assert_eq!(merge(&handler, &unset).await, rejected(NO_HEIGHT));
    })
    .unwrap()
}

#[test]
#[mantra_macros::req_test("go.shinzo.retention.rule")]
fn an_uncovered_collection_or_a_node_without_a_rule_stores_everything() {
    workers::block_on(async {
        let uncovered = logs("col-other");
        let (handler, blockstore) = node(uncovered.clone(), Some(100)).await;
        let block = log(&blockstore, &uncovered, "low", Some(1)).await;
        assert_eq!(merge(&handler, &block).await, MergeOutcome::Merged);

        let schema = logs("col-log");
        let (handler, blockstore) = node(schema.clone(), None).await;
        let block = log(&blockstore, &schema, "low", Some(1)).await;
        assert_eq!(merge(&handler, &block).await, MergeOutcome::Merged);
    })
    .unwrap()
}

#[test]
#[mantra_macros::req_test("go.shinzo.retention.scope")]
fn a_branchable_collection_is_not_judged() {
    workers::block_on(async {
        let schema = logs("col-log").as_branchable();
        let (handler, blockstore) = node(schema.clone(), Some(100)).await;
        let block = log(&blockstore, &schema, "low", Some(1)).await;
        assert_eq!(merge(&handler, &block).await, MergeOutcome::Merged);
    })
    .unwrap()
}

#[test]
#[mantra_macros::req_test("go.shinzo.retention.scope")]
fn the_floor_resolves_for_a_covered_collection_only() {
    let mut floors = RapidHashMap::new();
    floors.insert(
        "col-log".to_string(),
        RetentionFloor {
            field: "blockNumber".to_string(),
            floor: 100,
        },
    );
    let options = DbOptions::new().with_retention_rule(Arc::new(StaticRetentionRule::new(floors)));
    assert_eq!(
        options.retention_floor(&logs("col-log")),
        Some(RetentionFloor {
            field: "blockNumber".to_string(),
            floor: 100,
        })
    );
    assert_eq!(
        options.retention_floor(&logs("col-log").as_branchable()),
        None
    );
    assert_eq!(options.retention_floor(&logs("col-unknown")), None);
    assert_eq!(DbOptions::new().retention_floor(&logs("col-log")), None);
}

#[test]
#[mantra_macros::req_test("go.shinzo.retention.batch")]
fn a_refusal_in_a_batch_refuses_only_that_document() {
    workers::block_on(async {
        let schema = logs("col-log");
        let (handler, blockstore) = node(schema.clone(), Some(100)).await;
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
            [MergeOutcome::Merged, rejected(BELOW), MergeOutcome::Merged]
        );
        assert!(stored(&handler, "col-log", &blocks[0].doc_id).await);
        assert!(!stored(&handler, "col-log", &blocks[1].doc_id).await);
        assert!(stored(&handler, "col-log", &blocks[2].doc_id).await);
    })
    .unwrap()
}

/// A rule whose floor the test raises after the document is stored.
struct RaisedFloor(AtomicI64);

impl RetentionRule for RaisedFloor {
    fn retention_floor(&self, collection_id: &str) -> Option<RetentionFloor> {
        (collection_id == "col-log").then(|| RetentionFloor {
            field: "blockNumber".to_string(),
            floor: self.0.load(Ordering::SeqCst),
        })
    }
}

/// A Log merged at height 5 while the floor was 0; once the floor is 100, a
/// replicated delete of it merges, since deleted documents are not judged.
#[test]
#[mantra_macros::req_test("go.shinzo.retention.delete")]
fn a_delete_is_applied_below_the_floor() {
    workers::block_on(async {
        let schema = logs("col-log");
        let rule = Arc::new(RaisedFloor(AtomicI64::new(0)));
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let options = DbOptions::new().with_retention_rule(rule.clone());
        let db = Arc::new(DB::from_arc_with_options(store.clone(), options).unwrap());
        db.create_collection(schema.clone()).await.unwrap();
        let blockstore = Arc::new(DefraBlockstore::new(store, false));
        let handler = DbMergeHandler::new(db, blockstore.clone());

        let created = log(&blockstore, &schema, "low", Some(5)).await;
        assert_eq!(merge(&handler, &created).await, MergeOutcome::Merged);
        rule.0.store(100, Ordering::SeqCst);

        let payload = CompositeDeltaPayload {
            collection_version_id: "v1".to_string(),
            priority: 2,
            status: 2,
        };
        let delete = Block::new(
            CrdtDelta::Composite(payload.clone()),
            vec![created.cid],
            vec![],
        );
        let delete_cid = delete.generate_cid().unwrap();
        let delete_block = MergeBlock {
            cid: delete_cid,
            block_data: bytes::Bytes::from(delete.to_dag_cbor().unwrap()),
            ..created.clone()
        };
        blockstore
            .put(&delete_cid, &delete_block.block_data)
            .await
            .unwrap();

        assert_eq!(merge(&handler, &delete_block).await, MergeOutcome::Merged);
    })
    .unwrap()
}
