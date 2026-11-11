//! A reindex walk racing the drain, and a drop cleanup in pages.

use datastore::BasicTxn;
use document::{Document, NormalValue};
use rapidhash::RapidHashMap;
use storage::corekv::IterOptions;
use storage::keys::{index_pending_log, index_position_prefix, IndexDataStoreKey, IndexedField};
use storage::RegolithStore;

use super::drain::drain_index;
use super::drain::tests::{create, increment, indexed, NAME};
use crate::database::DB;
use crate::index::IndexManager;

async fn keys(db: &DB<RegolithStore>, prefix: Vec<u8>) -> Vec<Vec<u8>> {
    let txn = BasicTxn::new(db.store().as_ref(), 0, true).await.unwrap();
    let mut iter = txn
        .datastore()
        .iterator(
            IterOptions::default()
                .with_prefix(prefix)
                .with_keys_only(true),
        )
        .await
        .unwrap();
    let mut keys = Vec::new();
    while let Some(pair) = iter.next().await.unwrap() {
        keys.push(pair.key);
    }
    iter.close().await.unwrap();
    keys
}

fn at(short_id: u32, index_id: u32, value: i64) -> Vec<u8> {
    IndexDataStoreKey::new(
        short_id,
        index_id,
        vec![IndexedField::ascending(NormalValue::Int(value))],
    )
    .try_bytes()
    .unwrap()
}

async fn drain(db: &DB<RegolithStore>, index_id: u32) {
    while drain_index(db, NAME, index_id).await.unwrap() {}
}

fn manager(db: &DB<RegolithStore>, short_id: u32) -> IndexManager {
    let collection = db.require_collection(NAME).unwrap();
    IndexManager::from_indexes(
        short_id,
        collection.schema(),
        collection.write_indexes(),
        collection.crdts(),
    )
    .unwrap()
}

/// The reindex's clear: the entries go and the records stay.
async fn clear(db: &DB<RegolithStore>, short_id: u32) {
    let txn = BasicTxn::new(db.store().as_ref(), 0, false).await.unwrap();
    manager(db, short_id)
        .get_index("by_count")
        .unwrap()
        .remove_page(&mut txn.datastore(), None, 1024)
        .await
        .unwrap();
    txn.commit().await.unwrap();
}

/// The walk's write of the one document at `count`, not yet committed.
async fn walk(db: &DB<RegolithStore>, short_id: u32, index_id: u32, count: i64) -> BasicTxn {
    let doc_short_id = only_record(db, short_id, index_id).await;
    let collection = db.require_collection(NAME).unwrap();
    let mut doc = Document::new(collection.schema_arc());
    doc.set("count", NormalValue::Int(count)).unwrap();
    let txn = BasicTxn::new(db.store().as_ref(), 0, false).await.unwrap();
    manager(db, short_id)
        .reindex_document(
            &txn.datastore(),
            &txn.systemstore(),
            doc_short_id,
            &doc,
            collection.schema(),
            &RapidHashMap::default(),
        )
        .await
        .unwrap();
    txn
}

async fn only_record(db: &DB<RegolithStore>, short_id: u32, index_id: u32) -> u64 {
    let records = keys(db, index_position_prefix(short_id, index_id)).await;
    let [record] = records.as_slice() else {
        panic!("one document, one record: {records:?}");
    };
    u64::from_be_bytes(record[record.len() - 8..].try_into().unwrap())
}

/// Clear, a drain moves the entry, another increment, then the walk: the
/// entry the drain wrote goes with the walk.
#[test]
fn a_walk_after_a_drain_leaves_no_orphan() {
    workers::block_on(async {
        let (db, short_id, index_id) = indexed().await;
        let id = create(&db, 5).await;
        clear(&db, short_id).await;
        increment(&db, &id).await;
        drain(&db, index_id).await;
        assert_eq!(keys(&db, at(short_id, index_id, 6)).await.len(), 1);
        increment(&db, &id).await;

        walk(&db, short_id, index_id, 7)
            .await
            .commit()
            .await
            .unwrap();
        drain(&db, index_id).await;

        assert!(keys(&db, at(short_id, index_id, 6)).await.is_empty());
        assert_eq!(keys(&db, at(short_id, index_id, 7)).await.len(), 1);
    })
    .unwrap();
}

/// A drain that moves the record while the walk is open makes the walk run again.
#[test]
fn a_walk_that_read_a_moved_record_does_not_commit() {
    workers::block_on(async {
        let (db, short_id, index_id) = indexed().await;
        let id = create(&db, 5).await;
        clear(&db, short_id).await;
        increment(&db, &id).await;
        drain(&db, index_id).await;

        let walking = walk(&db, short_id, index_id, 6).await;
        increment(&db, &id).await;
        drain(&db, index_id).await;

        assert!(walking.commit().await.is_err());
    })
    .unwrap();
}

/// Each drop page truncates at most its limit of log entries and the next
/// one carries on after it.
#[test]
fn a_drop_page_truncates_at_most_its_limit() {
    workers::block_on(async {
        let (db, short_id, index_id) = indexed().await;
        let id = create(&db, 0).await;
        for _ in 0..3 {
            increment(&db, &id).await;
        }
        drain(&db, index_id).await;
        for _ in 0..5 {
            increment(&db, &id).await;
        }
        let log = index_pending_log(short_id, index_id)
            .entry_prefix()
            .to_vec();

        let mut after = None;
        let mut left = Vec::new();
        loop {
            let txn = BasicTxn::new(db.store().as_ref(), 0, false).await.unwrap();
            let (removed, last) =
                super::remove_page(&txn.datastore(), short_id, index_id, after.as_deref(), 2)
                    .await
                    .unwrap();
            txn.commit().await.unwrap();
            left.push(keys(&db, log.clone()).await.len());
            if removed < 2 {
                break;
            }
            after = last;
        }

        assert_eq!(left, [3, 1, 0]);
        assert!(keys(&db, index_position_prefix(short_id, index_id))
            .await
            .is_empty());
    })
    .unwrap();
}
