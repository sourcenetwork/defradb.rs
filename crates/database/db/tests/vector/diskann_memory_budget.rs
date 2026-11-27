//! The node's budget for what vector indexes keep in memory: what the caches
//! hold never passes it, a search returns the same hits under any budget, and
//! a reload gets the room the codes it replaces held
//! (`proofs/tla/VectorMemoryBudget.tla`).

use std::sync::Arc;

use crate::diskann_engine::{corpus, params, CORPUS, K, SEED};
use db::index::vector::budget::{Charge, MemoryBudget};
use db::index::vector::engine::ann::{Neighbor, VectorIndexEngine};
use db::index::vector::engine::diskann::codec;
use db::index::vector::engine::diskann::{
    CodeCache, CodeCaches, Codes, Copy, DiskAnn, DiskAnnParams, PendingCache, Segment, Tag,
};
use db::index::vector::store::{MemoryNodeStore, NodeId, VectorNodeStore};
use defra_core::vector::Metric;

async fn resident(vectors: &[Vec<f32>]) -> DiskAnn<MemoryNodeStore> {
    let params = DiskAnnParams {
        resident_codes: true,
        ..params()
    };
    let mut index = DiskAnn::try_new(MemoryNodeStore::new(), Metric::Cosine, params, SEED).unwrap();
    for (i, vector) in vectors.iter().enumerate() {
        index.insert(NodeId(i as u64 + 1), vector).await.unwrap();
    }
    index.build().await.unwrap();
    index
}

async fn hits(index: &DiskAnn<MemoryNodeStore>, queries: &[Vec<f32>]) -> Vec<Vec<Neighbor>> {
    let mut all = Vec::new();
    for query in queries {
        all.push(index.search(query.as_slice(), K, None).await.unwrap());
    }
    all
}

fn resident_codes(cache: &CodeCache) -> usize {
    (1..=CORPUS as u64)
        .filter(|id| cache.get(NodeId(*id)).is_some())
        .count()
}

#[test]
fn a_charge_past_the_limit_is_refused_and_a_dropped_one_given_back() {
    let budget = MemoryBudget::new(100);
    let held = budget.charge(60).unwrap();
    assert!(budget.charge(41).is_none(), "41 more do not fit beside 60");
    assert_eq!(budget.refusals(), 1);
    let rest = budget.charge(40).unwrap();
    assert_eq!(budget.used(), 100);

    drop(held);
    assert_eq!(budget.used(), 40);
    rest.release();
    assert_eq!(budget.used(), 0);
    drop(rest);
    assert_eq!(
        budget.used(),
        0,
        "a released charge gives nothing back twice"
    );
}

#[test]
fn settling_gives_back_the_excess_or_takes_what_is_missing() {
    let budget = MemoryBudget::new(100);
    let mut charge = Charge::empty(&budget);
    assert!(
        charge.cover(10, 64),
        "a cover takes a whole step where it fits"
    );
    assert_eq!(charge.bytes(), 64);
    assert!(
        charge.cover(90, 64),
        "and only what is missing where it does not"
    );
    assert_eq!(charge.bytes(), 90);

    assert!(charge.settle(30));
    assert_eq!(budget.used(), 30);
    assert!(charge.settle(100));
    assert!(!charge.settle(101));
    assert_eq!(budget.used(), 100);
    assert_eq!(budget.refusals(), 2);
}

/// Every worker shares one budget: charges taken at once from many threads
/// never pass it, and every byte comes back.
#[test]
fn concurrent_charges_never_pass_the_limit() {
    let limit = 1_000;
    let budget = MemoryBudget::new(limit);
    std::thread::scope(|scope| {
        for _ in 0..8 {
            let budget = budget.clone();
            scope.spawn(move || {
                let mut held = Vec::new();
                for round in 0..2_000u64 {
                    if let Some(charge) = budget.charge(1 + round % 7) {
                        assert!(budget.used() <= limit);
                        held.push(charge);
                    }
                    if round % 3 == 0 {
                        held.pop();
                    }
                }
            });
        }
    });
    assert_eq!(budget.used(), 0);
}

/// The budget decides what stays in memory, never what a search returns: a
/// search with no room, some room, and room for every code ranks the same
/// nodes at the same distances.
#[test]
fn a_search_returns_the_same_hits_under_any_budget() {
    workers::block_on(async {
        let (vectors, queries) = corpus();
        let mut index = resident(&vectors).await;
        let unlimited = Arc::new(CodeCache::default());
        index = index.with_codes(unlimited.clone());
        let want = hits(&index, &queries).await;
        let everything = unlimited.resident_bytes();
        assert_eq!(resident_codes(&unlimited), CORPUS);

        for limit in [0, everything / 3, everything] {
            let budget = MemoryBudget::new(limit);
            let cache = Arc::new(CodeCache::with_budget(budget.clone()));
            index = index.with_codes(cache.clone());

            assert_eq!(hits(&index, &queries).await, want, "budget {limit}");
            assert!(budget.used() <= limit, "budget {limit}");
            assert!(cache.resident_bytes() <= limit, "budget {limit}");
        }
    })
    .unwrap()
}

#[test]
fn a_budget_with_room_for_some_codes_keeps_some() {
    workers::block_on(async {
        let (vectors, queries) = corpus();
        let unlimited = Arc::new(CodeCache::default());
        let mut index = resident(&vectors).await.with_codes(unlimited.clone());
        hits(&index, &queries[..1]).await;
        let everything = unlimited.resident_bytes();

        let none = Arc::new(CodeCache::with_budget(MemoryBudget::new(0)));
        index = index.with_codes(none.clone());
        hits(&index, &queries).await;
        assert_eq!(resident_codes(&none), 0, "no room keeps nothing");

        let budget = MemoryBudget::new(everything / 2);
        let half = Arc::new(CodeCache::with_budget(budget.clone()));
        index = index.with_codes(half.clone());
        hits(&index, &queries).await;
        let kept = resident_codes(&half);
        assert!(kept > 0 && kept < CORPUS, "kept {kept} of {CORPUS}");
    })
    .unwrap()
}

/// A merge makes a new generation; the reload drops the old codes before it
/// takes new ones, so a budget with room for one generation holds the new
/// one whole.
#[test]
fn a_reload_gets_the_room_the_old_codes_held() {
    workers::block_on(async {
        let (vectors, _) = corpus();
        let unlimited = Arc::new(CodeCache::default());
        let mut index = resident(&vectors).await.with_codes(unlimited.clone());
        index.search(vectors[0].as_slice(), K, None).await.unwrap();
        let one_generation = unlimited.resident_bytes();

        let budget = MemoryBudget::new(one_generation * 3 / 2);
        let cache = Arc::new(CodeCache::with_budget(budget.clone()));
        index = index.with_codes(cache.clone());
        index.search(vectors[0].as_slice(), K, None).await.unwrap();
        assert_eq!(resident_codes(&cache), CORPUS);

        let moved = crate::diskann_streaming::fresh(0x7E57);
        index.insert(NodeId(3), &moved).await.unwrap();
        index.build().await.unwrap();
        let found = index.search(moved.as_slice(), K, None).await.unwrap();

        assert_eq!(found[0].id, NodeId(3));
        assert_eq!(
            resident_codes(&cache),
            CORPUS,
            "the new generation is whole"
        );
        assert!(budget.used() <= budget.limit());
    })
    .unwrap()
}

/// A reload stops reading the index after the one code that first does not
/// fit, rather than visiting every code in storage to discard them.
#[test]
fn a_reload_stops_reading_codes_when_the_budget_is_full() {
    workers::block_on(async {
        let (vectors, queries) = corpus();
        let unlimited = Arc::new(CodeCache::default());
        let params = DiskAnnParams {
            resident_codes: true,
            ..params()
        };
        let mut index = DiskAnn::try_new(
            super::diskann_engine::Counting::new(),
            Metric::Cosine,
            params,
            SEED,
        )
        .unwrap()
        .with_codes(unlimited.clone());
        for (i, vector) in vectors.iter().enumerate() {
            index.insert(NodeId(i as u64 + 1), vector).await.unwrap();
        }
        index.build().await.unwrap();
        index.search(queries[0].as_slice(), K, None).await.unwrap();
        let everything = unlimited.resident_bytes();

        let budget = MemoryBudget::new(everything / 2);
        let cache = Arc::new(CodeCache::with_budget(budget.clone()));
        let before = index.store().code_visits();
        let mut reload = cache.begin_reload(1).expect("the cache starts empty");
        let mut id = 0;
        index
            .store()
            .iterate_aux_while(codec::CODE, b"", |_key, code| {
                id += 1;
                reload.insert(NodeId(id), code)
            })
            .await
            .unwrap();
        reload.finish();
        let visited = index.store().code_visits() - before;
        let kept = resident_codes(&cache);

        assert!(kept > 0 && kept < CORPUS, "kept {kept} of {CORPUS}");
        assert!(
            visited <= kept + 1,
            "reload visited {visited} codes with room for {kept}"
        );
        assert!(budget.used() <= budget.limit());
    })
    .unwrap()
}

fn codes(count: u64) -> Codes {
    let mut codes = Codes::default();
    for id in 1..=count {
        codes.insert(NodeId(id), &id.to_le_bytes()).unwrap();
    }
    codes
}

/// Searches that find a reload running read codes from storage rather than
/// start one of their own; a reload that stops hands its generation back.
#[test]
fn one_search_reloads_a_generation_at_a_time() {
    let cache = CodeCache::default();
    let first = cache.begin_reload(1).expect("the first search reloads");
    assert!(cache.begin_reload(1).is_none(), "a second waits it out");

    drop(first);
    let again = cache
        .begin_reload(1)
        .expect("a dropped reload hands it back");
    again.finish();

    assert!(cache.is_current(1));
    assert!(
        cache.begin_reload(1).is_none(),
        "current codes need no reload"
    );
}

/// A code reload gives back the index's pending-copy room before it takes its
/// own codes.
#[test]
fn a_code_reload_clears_the_pending_copy() {
    let budget = MemoryBudget::new(1 << 20);
    let cache = CodeCache::with_budget(budget.clone());
    cache.pending.publish(Arc::new(Copy::loaded(
        Tag {
            generation: 1,
            position: 10,
        },
        segment(1, 10),
    )));
    assert!(budget.used() > 0);

    let reload = cache.begin_reload(2).unwrap();
    assert_eq!(cache.pending.load().tag(), None);
    assert_eq!(budget.used(), 0);
    reload.finish();
}

/// Concurrent misses for one code charge it once and leave one code resident.
#[test]
fn concurrent_misses_charge_a_code_once() {
    const THREADS: usize = 16;
    const INSERTS: usize = 10_000;

    let budget = MemoryBudget::new(1 << 20);
    let cache = Arc::new(CodeCache::with_budget(budget.clone()));
    let start = Arc::new(regolith::sync::Barrier::new(THREADS));
    std::thread::scope(|scope| {
        for _ in 0..THREADS {
            let cache = Arc::clone(&cache);
            let start = Arc::clone(&start);
            scope.spawn(move || {
                workers::block_on(start.wait()).unwrap();
                for _ in 0..INSERTS {
                    cache.insert(NodeId(1), vec![1; 8]);
                }
            });
        }
    });

    let expected = MemoryBudget::new(1 << 20);
    let expected_cache = CodeCache::with_budget(expected.clone());
    expected_cache.insert(NodeId(1), vec![1; 8]);
    assert_eq!(budget.used(), expected.used());
    assert_eq!(cache.get(NodeId(1)), Some(vec![1; 8]));
}

/// A reload of a newer generation takes the claim over; the older one's codes
/// are never published over it, and its room comes back.
#[test]
fn a_reload_taken_over_does_not_publish() {
    let budget = MemoryBudget::unlimited();
    let cache = CodeCache::with_budget(budget.clone());
    let mut older = cache.begin_reload(1).unwrap();
    let _ = older.insert(NodeId(1), &[1; 8]).unwrap();
    let mut newer = cache
        .begin_reload(2)
        .expect("a newer generation takes over");

    older.finish();
    assert!(!cache.is_current(1), "the older reload published nothing");
    let _ = newer.insert(NodeId(1), &[2; 8]).unwrap();
    newer.finish();

    assert!(cache.is_current(2));
    assert_eq!(cache.get(NodeId(1)), Some(vec![2; 8]));
    assert_eq!(budget.used(), cache.resident_bytes());
}

/// While a reload runs no missed code is kept, so the misses cannot take the
/// room the reload needs; once it is done, a miss is kept where there is room.
#[test]
fn a_miss_is_kept_only_with_room_and_no_reload_running() {
    let cache = CodeCache::with_budget(MemoryBudget::new(1 << 10));
    let reload = cache.begin_reload(1).unwrap();
    cache.insert(NodeId(7), vec![7; 8]);
    assert_eq!(cache.get(NodeId(7)), None);
    reload.finish();

    cache.insert(NodeId(7), vec![7; 8]);
    assert_eq!(cache.get(NodeId(7)), Some(vec![7; 8]));
    cache.insert(NodeId(8), vec![8; 4 << 10]);
    assert_eq!(cache.get(NodeId(8)), None, "past the budget it is not kept");
}

/// Codes replaced in one piece are charged as allocated, and only kept when
/// they fit.
#[test]
fn replaced_codes_are_charged_as_allocated() {
    let budget = MemoryBudget::unlimited();
    let cache = CodeCache::with_budget(budget.clone());
    let loaded = codes(1_000);
    cache.replace(1, loaded);
    assert!(budget.used() >= 1_000 * 8, "every code's bytes are charged");
    assert_eq!(budget.used(), cache.resident_bytes());

    let tight = CodeCache::with_budget(MemoryBudget::new(1_000));
    tight.replace(1, codes(1_000));
    assert!(tight.is_current(1));
    assert_eq!(
        tight.get(NodeId(1)),
        None,
        "codes that do not fit are dropped"
    );
    assert_eq!(tight.budget().used(), 0);
}

fn segment(base: u64, count: u64) -> Segment {
    let mut segment = Segment::default();
    for at in base..base + count {
        segment.push(at, NodeId(at), &[0; 8]);
    }
    segment
}

/// A copy of a pending list the budget has no room for is left unpublished so
/// a later search can try again; a copy caught up shares its segments.
#[test]
fn a_pending_copy_is_kept_only_where_the_budget_has_room() {
    let tag = |position| Tag {
        generation: 1,
        position,
    };
    let refused = PendingCache::with_budget(MemoryBudget::new(0));
    refused.publish(Arc::new(Copy::loaded(tag(10), segment(1, 10))));
    assert!(!refused.load().is_oversized());
    assert_eq!(refused.load().tag(), None);

    let budget = MemoryBudget::unlimited();
    let cache = PendingCache::with_budget(budget.clone());
    let first = segment(1, 10);
    let first_bytes = first.bytes() as u64;
    cache.publish(Arc::new(Copy::loaded(tag(10), first)));
    assert!(!cache.load().is_oversized());
    assert_eq!(budget.used(), first_bytes);

    let added = segment(11, 5);
    let added_bytes = added.bytes() as u64;
    let caught_up = cache.load().caught_up(15, added);
    cache.publish(Arc::new(caught_up));
    kovan::flush();
    assert_eq!(budget.used(), first_bytes + added_bytes);
}

/// A caught-up pending copy replaces the current copy's charge, so folding a
/// full set of segments does not need room for both copies at once.
#[test]
fn a_folded_pending_copy_replaces_the_room_it_held() {
    let tag = |position| Tag {
        generation: 1,
        position,
    };
    let current = Arc::new(Copy::loaded(tag(33), segment(1, 33)));
    let next = Arc::new(current.caught_up(34, segment(34, 1)));
    let limit = current.resident_bytes().max(next.resident_bytes()) as u64;
    assert!((current.resident_bytes() + next.resident_bytes()) as u64 > limit);

    let budget = MemoryBudget::new(limit);
    let cache = PendingCache::with_budget(budget.clone());
    cache.publish(Arc::clone(&current));
    cache.publish(Arc::clone(&next));

    assert!(!cache.load().is_oversized());
    assert_eq!(cache.load().tag(), Some(tag(34)));
    assert_eq!(budget.used(), next.resident_bytes() as u64);
}

/// A temporary budget refusal does not mark the generation oversized and stop
/// every later search from trying to build a copy.
#[test]
fn a_pending_copy_refused_by_the_budget_does_not_poison_the_generation() {
    let cache = PendingCache::with_budget(MemoryBudget::new(0));
    cache.publish(Arc::new(Copy::loaded(
        Tag {
            generation: 1,
            position: 10,
        },
        segment(1, 10),
    )));

    assert!(!cache.load().is_oversized());
    assert_eq!(cache.load().tag(), None);
}

/// Evicting an index, or every index of a collection, gives its budget back.
#[test]
fn evicting_vector_caches_returns_their_room() {
    let budget = MemoryBudget::new(1 << 20);
    let caches = CodeCaches::new(budget.clone());
    let first = caches.get(1, 1);
    first.replace(1, codes(100));
    let second = caches.get(2, 1);
    second.replace(1, codes(100));
    assert!(budget.used() > 0);

    drop(first);
    caches.evict(1, 1);
    kovan::flush();
    let after_index = budget.used();
    assert!(after_index > 0);

    drop(second);
    caches.evict_collection(2);
    kovan::flush();
    assert_eq!(budget.used(), 0);
}

/// Dropping the last handle to an index cache releases its charges.
#[test]
fn dropping_an_index_cache_returns_its_room() {
    let budget = MemoryBudget::new(1 << 20);
    let cache = CodeCaches::new(budget.clone()).get(1, 1);
    cache.replace(1, codes(100));
    assert!(budget.used() > 0);

    drop(cache);
    kovan::flush();
    assert_eq!(budget.used(), 0);
}
