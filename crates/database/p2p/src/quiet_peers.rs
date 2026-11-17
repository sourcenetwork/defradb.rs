//! The peers this node hung up on deliberately (#2992).
//!
//! `DisconnectPeers` closes a peer's connections, but discovery (Kademlia,
//! vinc peer exchange) would dial it again within milliseconds. A peer in
//! this table is never dialed by discovery; only an explicit dial
//! (`ConnectPeers`, a replicator, `AllowPeer`) takes it out. A real drop or a
//! restart never puts a peer here, so discovery reconnects those as before.

use std::collections::VecDeque;
use std::hash::Hash;

use rapidhash::{HashSetExt, RapidHashSet};
use tracing::warn;

/// The most peers kept. Past it the oldest is forgotten, and discovery may
/// dial it again.
pub const MAX_QUIET_PEERS: usize = 1024;

/// A bounded set of peers discovery must not dial, oldest evicted first.
#[derive(Debug)]
pub struct QuietPeers<K> {
    peers: RapidHashSet<K>,
    order: VecDeque<K>,
    capacity: usize,
    evicted: u64,
}

impl<K> Default for QuietPeers<K>
where
    K: Copy + Eq + Hash + std::fmt::Debug,
{
    fn default() -> Self {
        Self::with_capacity(MAX_QUIET_PEERS)
    }
}

impl<K> QuietPeers<K>
where
    K: Copy + Eq + Hash + std::fmt::Debug,
{
    /// An empty table holding at most `capacity` peers.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            peers: RapidHashSet::with_capacity(capacity),
            order: VecDeque::with_capacity(capacity),
            capacity: capacity.max(1),
            evicted: 0,
        }
    }

    /// Keep discovery away from `peer`.
    pub fn insert(&mut self, peer: K) {
        if !self.peers.insert(peer) {
            return;
        }
        self.order.push_back(peer);
        if self.order.len() > self.capacity {
            if let Some(oldest) = self.order.pop_front() {
                self.peers.remove(&oldest);
                self.evicted += 1;
                warn!(
                    peer_id = ?oldest,
                    evicted = self.evicted,
                    "quiet-peer table full: forgot the oldest deliberately disconnected peer"
                );
            }
        }
    }

    /// Let discovery dial `peer` again: something explicit named it.
    pub fn remove(&mut self, peer: &K) {
        if self.peers.remove(peer) {
            self.order.retain(|kept| kept != peer);
        }
    }

    /// Whether discovery must leave `peer` alone.
    pub fn contains(&self, peer: &K) -> bool {
        self.peers.contains(peer)
    }

    /// How many peers the table holds.
    pub fn len(&self) -> usize {
        self.peers.len()
    }

    /// Whether the table is empty.
    pub fn is_empty(&self) -> bool {
        self.peers.is_empty()
    }

    /// How many peers were forgotten because the table was full.
    pub fn evicted(&self) -> u64 {
        self.evicted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_table_forgets_its_oldest_peer_and_counts_it() {
        let mut quiet = QuietPeers::with_capacity(3);
        for peer in 0u32..3 {
            quiet.insert(peer);
        }
        assert_eq!(quiet.len(), 3);
        assert_eq!(quiet.evicted(), 0);

        quiet.insert(3);
        assert_eq!(quiet.len(), 3);
        assert_eq!(quiet.evicted(), 1);
        assert!(!quiet.contains(&0), "the oldest peer is forgotten");
        assert!((1..4).all(|peer| quiet.contains(&peer)));
    }

    #[test]
    fn inserting_a_held_peer_neither_grows_nor_evicts() {
        let mut quiet = QuietPeers::with_capacity(2);
        quiet.insert(1u32);
        quiet.insert(2);
        quiet.insert(1);
        assert_eq!(quiet.len(), 2);
        assert_eq!(quiet.evicted(), 0);
    }

    #[test]
    fn a_removed_peer_frees_its_slot() {
        let mut quiet = QuietPeers::with_capacity(2);
        quiet.insert(1u32);
        quiet.insert(2);
        quiet.remove(&1);
        assert!(!quiet.contains(&1));
        quiet.insert(3);
        assert_eq!(quiet.evicted(), 0);
        assert!(quiet.contains(&2) && quiet.contains(&3));
    }
}
