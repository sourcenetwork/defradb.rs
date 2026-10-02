//! The per-cid get state machine: the first provider is asked for the block, the rest are asked whether
//! they have it, and a peer that has it is asked for the block when none is in flight.

use libp2p::PeerId;
use rapidhash::{RapidHashMap, RapidHashSet};
use tokio::time::Instant;

use super::fetch::FetchId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Have,
    Block,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Slot {
    pub(crate) kind: Kind,
    pub(crate) seq: u64,
    pub(crate) deadline: Option<Instant>,
}

#[derive(Debug, Default)]
pub(crate) struct Get {
    pub(crate) waiters: Vec<FetchId>,
    pub(crate) requests: RapidHashMap<PeerId, Slot>,
    asked: RapidHashSet<PeerId>,
    candidates: Vec<PeerId>,
    blocking: bool,
}

impl Get {
    /// The requests to issue for providers not asked yet: a block request while none is in flight, a have
    /// request otherwise.
    pub(crate) fn provide(&mut self, providers: &[PeerId]) -> Vec<(PeerId, Kind)> {
        let mut issued = Vec::new();
        for peer in providers {
            if !self.asked.insert(*peer) {
                continue;
            }
            let kind = if self.blocking {
                Kind::Have
            } else {
                self.blocking = true;
                Kind::Block
            };
            issued.push((*peer, kind));
        }
        issued
    }

    /// Records an answered `kind` request to `peer` (already removed from `requests`); returns the peer to
    /// ask for the block next, if any.
    pub(crate) fn answered(&mut self, kind: Kind, peer: PeerId, have: bool) -> Option<PeerId> {
        if kind == Kind::Block {
            self.blocking = false;
        }
        if have {
            self.candidates.push(peer);
        }
        if self.blocking {
            return None;
        }
        let next = self.candidates.pop()?;
        self.blocking = true;
        Some(next)
    }
}
