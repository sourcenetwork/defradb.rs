//! Server tuning.

use std::fmt;
use std::time::Duration;

use super::filter::PeerBlockRequestFilter;

/// Configuration of the bitswap server.
pub struct ServerConfig {
    /// Admission check run for every want.
    pub peer_block_request_filter: Option<Box<dyn PeerBlockRequestFilter>>,
    /// Maximum messages being built or sent at once.
    pub worker_count: usize,
    /// Answer wants for missing blocks with DONT_HAVE when the requester asks for it.
    pub send_dont_haves: bool,
    /// Work to pop per message, in bytes.
    pub target_message_size: usize,
    /// Work a peer may have in flight; 0 disables the cap.
    pub max_outstanding_bytes_per_peer: usize,
    /// Largest block a want-have is answered with the block itself.
    pub max_replace_size: usize,
    /// Wants one peer may keep queued; 0 disables the cap.
    pub max_queued_wantlist_entries_per_peer: usize,
    /// The most time one inbound message may spend on filter checks and block-size lookups.
    pub message_lookup_timeout: Duration,
}

impl fmt::Debug for ServerConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ServerConfig")
            .field(
                "peer_block_request_filter",
                &self.peer_block_request_filter.as_ref().map(|_| "<fn>"),
            )
            .field("worker_count", &self.worker_count)
            .field("send_dont_haves", &self.send_dont_haves)
            .field("target_message_size", &self.target_message_size)
            .field(
                "max_outstanding_bytes_per_peer",
                &self.max_outstanding_bytes_per_peer,
            )
            .field("max_replace_size", &self.max_replace_size)
            .field(
                "max_queued_wantlist_entries_per_peer",
                &self.max_queued_wantlist_entries_per_peer,
            )
            .field("message_lookup_timeout", &self.message_lookup_timeout)
            .finish()
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig {
            peer_block_request_filter: None,
            worker_count: 8,
            send_dont_haves: true,
            target_message_size: 16 * 1024,
            max_outstanding_bytes_per_peer: 1 << 20,
            max_replace_size: 1024,
            max_queued_wantlist_entries_per_peer: 1024,
            message_lookup_timeout: Duration::from_secs(10),
        }
    }
}
