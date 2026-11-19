//! The lensed fetchers' generation cache: one immutable snapshot per
//! generation, read without a lock, never holding a value computed under an
//! older generation.

use std::sync::Arc;

use super::{GenerationCache, Snapshot};

fn value(tag: u32) -> Arc<u32> {
    Arc::new(tag)
}

fn snapshot(cache: &GenerationCache<u32>) -> (u64, usize) {
    cache
        .snapshot
        .peek(|snapshot| (snapshot.generation, snapshot.contexts.len()))
}

#[test]
fn a_value_is_published_only_at_its_generation_or_after_an_older_one() {
    let cache = GenerationCache::default();
    assert!(cache.publish(1, "a", &value(1)));
    assert!(cache.publish(1, "b", &value(2)));
    assert_eq!(snapshot(&cache), (1, 2));

    assert!(cache.publish(2, "c", &value(3)));
    assert_eq!(
        snapshot(&cache),
        (2, 1),
        "the values of an older generation are dropped whole"
    );
    assert!(
        !cache.publish(1, "a", &value(4)),
        "a value computed under an older generation is never published"
    );
    assert_eq!(snapshot(&cache), (2, 1));
    assert!(cache.get(1, "c").is_none());
    assert_eq!(cache.get(2, "c").as_deref(), Some(&3));
}

#[test]
fn a_published_snapshot_never_changes() {
    let one = Snapshot::default().with(1, "a", &value(1)).unwrap();
    let both = one.with(1, "b", &value(2)).unwrap();
    assert_eq!(one.contexts.len(), 1);
    assert_eq!(both.contexts.len(), 2);
    assert!(both.with(0, "c", &value(3)).is_none());
}

/// A cache whose generations are its own: invalidating drops every value and
/// refuses one computed before the invalidation, which a cleared map would
/// have taken back in.
#[test]
fn invalidate_drops_every_value_and_refuses_a_stale_one() {
    let cache = GenerationCache::default();
    let before = cache.generation();
    assert!(cache.publish(before, "a", &value(1)));
    let computing_at = cache.generation();

    cache.invalidate();

    assert_eq!(cache.generation(), before + 1);
    assert!(cache.get(before, "a").is_none());
    assert!(cache.get(cache.generation(), "a").is_none());
    assert!(
        !cache.publish(computing_at, "b", &value(2)),
        "a value computed before the invalidation must not be cached after it"
    );
    assert_eq!(snapshot(&cache), (before + 1, 0));

    assert!(cache.publish(cache.generation(), "b", &value(3)));
    assert_eq!(cache.get(before + 1, "b").as_deref(), Some(&3));
}

#[test]
fn concurrent_publishes_at_one_generation_all_land() {
    let cache = Arc::new(GenerationCache::default());
    let threads: Vec<_> = (0..8u32)
        .map(|thread| {
            let cache = Arc::clone(&cache);
            std::thread::spawn(move || {
                for index in 0..16u32 {
                    let tag = thread * 16 + index;
                    assert!(cache.publish(0, &tag.to_string(), &value(tag)));
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    assert_eq!(snapshot(&cache), (0, 128));
}
