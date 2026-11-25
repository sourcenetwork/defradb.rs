//! A write asks for each deferred index it owes once, however many documents
//! it owes, and the asks wait as one entry per index
//! (`proofs/tla/DeferredDrainTask.tla`, INV_OneAsk and Red_SharedNote).

use std::sync::Arc;

use db::index::deferred::DrainRequests;
use db::index::IndexManager;
use db::DB;
use document::{DocID, Document, NormalValue};
use schema::{IndexKind, IndexedFieldDescription, OrderedIndexDescription};
use storage::RegolithStore;

use super::{COLLECTION, COLLECTION_SDL};
use crate::common::counting_store::CountingStore;

type Store = CountingStore<RegolithStore>;

async fn open() -> Arc<DB<Store>> {
    let db = Arc::new(
        DB::open(CountingStore::new(RegolithStore::in_memory().unwrap()))
            .await
            .unwrap(),
    );
    db.add_collection(&defra_core::RequestContext::ambient(), COLLECTION_SDL)
        .await
        .unwrap();
    db.create_index(
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
    db
}

/// Owes the index `docs` documents in one transaction through one manager,
/// and returns how many commit callbacks the transaction registered.
async fn owe(db: &DB<Store>, drains: &DrainRequests, docs: u64, commit: bool) -> usize {
    let collection = db.require_collection(COLLECTION).unwrap();
    let manager = IndexManager::from_collection(
        collection.resolved_root_id(),
        collection.schema(),
        collection.crdts(),
    )
    .unwrap()
    .with_drains(Some(drains));
    let before = db.store().on_commits();
    let txn = db.new_txn(false).await.unwrap();
    let datastore = txn.datastore().unwrap();
    for short_id in 1..=docs {
        let mut doc = Document::new(collection.schema_arc());
        doc.set_id(DocID::new_v0_from_seed(&format!("owed-{short_id}")));
        doc.set("count", NormalValue::Int(1)).unwrap();
        manager
            .on_document_update(&datastore, &doc, &doc, short_id, collection.schema())
            .await
            .unwrap();
    }
    drop(datastore);
    if commit {
        txn.commit().await.unwrap();
    } else {
        txn.discard().unwrap();
    }
    db.store().on_commits() - before
}

#[test]
fn a_batch_registers_one_drain_callback_however_many_documents_it_owes() {
    workers::block_on(async {
        let db = open().await;
        let one = owe(&db, &DrainRequests::default(), 1, true).await;
        let many = owe(&db, &DrainRequests::default(), 200, true).await;
        assert_eq!(many, one);
    })
    .unwrap();
}

#[test]
fn asks_for_one_index_wait_as_one_entry() {
    workers::block_on(async {
        let db = open().await;
        let drains = DrainRequests::default();
        owe(&db, &drains, 50, true).await;
        owe(&db, &drains, 50, true).await;
        assert_eq!(drains.len(), 1);
    })
    .unwrap();
}

#[test]
fn an_attempt_after_an_aborted_one_still_asks() {
    workers::block_on(async {
        let db = open().await;
        let drains = DrainRequests::default();
        owe(&db, &drains, 3, false).await;
        assert!(drains.is_empty());
        owe(&db, &drains, 3, true).await;
        assert_eq!(drains.len(), 1);
    })
    .unwrap();
}
