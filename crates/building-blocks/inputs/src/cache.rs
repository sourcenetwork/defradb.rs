//! A cache bounded by bytes, lock-free, with a time to live and invalidation
//! a fetch in progress cannot undo.
//!
//! Entries live in a `HopscotchMap`; their insertion order in a `SegQueue`.
//! An insert that takes the total past the budget evicts the oldest entries
//! until it is back under. Each entry carries an id, so the queue's record of
//! a replaced or removed entry never evicts its successor.
//!
//! Invalidation bumps a generation before it removes anything. A fetch notes
//! the generation when it starts and caches its answer only if no
//! invalidation began since: an answer read before a revocation never
//! outlives the revocation's invalidation.

use std::hash::Hash;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use defra_core::thread_bounds::MaybeSendSync;
use rapidhash::fast::RandomState;
use regolith::sync::{HopscotchMap, SegQueue};

/// The bytes the cache charges per entry on top of what the caller counts:
/// the map slot, the entry's header and its place in the order queue.
const ENTRY_OVERHEAD: usize = 96;

/// When the order queue holds this many records per live entry (plus
/// slack), records of replaced or removed entries are dropped.
const ORDER_SLACK: usize = 64;

struct Entry<V> {
    id: u64,
    value: V,
    bytes: usize,
    at: u64,
}

/// A map from keys to answers, bounded by bytes.
pub struct Cache<K, V>
where
    K: Clone + Eq + Hash + 'static,
    V: Clone + 'static,
{
    entries: HopscotchMap<K, Arc<Entry<V>>, RandomState>,
    order: SegQueue<(K, u64)>,
    bytes: AtomicUsize,
    budget: usize,
    ttl_micros: Option<u64>,
    next_id: AtomicU64,
    generation: AtomicU64,
}

impl<K, V> Cache<K, V>
where
    K: Clone + Eq + Hash + MaybeSendSync + 'static,
    V: Clone + MaybeSendSync + 'static,
{
    /// A cache of at most `budget` bytes whose entries expire `ttl` after
    /// they were cached (never, with `None`). A budget of 0 caches nothing.
    pub fn new(budget: usize, ttl: Option<Duration>) -> Self {
        Self {
            entries: HopscotchMap::with_hasher(RandomState::default()),
            order: SegQueue::new(),
            bytes: AtomicUsize::new(0),
            budget,
            ttl_micros: ttl.map(|ttl| u64::try_from(ttl.as_micros()).unwrap_or(u64::MAX)),
            next_id: AtomicU64::new(0),
            generation: AtomicU64::new(0),
        }
    }

    /// `key`'s answer, unless absent or older than the time to live at `now`.
    pub fn get(&self, key: &K, now: u64) -> Option<V> {
        let entry = self.entries.get(key)?;
        if self
            .ttl_micros
            .is_some_and(|ttl| now.saturating_sub(entry.at) >= ttl)
        {
            self.remove_entry(key, entry.id);
            return None;
        }
        Some(entry.value.clone())
    }

    /// The generation a fetch notes before it starts, for
    /// [`insert`](Self::insert).
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    /// Caches `value` for `key`, charged `bytes`, unless an invalidation began
    /// since `generation` or the entry alone exceeds the budget. Returns
    /// whether it was cached.
    pub fn insert(&self, key: K, value: V, bytes: usize, now: u64, generation: u64) -> bool {
        let bytes = bytes.saturating_add(ENTRY_OVERHEAD);
        if bytes > self.budget || self.generation() != generation {
            return false;
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let entry = Arc::new(Entry {
            id,
            value,
            bytes,
            at: now,
        });
        if let Some(old) = self.entries.insert(key.clone(), entry) {
            self.bytes.fetch_sub(old.bytes, Ordering::AcqRel);
        }
        self.bytes.fetch_add(bytes, Ordering::AcqRel);
        self.order.push((key.clone(), id));
        // An invalidation that began after the check above may have scanned
        // before this entry landed: it must not survive it.
        if self.generation() != generation {
            self.remove_entry(&key, id);
            return false;
        }
        self.evict();
        true
    }

    /// Removes every entry whose key matches, and keeps any fetch already in
    /// progress from caching its answer. Returns how many were removed.
    pub fn invalidate<F>(&self, matches: F) -> usize
    where
        F: Fn(&K) -> bool,
    {
        self.generation.fetch_add(1, Ordering::AcqRel);
        let stale: Vec<K> = self.entries.keys().filter(|key| matches(key)).collect();
        stale
            .iter()
            .filter(|key| match self.entries.remove(*key) {
                Some(entry) => {
                    self.bytes.fetch_sub(entry.bytes, Ordering::AcqRel);
                    true
                }
                None => false,
            })
            .count()
    }

    /// Removes every entry, as [`invalidate`](Self::invalidate) of all keys.
    pub fn clear(&self) -> usize {
        self.invalidate(|_| true)
    }

    /// The bytes charged now.
    pub fn bytes(&self) -> usize {
        self.bytes.load(Ordering::Acquire)
    }

    /// Entries held now.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether nothing is cached.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn remove_entry(&self, key: &K, id: u64) -> bool {
        match self.entries.remove_if(key, |entry| entry.id == id) {
            Some(entry) => {
                self.bytes.fetch_sub(entry.bytes, Ordering::AcqRel);
                true
            }
            None => false,
        }
    }

    fn evict(&self) {
        while self.bytes() > self.budget {
            let Some((key, id)) = self.order.pop() else {
                break;
            };
            self.remove_entry(&key, id);
        }
        // Records of replaced or removed entries: drop them once they
        // outnumber the live entries, so the queue stays proportional to the
        // cache. A live record met on the way goes back to the end, a second
        // chance as in CLOCK.
        let live = self.entries.len();
        let mut budget = self.order.len();
        while self.order.len() > live.saturating_mul(2).saturating_add(ORDER_SLACK) && budget > 0 {
            budget -= 1;
            let Some((key, id)) = self.order.pop() else {
                break;
            };
            if self.entries.get(&key).is_some_and(|entry| entry.id == id) {
                self.order.push((key, id));
            }
        }
    }
}
