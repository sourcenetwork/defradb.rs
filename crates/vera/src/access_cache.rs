use kovan_map::HopscotchMap;
use rapidhash::fast::RandomState;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

#[derive(Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    actor_did: String,
    policy_id: String,
    resource: String,
    doc_id: String,
    permission: String,
}

struct CachedDecision {
    allowed: OnceLock<bool>,
    cached_at: Instant,
    pending: AtomicUsize,
}

const RETIRED: usize = usize::MAX;

pub(crate) struct PendingDecision<'a> {
    entry: Arc<CachedDecision>,
    cache: &'a AccessCache,
    key: CacheKey,
}

impl PendingDecision<'_> {
    pub(crate) fn complete(self, allowed: bool) {
        // An invalidated entry stays detached; completion never reinserts it.
        let _ = self.entry.allowed.set(allowed);
    }
}

impl Drop for PendingDecision<'_> {
    fn drop(&mut self) {
        let previous = self
            .entry
            .pending
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                Some(if count == 1 && self.entry.allowed.get().is_none() {
                    RETIRED
                } else {
                    count - 1
                })
            })
            .expect("pending decision owns a reservation");
        if previous == 1 && self.entry.allowed.get().is_none() {
            self.cache
                .entries
                .remove_if(&self.key, |entry| Arc::ptr_eq(entry, &self.entry));
        }
    }
}

/// In-memory cache for ACP access decisions.
///
/// Caches the result of `verify_access` calls keyed by
/// `(actor, policy, resource, doc_id, permission)`. Entries expire
/// after a configurable TTL. Relationship mutations eagerly invalidate
/// all entries for their policy so indirect grants cannot remain cached.
pub(crate) struct AccessCache {
    ttl: Duration,
    entries: HopscotchMap<CacheKey, Arc<CachedDecision>, RandomState>,
}

fn cache_key(
    actor_did: &str,
    policy_id: &str,
    resource: &str,
    doc_id: &str,
    permission: &str,
) -> CacheKey {
    CacheKey {
        actor_did: actor_did.to_string(),
        policy_id: policy_id.to_string(),
        resource: resource.to_string(),
        doc_id: doc_id.to_string(),
        permission: permission.to_string(),
    }
}

impl AccessCache {
    pub(crate) fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            entries: HopscotchMap::with_hasher(RandomState::default()),
        }
    }

    pub(crate) fn get(
        &self,
        actor_did: &str,
        policy_id: &str,
        resource: &str,
        doc_id: &str,
        permission: &str,
    ) -> Option<bool> {
        let key = cache_key(actor_did, policy_id, resource, doc_id, permission);
        let entry = self.entries.get(&key)?;
        if entry.cached_at.elapsed() >= self.ttl {
            None
        } else {
            entry.allowed.get().copied()
        }
    }

    pub(crate) fn begin_check(
        &self,
        actor_did: &str,
        policy_id: &str,
        resource: &str,
        doc_id: &str,
        permission: &str,
    ) -> PendingDecision<'_> {
        let key = cache_key(actor_did, policy_id, resource, doc_id, permission);
        let entry = loop {
            let fresh = Arc::new(CachedDecision {
                allowed: OnceLock::new(),
                cached_at: Instant::now(),
                pending: AtomicUsize::new(0),
            });
            let entry = if self.ttl.is_zero() {
                fresh
            } else {
                self.entries.get_or_insert(key.clone(), fresh)
            };
            if !self.ttl.is_zero() && entry.cached_at.elapsed() >= self.ttl {
                self.entries
                    .remove_if(&key, |current| Arc::ptr_eq(current, &entry));
                continue;
            }
            // Last-caller cleanup retires an empty entry before removing it.
            // A racing check must reserve a new entry rather than revive it.
            if entry
                .pending
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                    count.checked_add(1).filter(|next| *next != RETIRED)
                })
                .is_ok()
            {
                break entry;
            }
            self.entries
                .remove_if(&key, |current| Arc::ptr_eq(current, &entry));
        };
        PendingDecision {
            entry,
            cache: self,
            key,
        }
    }

    #[cfg(test)]
    pub(crate) fn set(
        &self,
        actor_did: &str,
        policy_id: &str,
        resource: &str,
        doc_id: &str,
        permission: &str,
        allowed: bool,
    ) {
        self.begin_check(actor_did, policy_id, resource, doc_id, permission)
            .complete(allowed);
    }

    /// Invalidate ALL cached decisions for a specific document.
    ///
    /// Used for remote registration, archive, and unarchive events. Local
    /// mutations invalidate the whole policy to include inherited grants.
    pub(crate) fn invalidate_object(&self, policy_id: &str, resource: &str, doc_id: &str) -> usize {
        let stale: Vec<CacheKey> = self
            .entries
            .keys()
            .filter(|key| {
                key.policy_id == policy_id && key.resource == resource && key.doc_id == doc_id
            })
            .collect();
        let count = stale.len();
        for key in &stale {
            self.entries.force_remove(key);
        }
        count
    }

    pub(crate) fn invalidate_policy(&self, policy_id: &str) -> usize {
        let stale: Vec<CacheKey> = self
            .entries
            .keys()
            .filter(|key| key.policy_id == policy_id)
            .collect();
        let count = stale.len();
        for key in &stale {
            self.entries.force_remove(key);
        }
        count
    }

    pub(crate) fn clear(&self) -> usize {
        let count = self.entries.len();
        self.entries.clear();
        count
    }
}

#[cfg(test)]
#[path = "../tests/unit/access_cache_pending.rs"]
mod pending_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_hit_returns_stored_value() {
        let cache = AccessCache::new(Duration::from_secs(300));
        cache.set("did:key:alice", "policy1", "users", "doc1", "read", true);

        assert_eq!(
            cache.get("did:key:alice", "policy1", "users", "doc1", "read"),
            Some(true)
        );
    }

    #[test]
    fn cache_miss_returns_none() {
        let cache = AccessCache::new(Duration::from_secs(300));

        assert_eq!(
            cache.get("did:key:alice", "policy1", "users", "doc1", "read"),
            None
        );
    }

    #[test]
    fn expired_entry_returns_none() {
        let cache = AccessCache::new(Duration::from_millis(1));
        cache.set("did:key:alice", "policy1", "users", "doc1", "read", true);

        std::thread::sleep(Duration::from_millis(5));

        assert_eq!(
            cache.get("did:key:alice", "policy1", "users", "doc1", "read"),
            None
        );
    }

    #[test]
    fn invalidate_object_clears_all_entries_for_document() {
        let cache = AccessCache::new(Duration::from_secs(300));
        cache.set("did:key:alice", "p1", "users", "doc1", "read", true);
        cache.set("did:key:alice", "p1", "users", "doc1", "update", true);
        cache.set("did:key:bob", "p1", "users", "doc1", "read", false);

        cache.invalidate_object("p1", "users", "doc1");

        assert_eq!(
            cache.get("did:key:alice", "p1", "users", "doc1", "read"),
            None
        );
        assert_eq!(
            cache.get("did:key:alice", "p1", "users", "doc1", "update"),
            None
        );
        assert_eq!(
            cache.get("did:key:bob", "p1", "users", "doc1", "read"),
            None
        );
    }

    #[test]
    fn invalidate_object_preserves_other_documents() {
        let cache = AccessCache::new(Duration::from_secs(300));
        cache.set("did:key:alice", "p1", "users", "doc1", "read", true);
        cache.set("did:key:alice", "p1", "users", "doc2", "read", true);

        cache.invalidate_object("p1", "users", "doc1");

        assert_eq!(
            cache.get("did:key:alice", "p1", "users", "doc1", "read"),
            None
        );
        assert_eq!(
            cache.get("did:key:alice", "p1", "users", "doc2", "read"),
            Some(true)
        );
    }

    #[test]
    fn invalidate_object_uses_exact_key_components() {
        let cache = AccessCache::new(Duration::from_secs(300));
        cache.set("did:key:alice", "p1", "users", "doc1", "read", true);
        cache.set("did:key:alice", "p1", "users|doc1", "other", "read", true);

        cache.invalidate_object("p1", "users", "doc1");

        assert_eq!(
            cache.get("did:key:alice", "p1", "users|doc1", "other", "read"),
            Some(true)
        );
    }

    #[test]
    fn invalidate_policy_preserves_other_policies() {
        let cache = AccessCache::new(Duration::from_secs(300));
        cache.set("did:key:alice", "p1", "users", "doc1", "read", true);
        cache.set("did:key:alice", "p2", "users", "doc1", "read", true);

        assert_eq!(cache.invalidate_policy("p1"), 1);
        assert_eq!(
            cache.get("did:key:alice", "p1", "users", "doc1", "read"),
            None
        );
        assert_eq!(
            cache.get("did:key:alice", "p2", "users", "doc1", "read"),
            Some(true)
        );
    }

    #[test]
    fn clear_removes_every_entry() {
        let cache = AccessCache::new(Duration::from_secs(300));
        cache.set("did:key:alice", "p1", "users", "doc1", "read", true);
        cache.set("did:key:bob", "p2", "books", "doc2", "update", true);

        assert_eq!(cache.clear(), 2);
        assert_eq!(
            cache.get("did:key:alice", "p1", "users", "doc1", "read"),
            None
        );
        assert_eq!(
            cache.get("did:key:bob", "p2", "books", "doc2", "update"),
            None
        );
    }
}
