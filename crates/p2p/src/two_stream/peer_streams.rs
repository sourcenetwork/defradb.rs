//! A per-peer cap on inbound streams running at once (`max_streams_per_peer`).
//!
//! A stream past the cap is reset at once, as Go's resource manager resets it
//! with StreamResourceLimitExceeded; an admitted stream holds its slot until
//! its handler ends, however it ends. Modeled by
//! `proofs/tla/PeerStreamCap.tla`.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use libp2p::PeerId;
use rapidhash::{HashMapExt, RapidHashMap};

/// The fewest counters kept before idle ones are dropped.
const MIN_PRUNE_AT: usize = 64;

/// Inbound streams running per peer, against an optional cap. Only the
/// runner admits, so only it can raise a counter: a counter it finds at zero
/// has no stream left to give a slot back, and is dropped with its peer.
pub(crate) struct PeerStreams {
    cap: Option<usize>,
    running: RapidHashMap<PeerId, Arc<AtomicUsize>>,
    // Idle counters are dropped once this many peers are counted, so the map
    // holds at most twice the peers with a stream running.
    prune_at: usize,
}

/// One admitted stream's slot, given back when dropped.
pub(crate) struct StreamAdmission(Option<Arc<AtomicUsize>>);

impl Drop for StreamAdmission {
    fn drop(&mut self) {
        if let Some(count) = self.0.take() {
            count.fetch_sub(1, Ordering::AcqRel);
        }
    }
}

impl PeerStreams {
    /// A cap of zero is unset: every stream is admitted.
    pub(crate) fn new(cap: Option<usize>) -> Self {
        Self {
            cap: cap.filter(|&cap| cap > 0),
            running: RapidHashMap::new(),
            prune_at: MIN_PRUNE_AT,
        }
    }

    /// A slot for a stream from `peer`, or `None` when the peer already runs
    /// `cap` streams. With no cap every stream is admitted.
    pub(crate) fn admit(&mut self, peer: PeerId) -> Option<StreamAdmission> {
        let Some(cap) = self.cap else {
            return Some(StreamAdmission(None));
        };
        if self.running.len() >= self.prune_at {
            self.running
                .retain(|_, count| count.load(Ordering::Acquire) > 0);
            self.prune_at = MIN_PRUNE_AT.max(self.running.len() * 2);
        }
        let count = self.running.entry(peer).or_default();
        if count.load(Ordering::Acquire) >= cap {
            return None;
        }
        count.fetch_add(1, Ordering::AcqRel);
        Some(StreamAdmission(Some(Arc::clone(count))))
    }

    /// How many peers have a counter.
    #[cfg(test)]
    fn counted(&self) -> usize {
        self.running.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_peer_runs_at_most_cap_streams_and_gets_slots_back() {
        let mut streams = PeerStreams::new(Some(2));
        let (peer, other) = (PeerId::random(), PeerId::random());
        let first = streams.admit(peer).unwrap();
        let second = streams.admit(peer).unwrap();
        assert!(streams.admit(peer).is_none(), "past the cap");
        assert!(
            streams.admit(other).is_some(),
            "another peer has its own cap"
        );
        drop(first);
        let third = streams.admit(peer).unwrap();
        drop((second, third));
        assert!(streams.admit(peer).is_some());
    }

    #[test]
    fn a_cap_of_zero_is_unset_and_admits_every_stream() {
        let mut streams = PeerStreams::new(Some(0));
        let peer = PeerId::random();
        let held: Vec<_> = (0..100).map(|_| streams.admit(peer).unwrap()).collect();
        assert_eq!(held.len(), 100);
    }

    #[test]
    fn without_a_cap_every_stream_is_admitted() {
        let mut streams = PeerStreams::new(None);
        let peer = PeerId::random();
        let held: Vec<_> = (0..100).map(|_| streams.admit(peer).unwrap()).collect();
        assert_eq!(held.len(), 100);
    }

    #[test]
    fn idle_peers_are_dropped_and_busy_ones_kept() {
        let mut streams = PeerStreams::new(Some(1));
        let busy = PeerId::random();
        let held = streams.admit(busy).unwrap();
        for _ in 0..10 * MIN_PRUNE_AT {
            drop(streams.admit(PeerId::random()).unwrap());
        }
        assert!(streams.counted() <= MIN_PRUNE_AT, "idle peers are dropped");
        assert!(streams.admit(busy).is_none(), "a busy peer keeps its count");
        drop(held);
        assert!(streams.admit(busy).is_some());
    }
}
