//! Bounded, expiring cache of authenticated Iroh endpoint-to-DID bindings.

use std::time::Duration;

use regolith::sync::{ArrayQueue, HopscotchMap};

use crate::transport::PeerId;

#[derive(Clone)]
struct Entry {
    did: identity::Did,
    verified_at: web_time::Instant,
}

/// Positive cache shared by every resolver clone without a lock.
///
/// Peers are evicted in first-insert order. A peer takes one place in a
/// bounded FIFO when it is first inserted, and a refresh keeps that place.
/// When the FIFO is full the oldest peer is removed. A refresh that lands
/// while its own peer is being evicted is removed with it, so the next lookup
/// resolves the peer again.
pub(crate) struct IrohPeerIdentityCache {
    entries: HopscotchMap<PeerId, Entry, rapidhash::fast::RandomState>,
    order: ArrayQueue<PeerId>,
    ttl: Duration,
}

impl IrohPeerIdentityCache {
    pub(crate) fn new(ttl: Duration, capacity: usize) -> Self {
        Self {
            entries: HopscotchMap::with_capacity_and_hasher(
                capacity,
                rapidhash::fast::RandomState::default(),
            ),
            order: ArrayQueue::new(capacity),
            ttl,
        }
    }

    pub(crate) fn get(&self, peer_id: &PeerId, now: web_time::Instant) -> Option<identity::Did> {
        let entry = self.entries.get(peer_id)?;
        (now.duration_since(entry.verified_at) < self.ttl).then_some(entry.did)
    }

    pub(crate) fn insert(&self, peer_id: PeerId, did: identity::Did, now: web_time::Instant) {
        let mut first_insert = false;
        self.entries.compute(peer_id.clone(), |existing| {
            first_insert = existing.is_none();
            Some(Entry {
                did,
                verified_at: now,
            })
        });
        if first_insert {
            self.enqueue(peer_id);
        }
    }

    /// Takes the peer's place in the FIFO only after its entry is in the map,
    /// so an eviction never pops a place whose entry has yet to land.
    fn enqueue(&self, peer_id: PeerId) {
        let mut pending = peer_id;
        while let Err(rejected) = self.order.push(pending) {
            pending = rejected;
            if let Some(evicted) = self.order.pop() {
                self.entries.remove(&evicted);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn did(label: &str) -> identity::Did {
        identity::Did::new(format!("did:key:{label}")).unwrap()
    }

    fn peer(label: &str) -> PeerId {
        PeerId::new(format!("peer-{label}"))
    }

    #[test]
    fn refreshing_a_peer_keeps_its_single_place() {
        let now = web_time::Instant::now();
        let cache = IrohPeerIdentityCache::new(Duration::from_secs(10), 2);
        let peer_a = peer("a");
        let peer_b = peer("b");

        cache.insert(peer_a.clone(), did("a1"), now);
        cache.insert(peer_a.clone(), did("a2"), now);
        cache.insert(peer_b.clone(), did("b"), now);

        assert_eq!(cache.get(&peer_a, now).unwrap(), did("a2"));
        assert_eq!(cache.get(&peer_b, now).unwrap(), did("b"));
    }

    #[test]
    fn a_refresh_keeps_its_first_insert_place_in_eviction_order() {
        let now = web_time::Instant::now();
        let cache = IrohPeerIdentityCache::new(Duration::from_secs(10), 2);
        let peer_a = peer("a");
        let peer_b = peer("b");
        let peer_c = peer("c");

        cache.insert(peer_a.clone(), did("a1"), now);
        cache.insert(peer_b.clone(), did("b"), now);
        cache.insert(peer_a.clone(), did("a2"), now);
        cache.insert(peer_c.clone(), did("c"), now);

        assert!(cache.get(&peer_a, now).is_none());
        assert_eq!(cache.get(&peer_b, now).unwrap(), did("b"));
        assert_eq!(cache.get(&peer_c, now).unwrap(), did("c"));
        assert_eq!(cache.entries.len(), 2);
    }

    #[test]
    fn a_peer_inserted_again_after_eviction_takes_a_new_place() {
        let now = web_time::Instant::now();
        let cache = IrohPeerIdentityCache::new(Duration::from_secs(10), 1);
        let peer_a = peer("a");
        let peer_b = peer("b");

        cache.insert(peer_a.clone(), did("a1"), now);
        cache.insert(peer_b.clone(), did("b"), now);
        cache.insert(peer_a.clone(), did("a2"), now);

        assert_eq!(cache.get(&peer_a, now).unwrap(), did("a2"));
        assert!(cache.get(&peer_b, now).is_none());
        assert_eq!(cache.entries.len(), 1);
    }

    #[test]
    fn concurrent_inserts_settle_with_one_place_per_cached_peer() {
        const CAPACITY: usize = 4;
        const THREADS: usize = 8;
        const ROUNDS: usize = 2_000;
        let now = web_time::Instant::now();
        let cache = IrohPeerIdentityCache::new(Duration::from_secs(10), CAPACITY);

        std::thread::scope(|scope| {
            for thread in 0..THREADS {
                let cache = &cache;
                scope.spawn(move || {
                    for round in 0..ROUNDS {
                        let label = (thread * 7 + round * 3) % (CAPACITY * 3);
                        cache.insert(peer(&label.to_string()), did("x"), now);
                    }
                });
            }
        });

        let mut queued = rapidhash::RapidHashSet::default();
        while let Some(peer_id) = cache.order.pop() {
            assert!(queued.insert(peer_id), "a peer holds two places");
        }
        let cached: rapidhash::RapidHashSet<PeerId> = cache.entries.keys().collect();
        assert!(cached.len() <= CAPACITY);
        assert_eq!(cached, queued);
    }
}
