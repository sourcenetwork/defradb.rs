//! Bounded retry and provider rotation for poll-based DAG fetches (#1093).

use std::time::Duration;

use crate::sync::manager::SyncDiagnostics;
use crate::transport::PeerId;
use rapidhash::{HashMapExt, HashSetExt};

/// Total fetch attempts per root before the failure is escalated to ERROR.
///
/// Each attempt spends up to 20s on the CAR phase (a 10s bounded request
/// round-trip plus a 10s arrival watch), a stall budget of one full provider
/// rotation (providers × 30s) on stalled selective windows, and at most
/// `MAX_FREE_REFUSALS` fast refusals (each under `FAST_REFUSAL`, so together
/// about one more window), so three attempts bound the worst-case
/// dead-provider task lifetime at roughly 9 minutes at the 4-provider cap —
/// independent of how many batches the missing frontier spans — while still
/// surviving a transiently overloaded or reconnecting provider set.
/// `proofs/tla/DagFetchBudget.tla` models this bound.
pub(super) const MAX_FETCH_ATTEMPTS: u32 = 3;

/// A failure reported within this long is a refusal, not a stall; well below
/// the transport's 10s connect and 30s response timeouts.
pub(super) const FAST_REFUSAL: Duration = Duration::from_secs(3);

/// Fast refusals one attempt absorbs before they spend stall budget, so a
/// provider set that refuses every batch cannot stretch an attempt.
pub(super) const MAX_FREE_REFUSALS: usize = 11;

/// Base backoff between attempts; doubles per retry (2s, then 4s).
///
/// The backoff mainly de-synchronizes fleet-wide retry waves — the per-batch
/// 30s fetch window is the dominant wait, not the backoff itself.
const FETCH_RETRY_BACKOFF_BASE: Duration = Duration::from_secs(2);

/// Shed attempts retried within one dispatch before deferring to the per-root
/// clock: rides out a momentary saturation without hammering a saturated hub.
pub(super) const BUSY_RETRIES_PER_DISPATCH: u32 = 1;

/// Backoff to sleep before `attempt` (1-based; only called for attempts >= 2).
pub(super) fn retry_backoff(attempt: u32) -> Duration {
    FETCH_RETRY_BACKOFF_BASE * 2u32.saturating_pow(attempt.saturating_sub(2))
}

/// Round-robin cursor over the fetch providers for one DAG root.
///
/// The cursor persists across batches and attempts so a provider that just
/// timed out is not immediately re-tried while alternates remain.
pub(super) struct ProviderRotation {
    peers: Vec<PeerId>,
    cursor: usize,
    unservable: rapidhash::RapidHashMap<PeerId, cid::Cid>,
    productive: rapidhash::RapidHashSet<PeerId>,
    responsive: rapidhash::RapidHashSet<PeerId>,
    /// Per attempt: providers that shed a request, and whether any delivered.
    busy: rapidhash::RapidHashSet<PeerId>,
    attempt_progress: bool,
}

impl ProviderRotation {
    pub(super) fn new(peers: Vec<PeerId>) -> Self {
        debug_assert!(!peers.is_empty(), "DagFetchContext always has source_peer");
        Self {
            peers,
            cursor: 0,
            unservable: rapidhash::RapidHashMap::new(),
            productive: rapidhash::RapidHashSet::new(),
            responsive: rapidhash::RapidHashSet::new(),
            busy: rapidhash::RapidHashSet::new(),
            attempt_progress: false,
        }
    }

    pub(super) fn begin_attempt(&mut self) {
        self.busy.clear();
        self.attempt_progress = false;
    }

    /// The current provider delivered at least one block for this root.
    pub(super) fn record_progress(&mut self) {
        self.productive.insert(self.current().clone());
        self.attempt_progress = true;
    }

    fn record_busy(&mut self) {
        self.busy.insert(self.current().clone());
    }

    /// The current provider shed the request: skip it for the rest of the
    /// attempt and rotate on without spending stall budget.
    pub(super) fn rotate_past_busy(&mut self, diagnostics: &SyncDiagnostics) {
        self.record_busy();
        self.advance();
        diagnostics.record_provider_rotation();
        diagnostics.record_provider_busy_rotation();
    }

    pub(super) fn current_is_busy(&self) -> bool {
        self.busy.contains(self.current())
    }

    pub(super) fn attempt_was_shed(&self) -> bool {
        !self.busy.is_empty() && !self.attempt_progress
    }

    /// The current provider answered a rooted CAR request during this fetch.
    ///
    /// The CAR request and its response are their own libp2p substreams, so a
    /// reply is positive evidence that the peer is alive on this connection —
    /// evidence Bitswap block delivery cannot give, because a want that is
    /// never transmitted and a want the peer cannot satisfy both arrive as
    /// silence.
    pub(super) fn record_responsive(&mut self) {
        self.responsive.insert(self.current().clone());
    }

    /// The peer answered on a non-Bitswap substream yet delivered no block
    /// across every window of this fetch.
    pub(super) fn is_responsive_but_silent(&self, peer: &PeerId) -> bool {
        self.responsive.contains(peer) && !self.productive.contains(peer)
    }

    pub(super) fn current(&self) -> &PeerId {
        &self.peers[self.cursor % self.peers.len()]
    }

    pub(super) fn advance(&mut self) {
        self.cursor = (self.cursor + 1) % self.peers.len();
    }

    pub(super) fn len(&self) -> usize {
        self.peers.len()
    }

    pub(super) fn peers(&self) -> &[PeerId] {
        &self.peers
    }

    pub(super) fn record_size_limit(&mut self, cid: cid::Cid) {
        self.unservable.insert(self.current().clone(), cid);
    }

    pub(super) fn cannot_serve(&self, cids: &[cid::Cid]) -> bool {
        self.unservable
            .get(self.current())
            .is_some_and(|cid| cids.contains(cid))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peer(id: &str) -> PeerId {
        PeerId::new(id.to_string())
    }

    #[test]
    fn provider_rotation_wraps_around() {
        let mut rotation = ProviderRotation::new(vec![peer("a"), peer("b"), peer("c")]);
        assert_eq!(rotation.len(), 3);
        assert_eq!(rotation.current(), &peer("a"));
        rotation.advance();
        assert_eq!(rotation.current(), &peer("b"));
        rotation.advance();
        assert_eq!(rotation.current(), &peer("c"));
        rotation.advance();
        assert_eq!(rotation.current(), &peer("a"));
    }

    #[test]
    fn busy_marks_last_one_attempt_and_progress_clears_the_shed_verdict() {
        let mut rotation = ProviderRotation::new(vec![peer("a"), peer("b")]);
        rotation.begin_attempt();
        rotation.record_busy();
        assert!(rotation.current_is_busy());
        assert!(rotation.attempt_was_shed());
        rotation.advance();
        assert!(!rotation.current_is_busy());
        rotation.record_progress();
        assert!(!rotation.attempt_was_shed());
        rotation.begin_attempt();
        rotation.advance();
        assert!(!rotation.current_is_busy(), "a new attempt asks a again");
        assert!(!rotation.attempt_was_shed());
    }

    #[test]
    fn retry_backoff_doubles_per_attempt() {
        assert_eq!(retry_backoff(2), Duration::from_secs(2));
        assert_eq!(retry_backoff(3), Duration::from_secs(4));
    }
}
