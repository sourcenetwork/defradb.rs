//! The node's in-memory copy of a DiskANN pending list: searches use it where
//! it covers their snapshot, catch it up from the pending log, and scan where
//! it may not cover them.

use std::sync::Arc;

use crate::diskann_streaming::{fresh, merge_due, trained_store, value};
use db::index::vector::engine::diskann::{
    codec, CodeCaches, Copy, PendingCache, Segment, Tag, ENTRY_OVERHEAD, MAX_PENDING_CACHE_BYTES,
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

        let second = insert(&store, &index, 60_002, 0x62).await;
        assert_eq!(nearest(&store, &index, &second).await, 60_002);
        let caught_up = tag(&caches).unwrap();
        assert_eq!(caught_up.generation, loaded.generation);
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
            "its own appends never reach the shared copy"
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
        let (store, index, _caches) = cached_store().await;
        let early = insert(&store, &index, 63_001, 0x91).await;
        nearest(&store, &index, &early).await;
        let mut old = store.new_txn(true).await.unwrap();

        let late = insert(&store, &index, 63_002, 0x92).await;
        assert_eq!(nearest(&store, &index, &late).await, 63_002);

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
            after.position > before.position,
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
fn a_newer_copy_wins_and_an_oversized_one_leaves_a_marker() {
    let cache = PendingCache::default();
    let copy = |generation, position, ids: &[u64]| {
        let mut segment = Segment::default();
        for (position, id) in (1..).zip(ids) {
            segment.push(position, NodeId(*id), &[0]);
        }
        let tag = Tag {
            generation,
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
    cache
        .load()
        .for_each_latest_through(1, 1, |id, _| held.push(id));
    assert_eq!(held, vec![NodeId(4)]);

    let mut big = Segment::default();
    big.push(1, NodeId(5), &vec![0; MAX_PENDING_CACHE_BYTES]);
    let tag = Tag {
        generation: 3,
        position: 0,
    };
    cache.publish(Arc::new(Copy::loaded(tag, big)));
    let marker = cache.load();
    assert!(marker.is_oversized(), "past the cap only a marker is kept");
    assert_eq!(marker.tag().unwrap().generation, 3);
    let mut held = 0;
    marker.for_each_latest_through(1, 1, |_, _| held += 1);
    assert_eq!(held, 0, "the marker holds nothing");
}

#[test]
fn evicting_drops_only_the_named_caches() {
    let caches = CodeCaches::default();
    let kept = caches.get(1, 1);
    caches.get(1, 2);
    caches.get(2, 1);
    caches.get(2, 2);

    caches.evict(1, 2);
    caches.evict_collection(2);

    assert_eq!(caches.len(), 1);
    assert!(Arc::ptr_eq(&kept, &caches.get(1, 1)));
}

#[test]
fn a_catch_up_past_the_cap_is_refused() {
    let mut segment = Segment::default();
    segment.push(1, NodeId(1), &[0; 8]);
    let tag = Tag {
        generation: 0,
        position: 1,
    };
    let copy = Copy::loaded(tag, segment);
    let per_entry = ENTRY_OVERHEAD + 8;
    let room = ((MAX_PENDING_CACHE_BYTES - per_entry) / per_entry) as u64;
    assert!(copy.can_hold(room, 8));
    assert!(
        !copy.can_hold(room + 1, 8),
        "a gap past the cap scans instead"
    );
    assert!(
        !copy.can_hold(u64::MAX, 8),
        "an overflowing gap does not fit"
    );
}

#[test]
fn an_insert_during_a_merge_stays_pending() {
    workers::block_on(async {
        let (store, index, _caches) = cached_store().await;
        insert(&store, &index, 66_001, 0xC1).await;
        let mut snapshot = store.new_txn(true).await.unwrap();
        let plan = index.merge_plan(&mut snapshot).await.unwrap().unwrap();
        snapshot.discard();

        let late = insert(&store, &index, 66_002, 0xC2).await;
        let mut write = store.new_txn(false).await.unwrap();
        index.merge(&mut write, plan).await.unwrap();
        write.commit().await.unwrap();

        assert_eq!(
            nearest(&store, &index, &late).await,
            66_002,
            "the merge truncated only what its plan read"
        );
        assert!(merge_due(&store, &index).await, "it is still due a merge");
        assert_eq!(nearest(&store, &index, &late).await, 66_002);
    })
    .unwrap()
}

#[test]
fn log_entries_round_trip() {
    let bytes = codec::encode_log_entry(NodeId(7), &[1, 2, 3]);
    assert_eq!(
        codec::decode_log_entry(&bytes, 3).unwrap(),
        (NodeId(7), [1u8, 2, 3].as_slice())
    );
    assert!(codec::decode_log_entry(&[], 0).is_err());
    assert!(codec::decode_log_entry(&[0, 1, 2], 0).is_err());
    assert!(
        codec::decode_log_entry(&bytes, 4).is_err(),
        "a code of the wrong width is refused"
    );
}

const CROWD: std::ops::Range<u64> = 80_000..80_080;
const TARGET: u64 = 90_000;

fn negated(vector: &[f32]) -> Vec<f32> {
    vector.iter().map(|x| -x).collect()
}

async fn save_all(
    store: &RegolithStore,
    index: &VectorIndex,
    ids: std::ops::Range<u64>,
    vector: &[f32],
) {
    let mut write = store.new_txn(false).await.unwrap();
    for id in ids {
        index.save(&mut write, id, &value(vector)).await.unwrap();
    }
    write.commit().await.unwrap();
}

/// More updated vectors than `l_search` once sat at the query: their old
/// codes must not crowd out the vector that is nearest now.
#[test]
fn an_updated_vector_is_scored_by_its_latest_code() {
    workers::block_on(async {
        let (store, index) = trained_store().await;
        let q = fresh(0x123456);
        save_all(&store, &index, TARGET..TARGET + 1, &q).await;
        save_all(&store, &index, CROWD, &q).await;
        save_all(&store, &index, CROWD, &negated(&q)).await;
        assert_eq!(nearest(&store, &index, &q).await, TARGET);
    })
    .unwrap()
}

#[test]
fn a_transaction_scores_its_own_update_by_its_latest_code() {
    workers::block_on(async {
        let (store, index) = trained_store().await;
        let q = fresh(0x123457);
        save_all(&store, &index, TARGET..TARGET + 1, &q).await;
        let mut write = store.new_txn(false).await.unwrap();
        for vector in [q.clone(), negated(&q)] {
            for id in CROWD {
                index.save(&mut write, id, &value(&vector)).await.unwrap();
            }
        }
        let hits = index.search(&mut write, &q, 1, None).await.unwrap();
        write.discard();
        assert_eq!(hits[0].id.0, TARGET);
    })
    .unwrap()
}

/// A copy caught up past an older snapshot's head holds codes from that
/// snapshot's future; the older search must score each vector by the code it
/// can see.
#[test]
fn a_snapshot_behind_the_copy_scores_the_codes_it_can_see() {
    workers::block_on(async {
        let (store, index, _caches) = cached_store().await;
        let q = fresh(0x123458);
        save_all(&store, &index, TARGET..TARGET + 1, &q).await;
        save_all(&store, &index, CROWD, &negated(&q)).await;
        let mut old = store.new_txn(true).await.unwrap();
        let hits = index.search(&mut old, &q, 1, None).await.unwrap();
        assert_eq!(hits[0].id.0, TARGET);

        save_all(&store, &index, CROWD, &q).await;
        nearest(&store, &index, &q).await;

        let hits = index.search(&mut old, &q, 1, None).await.unwrap();
        old.discard();
        assert_eq!(hits[0].id.0, TARGET);
    })
    .unwrap()
}

fn visible(copy: &Copy, head: u64) -> Vec<(u64, u8)> {
    let mut seen = Vec::new();
    copy.for_each_latest_through(head, 1, |id, code| seen.push((id.0, code[0])));
    seen
}

fn segment(entries: &[(u64, u64, u8)]) -> Segment {
    let mut segment = Segment::default();
    for (position, id, code) in entries {
        segment.push(*position, NodeId(*id), &[*code]);
    }
    segment
}

/// A later update marks the entry it supersedes, and an older reader of the
/// same copy still sees that entry, since the marker is past its head.
#[test]
fn a_superseded_marker_respects_the_readers_head() {
    let tag = Tag {
        generation: 0,
        position: 3,
    };
    let loaded = Copy::loaded(tag, segment(&[(1, 7, 10), (2, 8, 20), (3, 7, 11)]));
    assert_eq!(visible(&loaded, 3), vec![(8, 20), (7, 11)]);
    assert_eq!(visible(&loaded, 2), vec![(7, 10), (8, 20)]);

    let caught_up = loaded.caught_up(5, segment(&[(4, 8, 21), (5, 7, 12)]));
    assert_eq!(visible(&caught_up, 5), vec![(8, 21), (7, 12)]);
    assert_eq!(visible(&caught_up, 4), vec![(7, 11), (8, 21)]);
    assert_eq!(
        visible(&loaded, 3),
        vec![(8, 20), (7, 11)],
        "markers set by a catch-up never hide an entry from a reader behind them"
    );
}

/// Two searches catching up from one copy both mark it; whichever copy wins,
/// every reader sees one entry per vector.
#[test]
fn racing_catch_ups_agree_on_markers() {
    let tag = Tag {
        generation: 0,
        position: 1,
    };
    let loaded = Copy::loaded(tag, segment(&[(1, 7, 10)]));
    let short = loaded.caught_up(2, segment(&[(2, 7, 11)]));
    let long = loaded.caught_up(3, segment(&[(2, 7, 11), (3, 7, 12)]));
    assert_eq!(visible(&short, 2), vec![(7, 11)]);
    assert_eq!(visible(&long, 3), vec![(7, 12)]);
    assert_eq!(visible(&long, 2), vec![(7, 11)]);
    assert_eq!(visible(&loaded, 1), vec![(7, 10)]);
}

/// Past the segment cap the copy folds into one run and keeps its markers.
#[test]
fn a_folded_copy_keeps_one_entry_per_vector() {
    let tag = Tag {
        generation: 0,
        position: 1,
    };
    let mut copy = Copy::loaded(tag, segment(&[(1, 7, 0)]));
    for position in 2..=40u64 {
        copy = copy.caught_up(
            position,
            segment(&[(position, 7 + position % 2, position as u8)]),
        );
    }
    assert_eq!(visible(&copy, 40), vec![(8, 39), (7, 40)]);
    assert_eq!(visible(&copy, 10), vec![(8, 9), (7, 10)]);
}

/// Clearing an index (a truncate, a drop) restarts its pending log, so a copy
/// caught up before the clear must not stand in for the new log.
#[test]
fn a_cleared_index_does_not_search_its_old_copy() {
    workers::block_on(async {
        let (store, index, _caches) = cached_store().await;
        let before = insert(&store, &index, 60_101, 0x71).await;
        assert_eq!(nearest(&store, &index, &before).await, 60_101);

        let mut clear = store.new_txn(false).await.unwrap();
        index.remove_all(&mut clear).await.unwrap();
        clear.commit().await.unwrap();

        let after = insert(&store, &index, 60_102, 0x72).await;
        let mut read = store.new_txn(true).await.unwrap();
        let hits = index.search(&mut read, &after, 1, None).await.unwrap();
        read.discard();
        assert_eq!(hits.first().map(|hit| hit.id.0), Some(60_102));
    })
    .unwrap()
}
