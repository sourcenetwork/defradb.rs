//! The node's budget for what vector indexes keep in memory: what the caches
//! hold never passes it, a search returns the same hits under any budget, and
//! a reload gets the room the codes it replaces held
//! (`proofs/tla/VectorMemoryBudget.tla`).

use std::sync::Arc;

use crate::diskann_engine::{corpus, params, CORPUS, K, SEED};
use db::index::vector::budget::{Charge, MemoryBudget};
use db::index::vector::engine::ann::{Neighbor, VectorIndexEngine};
use db::index::vector::engine::diskann::{
    CodeCache, Codes, Copy, DiskAnn, DiskAnnParams, PendingCache, Segment, Tag,
};
use db::index::vector::store::{MemoryNodeStore, NodeId};
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

/// A reload of a newer generation takes the claim over; the older one's codes
/// are never published over it, and its room comes back.
#[test]
fn a_reload_taken_over_does_not_publish() {
    let budget = MemoryBudget::unlimited();
    let cache = CodeCache::with_budget(budget.clone());
    let mut older = cache.begin_reload(1).unwrap();
    older.insert(NodeId(1), &[1; 8]).unwrap();
    let mut newer = cache
        .begin_reload(2)
        .expect("a newer generation takes over");

    older.finish();
    assert!(!cache.is_current(1), "the older reload published nothing");
    newer.insert(NodeId(1), &[2; 8]).unwrap();
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

/// A copy of a pending list the budget has no room for is kept as a marker,
/// and its searches scan the log; a copy caught up shares its segments, each
/// charged once.
#[test]
fn a_pending_copy_is_kept_only_where_the_budget_has_room() {
    let tag = |position| Tag {
        generation: 1,
        position,
    };
    let refused = PendingCache::with_budget(MemoryBudget::new(0));
    refused.publish(Arc::new(Copy::loaded(tag(10), segment(1, 10))));
    assert!(refused.load().is_oversized());

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
