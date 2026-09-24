//! Peer lifecycle, queries, and maintenance operations.

use std::sync::atomic::Ordering;

use cid::Cid;

use super::PeerStateTracker;
use crate::sync::peer_state::stats::PeerStats;
use crate::topics::{DOC_SYNC_TOPIC, ENCRYPTION_TOPIC, SYNC_BRANCHABLE_TOPIC};

fn is_topic_or_subtopic(topic: &str, base: &str) -> bool {
    topic == base
        || topic
            .strip_prefix(base)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn is_data_subscription_topic(topic: &str) -> bool {
    topic != ENCRYPTION_TOPIC
        && !is_topic_or_subtopic(topic, DOC_SYNC_TOPIC)
        && !is_topic_or_subtopic(topic, SYNC_BRANCHABLE_TOPIC)
}

impl PeerStateTracker {
    /// Record that a peer connected.
    pub fn peer_connected(&self, peer_id: &str) {
        let info = self.peer_entry(peer_id);
        info.connected.store(true, Ordering::Relaxed);
        info.last_seen.store(self.now(), Ordering::Relaxed);
        self.enforce_global_limits();
    }

    /// Record that a peer disconnected.
    pub fn peer_disconnected(&self, peer_id: &str) {
        if let Some(info) = self.peers.get(peer_id) {
            info.connected.store(false, Ordering::Relaxed);
            info.last_seen.store(self.now(), Ordering::Relaxed);
        }
    }

    /// Record that a peer has a specific CID.
    ///
    /// Call this when:
    /// - Receiving a block from a peer (they definitely have it)
    /// - Successfully sending a block to a peer (they now have it)
    ///
    /// Creates a peer entry if one doesn't exist (handles race conditions
    /// where CID announcements arrive before connection events).
    ///
    /// Note: CID tracking is bounded by `max_cids_per_peer` (per-peer LRU)
    /// and `max_total_cids` (global limit). When limits are reached, oldest
    /// CIDs are evicted.
    pub fn peer_has_cid(&self, peer_id: &str, cid: Cid) {
        self.peer_has_cids(peer_id, std::iter::once(cid));
    }

    /// Record multiple CIDs for a peer.
    ///
    /// Creates a peer entry if one doesn't exist.
    ///
    /// Note: CID tracking is bounded by `max_cids_per_peer` (per-peer LRU)
    /// and `max_total_cids` (global limit). When limits are reached, oldest
    /// CIDs are evicted.
    pub fn peer_has_cids(&self, peer_id: &str, cids: impl IntoIterator<Item = Cid>) {
        let info = self.peer_entry(peer_id);
        let before = info.known_cids.len();
        for cid in cids {
            info.add_cid(cid, self.max_cids_per_peer);
        }
        let added = info.known_cids.len().saturating_sub(before);
        self.total_cids.fetch_add(added, Ordering::Relaxed);
        info.last_seen.store(self.now(), Ordering::Relaxed);
        self.enforce_global_limits();
    }

    /// Record that a peer subscribed to a collection.
    pub fn peer_subscribed(&self, peer_id: &str, collection_id: String) {
        let info = self.peer_entry(peer_id);
        info.subscribed_collections
            .insert_if_absent(collection_id, ());
        info.last_seen.store(self.now(), Ordering::Relaxed);
    }

    /// Record that a peer unsubscribed from a collection.
    pub fn peer_unsubscribed(&self, peer_id: &str, collection_id: &str) {
        if let Some(info) = self.peers.get(peer_id) {
            info.subscribed_collections.remove(collection_id);
            info.last_seen.store(self.now(), Ordering::Relaxed);
        }
    }

    /// Check if a peer likely has a CID.
    pub fn peer_has(&self, peer_id: &str, cid: &Cid) -> bool {
        self.peers
            .get(peer_id)
            .is_some_and(|info| info.known_cids.contains_key(cid))
    }

    /// Get all connected peers that might have a CID.
    ///
    /// Returns peers that:
    /// - Are currently connected
    /// - Have announced this CID
    pub fn peers_with_cid(&self, cid: &Cid) -> Vec<String> {
        self.peers
            .iter()
            .filter(|(_, info)| info.is_connected() && info.known_cids.contains_key(cid))
            .map(|(peer_id, _)| peer_id.to_string())
            .collect()
    }

    /// Get all connected peers subscribed to a collection.
    pub fn peers_for_collection(&self, collection_id: &str) -> Vec<String> {
        self.peers
            .iter()
            .filter(|(_, info)| {
                info.is_connected() && info.subscribed_collections.contains_key(collection_id)
            })
            .map(|(peer_id, _)| peer_id.to_string())
            .collect()
    }

    /// Check if a peer has advertised interest in any data topic.
    ///
    /// System RPC topics are excluded because every libp2p node joins
    /// them at startup; accepting those would collapse Controlled mode back
    /// to "any connected sync peer".
    pub fn peer_has_data_subscription(&self, peer_id: &str) -> bool {
        self.peers.get(peer_id).is_some_and(|info| {
            info.subscribed_collections
                .keys()
                .any(|topic| is_data_subscription_topic(&topic))
        })
    }

    /// Check if a peer has advertised interest in a specific data collection.
    pub fn peer_subscribed_to_collection(&self, peer_id: &str, collection_id: &str) -> bool {
        self.peers
            .get(peer_id)
            .is_some_and(|info| info.subscribed_collections.contains_key(collection_id))
    }

    /// Get all connected peers.
    pub fn connected_peers(&self) -> Vec<String> {
        self.peers
            .iter()
            .filter(|(_, info)| info.is_connected())
            .map(|(peer_id, _)| peer_id.to_string())
            .collect()
    }

    /// Get peers that DON'T have a CID (potential recipients for broadcast).
    ///
    /// Returns connected peers that haven't announced having this CID.
    pub fn peers_without_cid(&self, cid: &Cid) -> Vec<String> {
        self.peers
            .iter()
            .filter(|(_, info)| info.is_connected() && !info.known_cids.contains_key(cid))
            .map(|(peer_id, _)| peer_id.to_string())
            .collect()
    }

    /// Get number of CIDs known for a peer.
    pub fn peer_cid_count(&self, peer_id: &str) -> usize {
        self.peers
            .get(peer_id)
            .map_or(0, |info| info.known_cids.len())
    }

    /// Check if a peer is connected.
    pub fn is_connected(&self, peer_id: &str) -> bool {
        self.peers
            .get(peer_id)
            .is_some_and(|info| info.is_connected())
    }

    /// Remove stale peer entries that have been disconnected longer than TTL.
    pub fn cleanup_stale(&self) {
        let now = self.now();
        let ttl = self.peer_ttl.as_nanos() as u64;
        let stale: Vec<_> = self
            .peers
            .iter()
            .filter(|(_, info)| !info.is_connected() && now.saturating_sub(info.last_seen()) >= ttl)
            .map(|(peer_id, _)| peer_id)
            .collect();
        for peer_id in stale {
            self.remove_peer(&peer_id);
        }
    }

    /// Get statistics about tracked peers.
    pub fn stats(&self) -> PeerStats {
        let mut total = 0;
        let mut connected = 0;
        let mut total_cids = 0;
        for (_, info) in self.peers.iter() {
            total += 1;
            connected += usize::from(info.is_connected());
            total_cids += info.known_cids.len();
        }

        PeerStats::new(total, connected, total_cids)
    }
}
