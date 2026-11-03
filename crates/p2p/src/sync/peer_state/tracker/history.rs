//! Per-peer sync outcomes and the history and summary views over them.

use std::sync::atomic::{AtomicU64, Ordering};

use chrono::DateTime;
use kovan::AtomOption;

use super::{PeerInfo, PeerStateTracker};
use crate::sync::peer_state::history::{
    PeerFailure, PeerHistory, PeerSummary, SyncOutcomes, Timestamp,
};

/// Success and failure counts for one sync direction, updated in place.
pub(in crate::sync::peer_state) struct OutcomeCounters {
    succeeded: AtomicU64,
    /// Unix milliseconds; 0 = never
    last_succeeded_at_ms: AtomicU64,
    failed: AtomicU64,
    last_failure: AtomOption<(u64, String)>,
}

impl OutcomeCounters {
    pub(super) fn new() -> Self {
        Self {
            succeeded: AtomicU64::new(0),
            last_succeeded_at_ms: AtomicU64::new(0),
            failed: AtomicU64::new(0),
            last_failure: AtomOption::none(),
        }
    }

    fn succeed(&self, at_ms: u64) {
        self.succeeded.fetch_add(1, Ordering::Relaxed);
        self.last_succeeded_at_ms.store(at_ms, Ordering::Relaxed);
    }

    fn fail(&self, at_ms: u64, error: String) {
        self.failed.fetch_add(1, Ordering::Relaxed);
        self.last_failure.store_some((at_ms, error));
    }

    fn last_succeeded_at(&self) -> Option<Timestamp> {
        timestamp(self.last_succeeded_at_ms.load(Ordering::Relaxed))
    }

    fn snapshot(&self) -> SyncOutcomes {
        SyncOutcomes {
            succeeded: self.succeeded.load(Ordering::Relaxed),
            last_succeeded_at: self.last_succeeded_at(),
            failed: self.failed.load(Ordering::Relaxed),
            last_failure: self.last_failure.load().and_then(|failure| {
                Some(PeerFailure {
                    at: timestamp(failure.0)?,
                    error: failure.1.clone(),
                })
            }),
        }
    }
}

/// A Unix-millisecond field as a timestamp; 0 means never.
fn timestamp(unix_ms: u64) -> Option<Timestamp> {
    (unix_ms != 0)
        .then(|| DateTime::from_timestamp_millis(unix_ms as i64))
        .flatten()
}

impl PeerStateTracker {
    /// Record that a block this peer sent merged into the local database.
    ///
    /// Creates a peer entry if one doesn't exist.
    pub fn record_block_received(&self, peer_id: &str) {
        let info = self.peer_entry(peer_id);
        info.blocks_received.succeed(self.touch(&info));
        self.enforce_global_limits();
    }

    /// Record that a block this peer sent failed to merge.
    ///
    /// Creates a peer entry if one doesn't exist.
    pub fn record_block_failed(&self, peer_id: &str, error: String) {
        let info = self.peer_entry(peer_id);
        info.blocks_received.fail(self.touch(&info), error);
        self.enforce_global_limits();
    }

    /// Record that the peer acknowledged a document head this node pushed.
    ///
    /// Creates a peer entry if one doesn't exist.
    pub fn record_head_acked(&self, peer_id: &str) {
        let info = self.peer_entry(peer_id);
        info.heads_pushed.succeed(self.touch(&info));
        self.enforce_global_limits();
    }

    /// Record that pushing a document head to the peer failed.
    ///
    /// Creates a peer entry if one doesn't exist.
    pub fn record_head_push_failed(&self, peer_id: &str, error: String) {
        let info = self.peer_entry(peer_id);
        info.heads_pushed.fail(self.touch(&info), error);
        self.enforce_global_limits();
    }

    /// One summary per tracked peer, sorted by peer ID.
    pub fn summaries(&self) -> Vec<PeerSummary> {
        let mut peers: Vec<PeerSummary> = self
            .peers
            .iter()
            .filter_map(|(peer_id, info)| {
                Some(PeerSummary {
                    peer_id: peer_id.to_string(),
                    connected: info.is_connected(),
                    last_seen_at: self.last_seen_at(&info)?,
                    last_block_received_at: info.blocks_received.last_succeeded_at(),
                    last_head_acked_at: info.heads_pushed.last_succeeded_at(),
                })
            })
            .collect();
        peers.sort_by(|a, b| a.peer_id.cmp(&b.peer_id));
        peers
    }

    /// Everything tracked about one peer, or `None` if it is not tracked.
    pub fn history(&self, peer_id: &str) -> Option<PeerHistory> {
        let info = self.peers.get(peer_id)?;
        Some(PeerHistory {
            connected: info.is_connected(),
            last_seen_at: self.last_seen_at(&info),
            connect_count: info.connect_count.load(Ordering::Relaxed),
            disconnect_count: info.disconnect_count.load(Ordering::Relaxed),
            last_connected_at: timestamp(info.last_connected_at_ms.load(Ordering::Relaxed)),
            last_disconnected_at: timestamp(info.last_disconnected_at_ms.load(Ordering::Relaxed)),
            blocks_received: info.blocks_received.snapshot(),
            heads_pushed: info.heads_pushed.snapshot(),
            known_block_count: info.known_cids.len(),
            subscribed_topics: {
                let mut topics: Vec<String> = info.subscribed_collections.keys().collect();
                topics.sort();
                topics
            },
        })
    }

    fn last_seen_at(&self, info: &PeerInfo) -> Option<Timestamp> {
        timestamp(self.unix_ms(info.last_seen()))
    }
}
