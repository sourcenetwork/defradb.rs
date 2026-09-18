//! Peer state tracking for P2P synchronization.
//!
//! Tracks which blocks each peer has, enabling:
//! - Efficient block requests (ask peers who have the block)
//! - Avoiding redundant sends (don't send blocks peers already have)
//! - Replication status monitoring

mod memory;
mod operations;

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use web_time::Instant;

use cid::Cid;
use kovan_map::HopscotchMap;
use kovan_queue::seg_queue::SegQueue;
use rapidhash::fast::RandomState;

/// Default maximum number of CIDs to track per peer.
/// This prevents unbounded memory growth in long-running nodes.
const DEFAULT_MAX_CIDS_PER_PEER: usize = 10_000;

/// Default maximum total CIDs across all peers.
/// With 100 peers at 10k CIDs each = ~40MB memory usage.
const DEFAULT_MAX_TOTAL_CIDS: usize = 1_000_000;

/// Default maximum number of tracked peers.
const DEFAULT_MAX_PEERS: usize = 1_000;

pub(super) type PeerTable = HopscotchMap<Arc<str>, Arc<PeerInfo>, RandomState>;

/// Information about a single peer's sync state. Every field is updated in
/// place, so an entry is shared rather than copied.
pub(super) struct PeerInfo {
    /// CIDs this peer has announced or we've sent to them
    pub(super) known_cids: HopscotchMap<Cid, (), RandomState>,
    /// Insertion order for LRU eviction (oldest first)
    pub(super) cid_order: SegQueue<Cid>,
    /// Collections this peer is subscribed to
    pub(super) subscribed_collections: HopscotchMap<String, (), RandomState>,
    /// When we last heard from this peer, in nanoseconds since the tracker's epoch
    pub(super) last_seen: AtomicU64,
    /// Whether peer is currently connected
    pub(super) connected: AtomicBool,
}

impl PeerInfo {
    pub(super) fn new(now: u64) -> Self {
        Self {
            known_cids: HopscotchMap::with_hasher(RandomState::default()),
            cid_order: SegQueue::new(),
            subscribed_collections: HopscotchMap::with_hasher(RandomState::default()),
            last_seen: AtomicU64::new(now),
            connected: AtomicBool::new(false),
        }
    }

    pub(super) fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    pub(super) fn last_seen(&self) -> u64 {
        self.last_seen.load(Ordering::Relaxed)
    }

    /// Add a CID with LRU eviction if at capacity. Returns `true` when the
    /// CID was not already known.
    pub(super) fn add_cid(&self, cid: Cid, max_cids: usize) -> bool {
        if self.known_cids.insert_if_absent(cid, ()).is_some() {
            return false;
        }
        self.cid_order.push(cid);
        while self.known_cids.len() > max_cids {
            let Some(oldest) = self.cid_order.pop() else {
                break;
            };
            self.known_cids.remove(&oldest);
        }
        true
    }

    /// Drop the oldest known CID. Returns `false` once none is left.
    pub(super) fn evict_oldest_cid(&self) -> bool {
        while let Some(oldest) = self.cid_order.pop() {
            if self.known_cids.remove(&oldest).is_some() {
                return true;
            }
        }
        false
    }
}

/// Tracks the sync state of all known peers.
///
/// Thread-safe: can be shared across tasks.
///
/// # Memory Limits
///
/// To prevent unbounded memory growth, the tracker enforces three limits:
/// - `max_cids_per_peer`: Maximum CIDs tracked for any single peer (LRU eviction)
/// - `max_total_cids`: Maximum CIDs across ALL peers (oldest peers evicted first)
/// - `max_peers`: Maximum number of tracked peers (oldest disconnected peers evicted)
pub struct PeerStateTracker {
    /// Per-peer state
    pub(super) peers: PeerTable,
    /// Origin of every `last_seen` timestamp
    pub(super) epoch: Instant,
    /// CIDs known across all peers, kept as an upper bound: a peer removed
    /// while still receiving CIDs leaves the count high, never low, and the
    /// next limit sweep recomputes it exactly.
    pub(super) total_cids: AtomicUsize,
    /// How long to keep peer info after disconnect
    pub(super) peer_ttl: Duration,
    /// Maximum CIDs to track per peer (prevents memory exhaustion)
    pub(super) max_cids_per_peer: usize,
    /// Maximum total CIDs across all peers
    pub(super) max_total_cids: usize,
    /// Maximum number of tracked peers
    pub(super) max_peers: usize,
}

impl Default for PeerStateTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl PeerStateTracker {
    /// Create a new peer state tracker with default settings.
    pub fn new() -> Self {
        Self::with_ttl(Duration::from_secs(3600))
    }

    /// Create with custom peer TTL.
    pub fn with_ttl(peer_ttl: Duration) -> Self {
        Self::with_full_config(
            peer_ttl,
            DEFAULT_MAX_CIDS_PER_PEER,
            DEFAULT_MAX_TOTAL_CIDS,
            DEFAULT_MAX_PEERS,
        )
    }

    /// Create with custom configuration.
    ///
    /// # Arguments
    ///
    /// * `peer_ttl` - How long to keep disconnected peer info
    /// * `max_cids_per_peer` - Max CIDs per peer (0 = use default)
    ///
    /// # Logging
    ///
    /// Logs a warning if `max_cids_per_peer` is 0 (falls back to default).
    pub fn with_config(peer_ttl: Duration, max_cids_per_peer: usize) -> Self {
        Self::with_full_config(
            peer_ttl,
            max_cids_per_peer,
            DEFAULT_MAX_TOTAL_CIDS,
            DEFAULT_MAX_PEERS,
        )
    }

    /// Create with full custom configuration including global limits.
    ///
    /// # Arguments
    ///
    /// * `peer_ttl` - How long to keep disconnected peer info
    /// * `max_cids_per_peer` - Max CIDs per peer (0 = use default)
    /// * `max_total_cids` - Max total CIDs across all peers (0 = use default)
    /// * `max_peers` - Max tracked peers (0 = use default)
    pub fn with_full_config(
        peer_ttl: Duration,
        max_cids_per_peer: usize,
        max_total_cids: usize,
        max_peers: usize,
    ) -> Self {
        let max_cids = if max_cids_per_peer == 0 {
            tracing::warn!(
                "max_cids_per_peer was 0, using default value {}",
                DEFAULT_MAX_CIDS_PER_PEER
            );
            DEFAULT_MAX_CIDS_PER_PEER
        } else {
            max_cids_per_peer
        };
        let max_total = if max_total_cids == 0 {
            tracing::warn!(
                "max_total_cids was 0, using default value {}",
                DEFAULT_MAX_TOTAL_CIDS
            );
            DEFAULT_MAX_TOTAL_CIDS
        } else {
            max_total_cids
        };
        let max_p = if max_peers == 0 {
            tracing::warn!("max_peers was 0, using default value {}", DEFAULT_MAX_PEERS);
            DEFAULT_MAX_PEERS
        } else {
            max_peers
        };
        Self {
            peers: HopscotchMap::with_hasher(RandomState::default()),
            epoch: Instant::now(),
            total_cids: AtomicUsize::new(0),
            peer_ttl,
            max_cids_per_peer: max_cids,
            max_total_cids: max_total,
            max_peers: max_p,
        }
    }

    pub(super) fn now(&self) -> u64 {
        self.epoch.elapsed().as_nanos() as u64
    }

    /// The peer's entry, created on first sight.
    pub(super) fn peer_entry(&self, peer_id: &str) -> Arc<PeerInfo> {
        match self.peers.get(peer_id) {
            Some(info) => info,
            None => self
                .peers
                .get_or_insert(Arc::from(peer_id), Arc::new(PeerInfo::new(self.now()))),
        }
    }

    /// Remove a peer entry and release its CIDs from the global count.
    pub(super) fn remove_peer(&self, peer_id: &str) {
        if let Some(info) = self.peers.remove(peer_id) {
            let released = info.known_cids.len();
            self.total_cids
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |total| {
                    Some(total.saturating_sub(released))
                })
                .ok();
        }
    }
}
