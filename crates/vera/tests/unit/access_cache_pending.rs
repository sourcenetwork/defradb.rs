use super::*;

#[test]
fn invalidation_prevents_in_flight_checks_from_restoring_grants() {
    for invalidate in [
        |cache: &AccessCache| cache.invalidate_object("policy", "files", "doc"),
        |cache: &AccessCache| cache.invalidate_policy("policy"),
        |cache: &AccessCache| cache.clear(),
    ] {
        let cache = AccessCache::new(Duration::from_secs(60));
        let pending = cache.begin_check("alice", "policy", "files", "doc", "read");
        assert_eq!(cache.get("alice", "policy", "files", "doc", "read"), None);
        assert_eq!(invalidate(&cache), 1);
        pending.complete(true);
        assert_eq!(cache.get("alice", "policy", "files", "doc", "read"), None);

        cache
            .begin_check("alice", "policy", "files", "doc", "read")
            .complete(true);
        assert_eq!(
            cache.get("alice", "policy", "files", "doc", "read"),
            Some(true)
        );
    }
}

#[test]
fn late_completion_does_not_overwrite_a_new_check() {
    let cache = AccessCache::new(Duration::from_secs(60));
    let old = cache.begin_check("alice", "policy", "files", "doc", "read");
    cache.invalidate_policy("policy");
    cache
        .begin_check("alice", "policy", "files", "doc", "read")
        .complete(false);
    old.complete(true);
    assert_eq!(
        cache.get("alice", "policy", "files", "doc", "read"),
        Some(false)
    );
}

#[test]
fn cache_lifetime_starts_before_the_check_completes() {
    let cache = AccessCache::new(Duration::from_secs(60));
    let pending = PendingDecision {
        entry: Arc::new(CachedDecision {
            allowed: OnceLock::new(),
            cached_at: Instant::now() - Duration::from_secs(61),
            pending: AtomicUsize::new(1),
        }),
        cache: &cache,
        key: cache_key("alice", "policy", "files", "doc", "read"),
    };
    cache.entries.insert(
        cache_key("alice", "policy", "files", "doc", "read"),
        Arc::clone(&pending.entry),
    );
    pending.complete(true);
    assert_eq!(cache.get("alice", "policy", "files", "doc", "read"), None);
}

#[test]
fn zero_ttl_never_caches_a_grant() {
    let cache = AccessCache::new(Duration::ZERO);
    cache
        .begin_check("alice", "policy", "files", "doc", "read")
        .complete(true);
    assert_eq!(cache.get("alice", "policy", "files", "doc", "read"), None);
}

#[test]
fn failed_and_cancelled_checks_leave_no_entries() {
    let cache = AccessCache::new(Duration::from_secs(60));
    let pending = cache.begin_check("alice", "policy", "files", "doc", "read");
    assert_eq!(cache.entries.len(), 1);
    drop(pending);
    assert_eq!(cache.entries.len(), 0);
}

#[test]
fn concurrent_checks_share_the_reservation_and_keep_the_first_result() {
    let cache = AccessCache::new(Duration::from_secs(60));
    let first = cache.begin_check("alice", "policy", "files", "doc", "read");
    let second = cache.begin_check("alice", "policy", "files", "doc", "read");
    assert!(Arc::ptr_eq(&first.entry, &second.entry));
    first.complete(true);
    assert_eq!(
        cache.get("alice", "policy", "files", "doc", "read"),
        Some(true)
    );
    drop(second);
    assert_eq!(
        cache.get("alice", "policy", "files", "doc", "read"),
        Some(true)
    );
}

#[test]
fn cancellation_keeps_other_pending_checks_and_removes_only_the_last() {
    let cache = AccessCache::new(Duration::from_secs(60));
    let first = cache.begin_check("alice", "policy", "files", "doc", "read");
    let second = cache.begin_check("alice", "policy", "files", "doc", "read");
    drop(first);
    assert_eq!(cache.entries.len(), 1);
    second.complete(true);
    assert_eq!(
        cache.get("alice", "policy", "files", "doc", "read"),
        Some(true)
    );
    cache.clear();
    let first = cache.begin_check("alice", "policy", "files", "doc", "read");
    let second = cache.begin_check("alice", "policy", "files", "doc", "read");
    drop(first);
    drop(second);
    assert_eq!(cache.entries.len(), 0);
}

#[test]
fn cancelled_old_check_cannot_remove_a_replacement() {
    let cache = AccessCache::new(Duration::from_secs(60));
    let old = cache.begin_check("alice", "policy", "files", "doc", "read");
    cache.invalidate_policy("policy");
    let current = cache.begin_check("alice", "policy", "files", "doc", "read");
    drop(old);
    current.complete(true);
    assert_eq!(
        cache.get("alice", "policy", "files", "doc", "read"),
        Some(true)
    );
}
