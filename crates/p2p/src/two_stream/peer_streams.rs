//! A per-peer cap on inbound streams running at once (`max_streams_per_peer`).
//!
//! A stream past the cap is reset at once, as Go's resource manager resets it
//! with StreamResourceLimitExceeded; an admitted stream holds its slot until
//! its handler ends, however it ends. Modeled by
//! `proofs/tla/PeerStreamCap.tla`.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use kovan_map::HopscotchMap;
use libp2p::PeerId;
use rapidhash::fast::RandomState;

/// Inbound streams running per peer, against an optional cap.
pub(crate) struct PeerStreams {
    cap: Option<usize>,
    // ponytail: one counter per peer ever seen, never evicted; evict at zero
    // if a node meets enough distinct peers for that to matter.
    running: HopscotchMap<PeerId, Arc<AtomicUsize>, RandomState>,
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
    pub(crate) fn new(cap: Option<usize>) -> Self {
        Self {
            cap,
            running: HopscotchMap::with_hasher(RandomState::default()),
        }
    }

    /// A slot for a stream from `peer`, or `None` when the peer already runs
    /// `cap` streams. With no cap every stream is admitted.
    pub(crate) fn admit(&self, peer: PeerId) -> Option<StreamAdmission> {
        let Some(cap) = self.cap else {
            return Some(StreamAdmission(None));
        };
        let count = self
            .running
            .get_or_insert(peer, Arc::new(AtomicUsize::new(0)));
        count
            .try_update(Ordering::AcqRel, Ordering::Acquire, |running| {
                (running < cap).then_some(running + 1)
            })
            .ok()
            .map(|_| StreamAdmission(Some(count)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_peer_runs_at_most_cap_streams_and_gets_slots_back() {
        let streams = PeerStreams::new(Some(2));
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
    fn without_a_cap_every_stream_is_admitted() {
        let streams = PeerStreams::new(None);
        let peer = PeerId::random();
        let held: Vec<_> = (0..100).map(|_| streams.admit(peer).unwrap()).collect();
        assert_eq!(held.len(), 100);
    }
}
