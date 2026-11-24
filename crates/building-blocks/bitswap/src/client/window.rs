//! The per-peer window of outstanding wants. Go's bitswap server queues a bounded number of wants per peer
//! and silently drops the rest, so no peer is ever sent more than [`MAX_OUTSTANDING_WANTS_PER_PEER`] wants
//! at once; the rest wait in a per-peer FIFO backlog and go out as slots free.
//!
//! A slot is taken when a want enters the outbox and freed when the want is gone from the remote wantlist:
//! the peer served it, we handed the peer a cancel for it, the send failed, or the peer disconnected.

use std::collections::VecDeque;

use cid::Cid;
use libp2p::PeerId;

use super::query::{Kind, Slot};
use super::{Client, FetchId, KeepAlive};

/// The most wants a peer has outstanding from us across all fetches: half of Go's per-peer queue cap, which
/// leaves room for wants the remote has not yet dropped.
pub const MAX_OUTSTANDING_WANTS_PER_PEER: usize = 512;

#[derive(Debug, Default)]
pub(crate) struct Window {
    used: usize,
    /// Requests waiting for a slot as `(cid, request sequence)`; entries of ended requests are skipped on pop.
    backlog: VecDeque<(Cid, u64)>,
}

impl Client {
    pub(super) fn issue(&mut self, get: &mut super::Get, cid: Cid, peer: PeerId, kind: Kind) {
        let seq = self.next_seq;
        self.next_seq += 1;
        let window = self.windows.entry(peer).or_default();
        let queued = window.used >= MAX_OUTSTANDING_WANTS_PER_PEER || !window.backlog.is_empty();
        if queued {
            window.backlog.push_back((cid, seq));
        } else {
            window.used += 1;
            self.outbox.want(peer, cid, kind, seq);
        }
        get.requests.insert(
            peer,
            Slot {
                kind,
                seq,
                deadline: None,
                queued,
            },
        );
        let cids = self.by_peer.entry(peer).or_default();
        if cids.is_empty() {
            self.keep_alive.push(KeepAlive::Protect(peer));
        }
        cids.insert(cid);
    }

    /// Ends `peer`'s request for `cid`: drops its deadline, sends a cancel when the remote may still hold the
    /// want, and frees its window slot.
    pub(super) fn end_request(&mut self, peer: PeerId, cid: &Cid, slot: &Slot, cancel: bool) {
        self.deadlines.remove(slot.deadline, slot.seq);
        if let Some(cids) = self.by_peer.get_mut(&peer) {
            cids.remove(cid);
            if cids.is_empty() {
                self.by_peer.remove(&peer);
                self.keep_alive.push(KeepAlive::Unprotect(peer));
            }
        }
        if !slot.queued {
            if cancel {
                self.outbox.cancel(peer, *cid);
            }
            if let Some(window) = self.windows.get_mut(&peer) {
                window.used = window.used.saturating_sub(1);
            }
        }
        self.refill.insert(peer);
    }

    /// Issues backlogged requests into the slots freed since the last call.
    pub(super) fn settle(&mut self) {
        while let Some(peer) = self.refill.iter().next().copied() {
            self.refill.remove(&peer);
            self.refill_peer(peer);
        }
    }

    fn refill_peer(&mut self, peer: PeerId) {
        while self
            .windows
            .get(&peer)
            .is_some_and(|window| window.used < MAX_OUTSTANDING_WANTS_PER_PEER)
        {
            let Some((cid, seq)) = self
                .windows
                .get_mut(&peer)
                .and_then(|window| window.backlog.pop_front())
            else {
                break;
            };
            self.cancel_abandoned(&cid);
            let Some(mut get) = self.gets.remove(&cid) else {
                continue;
            };
            if let Some(slot) = get
                .requests
                .get_mut(&peer)
                .filter(|slot| slot.queued && slot.seq == seq)
            {
                slot.queued = false;
                self.outbox.want(peer, cid, slot.kind, seq);
                if let Some(window) = self.windows.get_mut(&peer) {
                    window.used += 1;
                }
            }
            self.gets.insert(cid, get);
        }
        if self
            .windows
            .get(&peer)
            .is_some_and(|window| window.used == 0 && window.backlog.is_empty())
        {
            self.windows.remove(&peer);
        }
    }

    /// Cancels every fetch waiting on `cid` whose receiver was dropped.
    fn cancel_abandoned(&mut self, cid: &Cid) {
        let abandoned: Vec<FetchId> = self
            .gets
            .get(cid)
            .map(|get| {
                get.waiters
                    .iter()
                    .filter(|id| self.fetches.get(id).is_none_or(|f| f.sender.is_closed()))
                    .copied()
                    .collect()
            })
            .unwrap_or_default();
        for id in abandoned {
            self.cancel_fetch(id);
        }
    }
}
