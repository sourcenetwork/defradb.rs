//! What the tracker remembers about each peer, in wall-clock time.

use chrono::{DateTime, Utc};

/// A wall-clock instant, serialized as RFC 3339.
pub type Timestamp = DateTime<Utc>;

/// One peer's line in `SyncStatus::peers`: enough to judge its health at a
/// glance.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PeerSummary {
    pub peer_id: String,
    pub connected: bool,
    /// Last time the peer connected, disconnected, announced a block, or
    /// completed a sync with this node.
    pub last_seen_at: Timestamp,
    /// Last time a block this peer sent us merged into the local database.
    pub last_block_received_at: Option<Timestamp>,
    /// Last time this peer acknowledged a document head this node pushed.
    pub last_head_acked_at: Option<Timestamp>,
}

/// Everything the tracker remembers about one peer.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct PeerHistory {
    pub connected: bool,
    pub last_seen_at: Option<Timestamp>,
    pub connect_count: u64,
    pub disconnect_count: u64,
    pub last_connected_at: Option<Timestamp>,
    pub last_disconnected_at: Option<Timestamp>,
    /// Blocks this peer sent us, by merge outcome.
    pub blocks_received: SyncOutcomes,
    /// Document heads this node pushed to the peer, by acknowledgement.
    pub heads_pushed: SyncOutcomes,
    /// Blocks the peer announced or was sent, up to the per-peer cap.
    pub known_block_count: usize,
    /// Pubsub topics the peer subscribed to.
    pub subscribed_topics: Vec<String>,
}

/// Successes and failures of one sync direction with one peer.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct SyncOutcomes {
    pub succeeded: u64,
    pub last_succeeded_at: Option<Timestamp>,
    pub failed: u64,
    pub last_failure: Option<PeerFailure>,
}

/// The most recent failure in one sync direction.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PeerFailure {
    pub at: Timestamp,
    pub error: String,
}
