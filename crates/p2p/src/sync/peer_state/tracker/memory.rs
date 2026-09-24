//! Global memory limit enforcement for the peer state tracker.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use super::{PeerInfo, PeerStateTracker};

impl PeerStateTracker {
    /// Enforce global limits by evicting oldest disconnected peers and their CIDs.
    ///
    /// Called internally when adding peers or CIDs.
    pub(super) fn enforce_global_limits(&self) {
        // Check peer count limit - evict oldest disconnected peers first
        while self.peers.len() > self.max_peers {
            // Find the oldest disconnected peer
            let oldest_disconnected = self
                .peers
                .iter()
                .filter(|(_, info)| !info.is_connected())
                .min_by_key(|(_, info)| info.last_seen())
                .map(|(id, _)| id);

            if let Some(peer_id) = oldest_disconnected {
                tracing::debug!(
                    peer_id = %peer_id,
                    "Evicting oldest disconnected peer to stay under max_peers limit"
                );
                self.remove_peer(&peer_id);
            } else {
                // All peers are connected, can't evict
                tracing::warn!(
                    current = self.peers.len(),
                    max = self.max_peers,
                    "Cannot evict peers - all are connected"
                );
                break;
            }
        }

        // Check total CID count limit - evict CIDs from peers with most CIDs
        if self.total_cids.load(Ordering::Relaxed) <= self.max_total_cids {
            return;
        }
        let mut peer_cid_counts: Vec<(Arc<PeerInfo>, usize, bool)> = self
            .peers
            .values()
            .map(|info| {
                let count = info.known_cids.len();
                let connected = info.is_connected();
                (info, count, connected)
            })
            .collect();
        let total_cids: usize = peer_cid_counts.iter().map(|(_, count, _)| count).sum();
        self.total_cids.store(total_cids, Ordering::Relaxed);
        if total_cids <= self.max_total_cids {
            return;
        }
        let excess = total_cids - self.max_total_cids;
        let mut evicted = 0;

        // Evict from peers with the most CIDs (disconnected first)
        peer_cid_counts.sort_by(|a, b| {
            // Disconnected peers should come first
            match (a.2, b.2) {
                (false, true) => std::cmp::Ordering::Less,
                (true, false) => std::cmp::Ordering::Greater,
                _ => b.1.cmp(&a.1), // More CIDs first
            }
        });

        for (info, _, _) in peer_cid_counts {
            // Evict oldest CIDs from this peer
            while evicted < excess && info.evict_oldest_cid() {
                evicted += 1;
            }
            if evicted >= excess {
                break;
            }
        }

        if evicted > 0 {
            self.total_cids
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |total| {
                    Some(total.saturating_sub(evicted))
                })
                .ok();
            tracing::debug!(
                evicted = evicted,
                "Evicted CIDs to stay under max_total_cids limit"
            );
        }
    }
}
