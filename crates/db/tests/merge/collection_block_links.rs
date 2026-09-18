//! A collection block names the documents in a branchable collection's
//! verifiable history. Merging one walks its links, then writes the new
//! collection head.
//!
//! The walk tolerates failure: a link the node does not hold is skipped, and a
//! linked composite whose merge returns an error is logged at debug and
//! skipped. Either way the head is written and the outcome is a *terminal*
//! skip, which replication discharges as merged. The document the block named
//! is then never merged and never retried.

use std::sync::Arc;

use blockstore::{Blockstore as _, DefraBlockstore};
use cid::Cid;
use db::database::DB;
use db::merge::merge_handler::DbMergeHandler;
use defra_core::block::{
    Block, CollectionDeltaPayload, CompositeDeltaPayload, CrdtDelta, DAGLink, LwwDeltaPayload,
};
use defra_core::merge::{BlockMetadata, MergeHandler, MergeOutcome};
use document::NormalValue;
use schema::{CollectionVersion, FieldDescription, FieldKind};
use storage::corekv::{IterOptions, Store};
use storage::RegolithStore;

const COLLECTION_ID: &str = "col-ledger";

type Handler = DbMergeHandler<RegolithStore, DefraBlockstore<RegolithStore>>;

async fn node() -> (
    Arc<RegolithStore>,
    Handler,
    Arc<DefraBlockstore<RegolithStore>>,
) {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let db = Arc::new(DB::from_arc(store.clone()).unwrap());
    db.create_collection(
        CollectionVersion::new(
            "Ledger",
            COLLECTION_ID,
            COLLECTION_ID,
            vec![
                FieldDescription::new("1", "_docID", FieldKind::doc_id()),
                FieldDescription::new("2", "body", FieldKind::string()),
            ],
        )
        .as_branchable(),
    )
    .await
    .unwrap();
    let blockstore = Arc::new(DefraBlockstore::new(store.clone(), false));
    let handler = DbMergeHandler::new(db, blockstore.clone());
    (store, handler, blockstore)
}

/// A genesis composite setting `body`, with its field block.
fn document(value: &str) -> (Cid, Vec<(Cid, Vec<u8>)>) {
    let field = Block::new(
        CrdtDelta::Lww(LwwDeltaPayload {
            field_name: "body".to_string(),
            priority: 1,
            schema_version_id: COLLECTION_ID.to_string(),
            data: db::block::builder::encode_value_as_cbor(&NormalValue::String(value.to_string()))
                .unwrap(),
        }),
        vec![],
        vec![],
    );
    let field_cid = field.generate_cid().unwrap();
    let composite = Block::new(
        CrdtDelta::Composite(CompositeDeltaPayload {
            schema_version_id: COLLECTION_ID.to_string(),
            priority: 1,
            status: 1,
        }),
        vec![],
        vec![DAGLink::new("body", field_cid)],
    );
    let composite_cid = composite.generate_cid().unwrap();
    (
        composite_cid,
        vec![
            (field_cid, field.to_dag_cbor().unwrap()),
            (composite_cid, composite.to_dag_cbor().unwrap()),
        ],
    )
}

/// A collection block linking `documents`.
fn collection_block(documents: &[Cid]) -> (Cid, Vec<u8>) {
    let block = Block::new(
        CrdtDelta::Collection(CollectionDeltaPayload {
            schema_version_id: COLLECTION_ID.to_string(),
            priority: 1,
        }),
        vec![],
        documents
            .iter()
            .map(|cid| DAGLink::new("_head", *cid))
            .collect(),
    );
    let cid = block.generate_cid().unwrap();
    (cid, block.to_dag_cbor().unwrap())
}

async fn merge(handler: &Handler, cid: &Cid, bytes: &[u8]) -> MergeOutcome {
    handler
        .handle_block(
            cid,
            bytes,
            BlockMetadata::normal("", COLLECTION_ID, "peer-did", Some("peer"), false),
        )
        .await
        .unwrap()
}

/// Every collection-head key in the store, so a failure shows what landed.
async fn collection_heads(store: &Arc<RegolithStore>) -> Vec<String> {
    let txn = store.new_txn(true).await.unwrap();
    let mut iter = txn.iterator(IterOptions::new()).await.unwrap();
    let mut keys = Vec::new();
    while let Some(pair) = iter.next().await.unwrap() {
        let key = String::from_utf8_lossy(&pair.key).into_owned();
        if key.contains("/c/") {
            keys.push(key);
        }
    }
    keys
}

#[tokio::test]
async fn a_collection_block_whose_link_is_not_held_installs_no_head() {
    let (store, handler, blockstore) = node().await;
    // The composite is authored but never delivered, so the link is missing.
    let (composite_cid, _) = document("first");
    let (cid, bytes) = collection_block(&[composite_cid]);
    blockstore.put(&cid, &bytes).await.unwrap();

    let outcome = merge(&handler, &cid, &bytes).await;

    assert!(
        !matches!(outcome, MergeOutcome::Skipped { terminal: true, .. }),
        "a block whose named document was never merged must not be discharged \
         as merged: {outcome:?}"
    );
    assert!(
        collection_heads(&store).await.is_empty(),
        "no head may be installed for a history this node does not hold"
    );
}

#[tokio::test]
async fn a_collection_block_whose_links_are_held_merges_and_installs_its_head() {
    let (store, handler, blockstore) = node().await;
    let (composite_cid, blocks) = document("first");
    for (block_cid, block_bytes) in &blocks {
        blockstore.put(block_cid, block_bytes).await.unwrap();
    }
    let (cid, bytes) = collection_block(&[composite_cid]);
    blockstore.put(&cid, &bytes).await.unwrap();

    let outcome = merge(&handler, &cid, &bytes).await;

    assert_eq!(outcome, MergeOutcome::Merged);
    let heads = collection_heads(&store).await;
    assert_eq!(heads.len(), 1, "{heads:?}");
    assert!(heads[0].contains(&cid.to_string()), "{heads:?}");
}
