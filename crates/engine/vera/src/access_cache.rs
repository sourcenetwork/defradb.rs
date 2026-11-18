use kovan_map::HopscotchMap;
use kovan_queue::seg_queue::SegQueue;
use rapidhash::fast::RandomState;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use regolith::sync::OnceCell;
use std::time::{Duration, Instant};

/// The bytes cached decisions may take, counted by their keys.
const DECISION_BYTES: usize = 8 << 20;

/// What one cached decision costs beyond its key's strings: the map slot,
/// the shared entry and its place in the eviction order.
const DECISION_OVERHEAD: usize = 128;

/// The bytes cached registrations may take.
const REGISTRATION_BYTES: usize = 2 << 20;

/// When the eviction order holds this many records per cached decision
/// (plus slack), records of decisions already gone are dropped.
const ORDER_SLACK: usize = 64;

#[derive(Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    actor_did: String,
    policy_id: String,
    resource: String,
    doc_id: String,
    permission: String,
}

struct CachedDecision {
    allowed: OnceCell<bool>,
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
        if self.entry.allowed.set(allowed).is_ok()
            && self
                .cache
                .entries
                .get(&self.key)
                .is_some_and(|current| Arc::ptr_eq(&current, &self.entry))
        {
            self.cache.admit(&self.key);
        }
    }
}

impl Drop for PendingDecision<'_> {
    #[allow(deprecated, reason = "try_update requires Rust 1.95; MSRV is 1.91")]
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
///
/// Settled decisions are bounded by bytes: past the budget the oldest are
/// evicted. Denials, and the registration lookups that make an unregistered
/// document public, are cached only once invalidation events reach this
/// cache ([`receive_events`](Self::receive_events)): a cached denial delays
/// seeing a grant, as a cached allow delays seeing a revocation, each bounded
/// by the TTL and cut short by the events (D36).
pub(crate) struct AccessCache {
    ttl: Duration,
    entries: HopscotchMap<CacheKey, Arc<CachedDecision>, RandomState>,
    /// Settled decisions in the order they were cached.
    order: SegQueue<CacheKey>,
    bytes: AtomicUsize,
    budget: usize,
    events: AtomicBool,
    registrations: inputs::Cache<RegistrationKey, bool>,
    origin: Instant,
}

/// The document a registration lookup is about.
#[derive(Clone, PartialEq, Eq, Hash)]
struct RegistrationKey {
    policy_id: String,
    resource: String,
    doc_id: String,
}

/// The bytes a cached decision is charged.
fn charge(key: &CacheKey) -> usize {
    key.actor_did.len()
        + key.policy_id.len()
        + key.resource.len()
        + key.doc_id.len()
        + key.permission.len()
        + DECISION_OVERHEAD
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
            order: SegQueue::new(),
            bytes: AtomicUsize::new(0),
            budget: DECISION_BYTES,
            events: AtomicBool::new(false),
            registrations: inputs::Cache::new(REGISTRATION_BYTES, Some(ttl)),
            origin: Instant::now(),
        }
    }

    /// Invalidation events reach this cache from now on: denials and
    /// registrations are cached too.
    pub(crate) fn receive_events(&self) {
        self.events.store(true, Ordering::Release);
    }

    /// Whether a denial may be cached.
    pub(crate) fn caches_denials(&self) -> bool {
        self.events.load(Ordering::Acquire)
    }

    fn now_micros(&self) -> u64 {
        u64::try_from(self.origin.elapsed().as_micros()).unwrap_or(u64::MAX)
    }

    /// Whether the document is registered, as a lookup cached while events
    /// reach this cache found it; `None` when it must be looked up.
    pub(crate) fn registration(
        &self,
        policy_id: &str,
        resource: &str,
        doc_id: &str,
    ) -> Option<bool> {
        if !self.caches_denials() {
            return None;
        }
        self.registrations.get(
            &RegistrationKey {
                policy_id: policy_id.to_string(),
                resource: resource.to_string(),
                doc_id: doc_id.to_string(),
            },
            self.now_micros(),
        )
    }

    /// Caches a registration lookup whose answer was read since
    /// `generation` ([`registration_generation`](Self::registration_generation)),
    /// while events reach this cache.
    pub(crate) fn remember_registration(
        &self,
        policy_id: &str,
        resource: &str,
        doc_id: &str,
        registered: bool,
        generation: u64,
    ) {
        if !self.caches_denials() {
            return;
        }
        let bytes = policy_id.len() + resource.len() + doc_id.len();
        self.registrations.insert(
            RegistrationKey {
                policy_id: policy_id.to_string(),
                resource: resource.to_string(),
                doc_id: doc_id.to_string(),
            },
            registered,
            bytes,
            self.now_micros(),
            generation,
        );
    }

    /// The generation a registration lookup notes before it asks.
    pub(crate) fn registration_generation(&self) -> u64 {
        self.registrations.generation()
    }

    /// Charges a settled decision and evicts the oldest past the budget.
    fn admit(&self, key: &CacheKey) {
        self.bytes.fetch_add(charge(key), Ordering::AcqRel);
        self.order.push(key.clone());
        while self.bytes.load(Ordering::Acquire) > self.budget {
            let Some(oldest) = self.order.pop() else {
                break;
            };
            self.remove_settled(&oldest);
        }
        // Records of decisions already gone: drop them once they outnumber
        // the cached decisions, so the order stays proportional to the cache.
        let mut passes = self.order.len();
        while self.order.len()
            > self
                .entries
                .len()
                .saturating_mul(2)
                .saturating_add(ORDER_SLACK)
            && passes > 0
        {
            passes -= 1;
            let Some(record) = self.order.pop() else {
                break;
            };
            if self
                .entries
                .get(&record)
                .is_some_and(|entry| entry.allowed.get().is_some())
            {
                self.order.push(record);
            }
        }
    }

    /// Removes `key`'s decision if it is settled, refunding its charge. A
    /// check in progress is never evicted.
    fn remove_settled(&self, key: &CacheKey) {
        if self
            .entries
            .remove_if(key, |entry| entry.allowed.get().is_some())
            .is_some()
        {
            self.bytes.fetch_sub(charge(key), Ordering::AcqRel);
        }
    }

    /// Removes `key`'s entry, refunding the charge of a settled one.
    fn remove_entry(&self, key: &CacheKey) {
        if let Some(entry) = self.entries.force_remove(key) {
            if entry.allowed.get().is_some() {
                self.bytes.fetch_sub(charge(key), Ordering::AcqRel);
            }
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

    #[allow(deprecated, reason = "try_update requires Rust 1.95; MSRV is 1.91")]
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
                allowed: OnceCell::new(),
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
        self.registrations.invalidate(|key| {
            key.policy_id == policy_id && key.resource == resource && key.doc_id == doc_id
        });
        let stale: Vec<CacheKey> = self
            .entries
            .keys()
            .filter(|key| {
                key.policy_id == policy_id && key.resource == resource && key.doc_id == doc_id
            })
            .collect();
        let count = stale.len();
        for key in &stale {
            self.remove_entry(key);
        }
        count
    }

    pub(crate) fn invalidate_policy(&self, policy_id: &str) -> usize {
        self.registrations
            .invalidate(|key| key.policy_id == policy_id);
        let stale: Vec<CacheKey> = self
            .entries
            .keys()
            .filter(|key| key.policy_id == policy_id)
            .collect();
        let count = stale.len();
        for key in &stale {
            self.remove_entry(key);
        }
        count
    }

    pub(crate) fn clear(&self) -> usize {
        self.registrations.clear();
        let count = self.entries.len();
        self.entries.clear();
        self.bytes.store(0, Ordering::Release);
        while self.order.pop().is_some() {}
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
