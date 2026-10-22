//! The collection head set: live exactly when nothing supersedes it, whatever
//! the order the blocks arrive in, and written only by blind merges.
//!
//! The properties asserted are the ones stated in `proofs/tla/HeadSet.tla`
//! and `proofs/lean/HeadSet/Core.lean`: a head is live exactly when nothing
//! supersedes it, and two writers never conflict.

use cid::Cid;
use datastore::{NamespaceView, SharedTxn};
use db::block::heads::{collection_set, live_collection_heads, publish_collection_block};
use defra_core::block::generate_cid_from_bytes;
use std::sync::Arc;
use storage::corekv::Store;
use storage::namespace::Namespace;
use storage::RegolithStore;

const COLLECTION: u32 = 3;

fn cid_of(value: &[u8]) -> Cid {
    generate_cid_from_bytes(value).unwrap()
}

fn headstore(shared: &Arc<SharedTxn>) -> NamespaceView {
    NamespaceView::new(Arc::clone(shared), Namespace::Headstore)
}

async fn commit(shared: Arc<SharedTxn>) {
    Arc::try_unwrap(shared)
        .ok()
        .expect("views dropped")
        .into_txn()
        .commit()
        .await
        .unwrap();
}

/// Publish `cid` over `parents` in a transaction opened as `shared`.
async fn publish_in(shared: &Arc<SharedTxn>, cid: Cid, priority: u64, parents: &[Cid]) {
    let view = headstore(shared);
    let set = collection_set(&view, COLLECTION).await.unwrap();
    publish_collection_block(&view, COLLECTION, &set, cid, priority, parents)
        .await
        .unwrap();
}

async fn publish(store: &RegolithStore, cid: Cid, priority: u64, parents: &[Cid]) {
    let shared = SharedTxn::new(store.new_txn(false).await.unwrap());
    publish_in(&shared, cid, priority, parents).await;
    commit(shared).await;
}

async fn live(store: &RegolithStore) -> (Vec<Cid>, usize) {
    let shared = SharedTxn::new(store.new_txn(true).await.unwrap());
    let view = headstore(&shared);
    let mut cids = live_collection_heads(&view, COLLECTION).await.unwrap().live;
    cids.sort_by_cached_key(Cid::to_string);
    let ahead = collection_set(&view, COLLECTION)
        .await
        .unwrap()
        .ahead()
        .count();
    (cids, ahead)
}

/// A head is live exactly when no block supersedes it.
#[test]
fn superseded_heads_are_not_live() {
    workers::block_on(async {
        let store = RegolithStore::in_memory().unwrap();
        let (a, b, c) = (cid_of(b"a"), cid_of(b"b"), cid_of(b"c"));
        publish(&store, a, 1, &[]).await;
        publish(&store, b, 2, &[]).await;
        publish(&store, c, 3, &[a]).await;

        let mut expected = vec![b, c];
        expected.sort_by_cached_key(Cid::to_string);
        assert_eq!(live(&store).await, (expected, 0));
    })
    .unwrap()
}

/// Every pattern of superseded heads gives the heads nothing superseded.
#[test]
fn the_live_set_holds_for_every_supersede_pattern() {
    workers::block_on(async {
        let all: Vec<Cid> = (0..6u8).map(|i| cid_of(&[i])).collect();
        let superseder = cid_of(b"superseder");

        for pattern in 0u32..(1 << 6) {
            let store = RegolithStore::in_memory().unwrap();
            for cid in &all {
                publish(&store, *cid, 1, &[]).await;
            }
            let superseded: Vec<Cid> = all
                .iter()
                .enumerate()
                .filter(|(index, _)| pattern & (1 << index) != 0)
                .map(|(_, cid)| *cid)
                .collect();
            publish(&store, superseder, 2, &superseded).await;

            let mut expected: Vec<Cid> = all
                .iter()
                .enumerate()
                .filter(|(index, _)| pattern & (1 << index) == 0)
                .map(|(_, cid)| *cid)
                .chain([superseder])
                .collect();
            expected.sort_by_cached_key(Cid::to_string);
            assert_eq!(live(&store).await, (expected, 0), "pattern {pattern:#08b}");
        }
    })
    .unwrap()
}

/// Several blocks superseding one head is the state two concurrent appends
/// leave behind. The head is superseded once, not once per block.
#[test]
fn a_head_superseded_by_siblings_is_superseded_once() {
    workers::block_on(async {
        let store = RegolithStore::in_memory().unwrap();
        let (seed, first, second) = (cid_of(b"seed"), cid_of(b"first"), cid_of(b"second"));
        publish(&store, seed, 1, &[]).await;
        publish(&store, first, 2, &[seed]).await;
        publish(&store, second, 2, &[seed]).await;

        let mut expected = vec![first, second];
        expected.sort_by_cached_key(Cid::to_string);
        assert_eq!(live(&store).await, (expected, 0));
    })
    .unwrap()
}

/// A block replicated ahead of its parent holds the parent ahead, and the
/// parent, when it arrives, is superseded on sight and leaves nothing.
#[test]
fn a_child_before_its_parent_supersedes_it_on_arrival() {
    workers::block_on(async {
        let store = RegolithStore::in_memory().unwrap();
        let (absent, child, live_head) = (cid_of(b"absent"), cid_of(b"child"), cid_of(b"live"));
        publish(&store, live_head, 1, &[]).await;
        publish(&store, child, 2, &[absent]).await;
        let (heads, ahead) = live(&store).await;
        assert_eq!(heads.len(), 2);
        assert_eq!(ahead, 1);

        publish(&store, absent, 1, &[]).await;
        let mut expected = vec![live_head, child];
        expected.sort_by_cached_key(Cid::to_string);
        assert_eq!(live(&store).await, (expected, 0));
    })
    .unwrap()
}

/// A block is published once: replaying it after it was superseded does not
/// bring it back.
#[test]
fn a_replayed_block_stays_superseded() {
    workers::block_on(async {
        let store = RegolithStore::in_memory().unwrap();
        let (old, current) = (cid_of(b"old"), cid_of(b"current"));
        publish(&store, old, 1, &[]).await;
        publish(&store, current, 2, &[old]).await;
        publish(&store, old, 1, &[]).await;
        assert_eq!(live(&store).await, (vec![current], 0));
    })
    .unwrap()
}

/// The point of the whole design: an append that overlaps another still
/// commits, and the live set is the union.
#[test]
fn overlapping_appends_both_commit() {
    workers::block_on(async {
        let store = RegolithStore::in_memory().unwrap();
        let (old, current) = (cid_of(b"old"), cid_of(b"current"));
        publish(&store, old, 1, &[]).await;
        publish(&store, current, 2, &[old]).await;

        let mut appends = Vec::new();
        for name in [b"first".as_slice(), b"second"] {
            let shared = SharedTxn::new(store.new_txn(false).await.unwrap());
            let found = live_collection_heads(&headstore(&shared), COLLECTION)
                .await
                .unwrap();
            assert_eq!(found.live, vec![current]);
            let appended = cid_of(name);
            publish_in(&shared, appended, 3, &found.live).await;
            appends.push((shared, appended));
        }
        let mut expected = Vec::new();
        for (shared, appended) in appends {
            commit(shared).await;
            expected.push(appended);
        }
        expected.sort_by_cached_key(Cid::to_string);
        assert_eq!(live(&store).await, (expected, 0));
    })
    .unwrap()
}

/// Appends through the database keep only the live head: after any number of
/// appends the set holds one name.
#[test]
fn appends_through_the_db_store_only_the_live_head() {
    workers::block_on(async {
        const APPENDS: usize = 64;
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let db = Arc::new(db::DB::from_arc(store.clone()).unwrap());
        let segment = schema::CollectionVersion::new(
            "Segment",
            "v1",
            "col-segment",
            vec![
                schema::FieldDescription::new(
                    "1",
                    "_docID",
                    schema::FieldKind::doc_id(),
                    schema::CType::LwwRegister,
                ),
                schema::FieldDescription::new(
                    "2",
                    "body",
                    schema::FieldKind::string(),
                    schema::CType::LwwRegister,
                ),
            ],
        )
        .as_branchable();
        db.create_collection(segment.clone()).await.unwrap();
        let mutator = db::AutoCommitMutator::new(db.clone());
        for index in 0..APPENDS {
            let mut doc = document::Document::new(segment.clone());
            doc.set("body", format!("segment {index}")).unwrap();
            query::mutator::DocMutator::create(&mutator, "Segment", doc)
                .await
                .unwrap();
        }
        let collection_id = {
            let txn = db.new_txn(true).await.unwrap();
            let id = txn
                .get_collection("Segment")
                .await
                .unwrap()
                .unwrap()
                .resolved_root_id();
            txn.discard().unwrap();
            id
        };
        let shared = SharedTxn::new(store.new_txn(true).await.unwrap());
        let set = collection_set(&headstore(&shared), collection_id)
            .await
            .unwrap();
        assert_eq!(set.live().count(), 1);
        assert_eq!(set.ahead().count(), 0);
    })
    .unwrap()
}
