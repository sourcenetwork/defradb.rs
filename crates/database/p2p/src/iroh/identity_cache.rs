//! Bounded, expiring cache of authenticated Iroh endpoint-to-DID bindings.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use regolith::sync::{ArrayQueue, HopscotchMap};

use crate::transport::PeerId;

#[derive(Clone)]
struct Entry {
    did: identity::Did,
    verified_at: web_time::Instant,
    slot: u64,
}

/// Positive cache shared by every resolver clone without a lock.
///
/// Each cached peer owns one slot in a bounded FIFO, tagged with a sequence
/// number. When the FIFO is full the oldest slot is evicted, and its peer is
/// removed only while the entry still carries that slot, so an eviction never
/// drops a binding inserted after it.
pub(crate) struct IrohPeerIdentityCache {
    entries: HopscotchMap<PeerId, Entry, rapidhash::fast::RandomState>,
    order: ArrayQueue<(PeerId, u64)>,
    next_slot: AtomicU64,
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
            next_slot: AtomicU64::new(0),
            ttl,
        }
    }

    pub(crate) fn get(&self, peer_id: &PeerId, now: web_time::Instant) -> Option<identity::Did> {
        let entry = self.entries.get(peer_id)?;
        (now.duration_since(entry.verified_at) < self.ttl).then_some(entry.did)
    }

    pub(crate) fn insert(&self, peer_id: PeerId, did: identity::Did, now: web_time::Instant) {
        let mut new_slot = None;
        self.entries.compute(peer_id.clone(), |existing| {
            let slot = match existing {
                Some(entry) => entry.slot,
                None => *new_slot.insert(self.next_slot.fetch_add(1, Ordering::Relaxed)),
            };
            Some(Entry {
                did,
                verified_at: now,
                slot,
            })
        });
        if let Some(slot) = new_slot {
            self.enqueue(peer_id, slot);
        }
    }

    fn enqueue(&self, peer_id: PeerId, slot: u64) {
        let mut pending = (peer_id, slot);
        while let Err(rejected) = self.order.push(pending) {
            pending = rejected;
            if let Some((evicted, evicted_slot)) = self.order.pop() {
                self.entries
                    .remove_if(&evicted, |entry| entry.slot == evicted_slot);
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

    #[test]
    fn refreshing_a_peer_keeps_its_single_slot() {
        let now = web_time::Instant::now();
        let cache = IrohPeerIdentityCache::new(Duration::from_secs(10), 2);
        let peer_a = PeerId::new("peer-a".to_string());
        let peer_b = PeerId::new("peer-b".to_string());

        cache.insert(peer_a.clone(), did("a1"), now);
        cache.insert(peer_a.clone(), did("a2"), now);
        cache.insert(peer_b.clone(), did("b"), now);

        assert_eq!(cache.get(&peer_a, now).unwrap(), did("a2"));
        assert_eq!(cache.get(&peer_b, now).unwrap(), did("b"));
    }

    #[test]
    fn eviction_never_drops_a_newer_binding() {
        let now = web_time::Instant::now();
        let cache = IrohPeerIdentityCache::new(Duration::from_secs(10), 1);
        let peer_a = PeerId::new("peer-a".to_string());
        let peer_b = PeerId::new("peer-b".to_string());

        cache.insert(peer_a.clone(), did("a1"), now);
        cache.insert(peer_b.clone(), did("b"), now);
        cache.insert(peer_a.clone(), did("a2"), now);

        assert_eq!(cache.get(&peer_a, now).unwrap(), did("a2"));
        assert!(cache.get(&peer_b, now).is_none());
        assert_eq!(cache.entries.len(), 1);
    }
}
