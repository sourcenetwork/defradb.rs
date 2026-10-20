//! The node's in-memory copy of a DiskANN pending list: searches use it where
//! it covers their snapshot, catch it up from the pending log, and scan where
//! it may not cover them.

use std::sync::Arc;

use crate::diskann_streaming::{fresh, merge_due, trained_store, value};
use db::index::vector::engine::diskann::{
    codec, CodeCaches, Copy, PendingCache, Segment, Tag, MAX_PENDING_CACHE_BYTES,
};
use db::index::vector::index::VectorIndex;
use db::index::vector::store::NodeId;
use storage::corekv::Store;
use storage::index::CollectionIndex;
use storage::RegolithStore;

const COLLECTION: u32 = 31;
const INDEX_ID: u32 = 3;

async fn cached_store() -> (RegolithStore, VectorIndex, Arc<CodeCaches>) {
    let (store, index) = trained_store().await;
    let caches = Arc::new(CodeCaches::default());
    let index = index.with_code_caches(&caches);
    (store, index, caches)
}

fn tag(caches: &CodeCaches) -> Option<Tag> {
    caches.get(COLLECTION, INDEX_ID).pending.load().tag()
}

async fn insert(store: &RegolithStore, index: &VectorIndex, id: u64, seed: u64) -> Vec<f32> {
    let vector = fresh(seed);
    let mut write = store.new_txn(false).await.unwrap();
    index.save(&mut write, id, &value(&vector)).await.unwrap();
    write.commit().await.unwrap();
    vector
}

async fn nearest(store: &RegolithStore, index: &VectorIndex, vector: &[f32]) -> u64 {
    let mut read = store.new_txn(true).await.unwrap();
    let hits = index.search(&mut read, vector, 1, None).await.unwrap();
    read.discard();
    hits[0].id.0
}

#[test]
fn later_inserts_catch_the_copy_up_without_a_reload() {
    workers::block_on(async {
        let (store, index, caches) = cached_store().await;
        let first = insert(&store, &index, 60_001, 0x61).await;
        assert_eq!(nearest(&store, &index, &first).await, 60_001);
        let loaded = tag(&caches).expect("the first search loads the copy");
        assert_eq!(loaded.position, loaded.loaded_at);

        let second = insert(&store, &index, 60_002, 0x62).await;
        assert_eq!(nearest(&store, &index, &second).await, 60_002);
        let caught_up = tag(&caches).unwrap();
        assert_eq!(
            caught_up.loaded_at, loaded.loaded_at,
            "caught up, not reloaded"
        );
        assert_eq!(caught_up.position, loaded.position + 1);
        assert_eq!(nearest(&store, &index, &first).await, 60_001);
    })
    .unwrap()
}

#[test]
fn a_transaction_finds_its_own_uncommitted_insert() {
    workers::block_on(async {
        let (store, index, caches) = cached_store().await;
        let warm = insert(&store, &index, 61_001, 0x71).await;
        nearest(&store, &index, &warm).await;
        let before = tag(&caches);

        let mut write = store.new_txn(false).await.unwrap();
        let own = fresh(0x72);
        index.save(&mut write, 61_002, &value(&own)).await.unwrap();
        let hits = index.search(&mut write, &own, 1, None).await.unwrap();
        assert_eq!(hits[0].id.0, 61_002, "its own pending write is visible");
        write.discard();
        assert_eq!(
            tag(&caches),
            before,
            "a scan for own writes publishes nothing"
        );
    })
    .unwrap()
}

#[test]
fn a_snapshot_from_before_a_delete_still_finds_the_vector() {
    workers::block_on(async {
        let (store, index, caches) = cached_store().await;
        let doomed = insert(&store, &index, 62_001, 0x81).await;
        let mut old = store.new_txn(true).await.unwrap();

        let mut write = store.new_txn(false).await.unwrap();
        index
            .delete(&mut write, 62_001, &value(&doomed))
            .await
            .unwrap();
        write.commit().await.unwrap();

        assert_ne!(nearest(&store, &index, &doomed).await, 62_001);
        let loaded = tag(&caches).expect("loaded after the delete");

        let hits = index.search(&mut old, &doomed, 1, None).await.unwrap();
        assert_eq!(
            hits[0].id.0, 62_001,
            "the older snapshot still has it pending"
        );
        assert_eq!(
            tag(&caches),
            Some(loaded),
            "an older snapshot never republishes"
        );
    })
    .unwrap()
}

#[test]
fn a_snapshot_behind_the_copy_ignores_what_it_cannot_see() {
    workers::block_on(async {
        let (store, index, caches) = cached_store().await;
        let early = insert(&store, &index, 63_001, 0x91).await;
        nearest(&store, &index, &early).await;
        let mut old = store.new_txn(true).await.unwrap();

        let late = insert(&store, &index, 63_002, 0x92).await;
        assert_eq!(nearest(&store, &index, &late).await, 63_002);
        assert!(tag(&caches).unwrap().position > tag(&caches).unwrap().loaded_at);

        let hits = index.search(&mut old, &late, 3, None).await.unwrap();
        assert!(
            hits.iter().all(|hit| hit.id.0 != 63_002),
            "not in its snapshot"
        );
        let hits = index.search(&mut old, &early, 1, None).await.unwrap();
        assert_eq!(hits[0].id.0, 63_001);
    })
    .unwrap()
}

#[test]
fn a_merge_starts_a_new_copy() {
    workers::block_on(async {
        let (store, index, caches) = cached_store().await;
        let merged = insert(&store, &index, 64_001, 0xA1).await;
        nearest(&store, &index, &merged).await;
        let before = tag(&caches).unwrap();

        assert!(merge_due(&store, &index).await);
        let pending = insert(&store, &index, 64_002, 0xA2).await;
        assert_eq!(nearest(&store, &index, &pending).await, 64_002);
        assert_eq!(nearest(&store, &index, &merged).await, 64_001);
        let after = tag(&caches).unwrap();
        assert_eq!(after.generation, before.generation + 1);
        assert!(
            after.loaded_at > before.position,
            "reloaded at the new generation"
        );
    })
    .unwrap()
}

#[test]
fn a_merge_truncates_the_log_it_absorbed() {
    workers::block_on(async {
        let (store, index, _caches) = cached_store().await;
        insert(&store, &index, 65_001, 0xB1).await;
        insert(&store, &index, 65_002, 0xB2).await;
        assert!(merge_due(&store, &index).await);

        let log = storage::keys::datastore::vector_pending_log(COLLECTION, INDEX_ID, 0);
        let read = store.new_txn(true).await.unwrap();
        let mut iter = read
            .iterator(
                storage::corekv::IterOptions::default().with_prefix(log.entry_prefix().to_vec()),
            )
            .await
            .unwrap();
        assert!(
            iter.next().await.unwrap().is_none(),
            "no entry outlives the merge"
        );
        assert!(
            read.get(log.head_key()).await.unwrap().is_some(),
            "the head stays"
        );
    })
    .unwrap()
}

#[test]
fn a_newer_copy_wins_and_an_oversized_one_drops_the_cache() {
    let cache = PendingCache::default();
    let copy = |generation, position, ids: &[u64]| {
        let mut segment = Segment::default();
        for id in ids {
            segment.push(NodeId(*id), &[0]);
        }
        let tag = Tag {
            generation,
            loaded_at: 0,
            position,
        };
        Arc::new(Copy::loaded(tag, segment))
    };
    cache.publish(copy(1, 5, &[1]));
    cache.publish(copy(1, 4, &[2]));
    assert_eq!(
        cache.load().tag().unwrap().position,
        5,
        "an older position loses"
    );
    cache.publish(copy(0, 9, &[3]));
    assert_eq!(
        cache.load().tag().unwrap().generation,
        1,
        "an older generation loses"
    );
    cache.publish(copy(2, 1, &[4]));
    assert_eq!(cache.load().tag().unwrap().generation, 2);

    let mut held = Vec::new();
    cache.load().for_each(1, |id, _| held.push(id));
    assert_eq!(held, vec![NodeId(4)]);

    let mut big = Segment::default();
    big.push(NodeId(5), &vec![0; MAX_PENDING_CACHE_BYTES]);
    let tag = Tag {
        generation: 3,
        loaded_at: 0,
        position: 0,
    };
    cache.publish(Arc::new(Copy::loaded(tag, big)));
    assert_eq!(cache.load().tag(), None, "past the cap nothing is cached");
}

#[test]
fn log_entries_round_trip() {
    for entry in [
        codec::LogEntry::Insert(NodeId(7), vec![1, 2, 3]),
        codec::LogEntry::Delete(NodeId(u64::MAX)),
    ] {
        let bytes = codec::encode_log_entry(&entry);
        assert_eq!(codec::decode_log_entry(&bytes).unwrap(), entry);
    }
    assert!(codec::decode_log_entry(&[]).is_err());
    assert!(codec::decode_log_entry(&[0, 1, 2]).is_err());
    assert!(
        codec::decode_log_entry(&[0; 10]).is_err(),
        "a delete carries no code"
    );
}
