//! Deadline queue for outstanding requests, ordered by expiry.

use std::collections::BTreeMap;

use cid::Cid;
use libp2p::PeerId;
use tokio::time::Instant;

#[derive(Debug, Default)]
pub(crate) struct Timeouts(BTreeMap<(Instant, u64), (PeerId, Cid)>);

impl Timeouts {
    pub(crate) fn insert(&mut self, deadline: Instant, seq: u64, peer: PeerId, cid: Cid) {
        self.0.insert((deadline, seq), (peer, cid));
    }

    pub(crate) fn remove(&mut self, deadline: Option<Instant>, seq: u64) {
        if let Some(deadline) = deadline {
            self.0.remove(&(deadline, seq));
        }
    }

    /// Pops the earliest request whose deadline is not after `now`.
    pub(crate) fn pop_expired(&mut self, now: Instant) -> Option<(PeerId, Cid)> {
        let ((deadline, _), _) = self.0.first_key_value()?;
        if *deadline > now {
            return None;
        }
        self.0.pop_first().map(|(_, request)| request)
    }

    pub(crate) fn next(&self) -> Option<Instant> {
        self.0.first_key_value().map(|((deadline, _), _)| *deadline)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
