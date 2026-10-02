//! The bitswap client: fetches blocks from providers with a per-cid get state machine and coalesces the
//! resulting requests into one message per peer.
//!
//! [`Client`] is plain state owned by the behaviour and driven through `&mut self`; it performs no I/O.
//! Invariant: every map is keyed by something outstanding, so all state is removed when its fetch or
//! request completes and the client is [idle](Client::is_idle) once nothing is pending.

mod driver;
mod fetch;
mod outbox;
mod query;
mod timeouts;

use std::hash::Hash;
use std::time::Duration;

use cid::Cid;
use libp2p::PeerId;
use rapidhash::{RapidHashMap, RapidHashSet};
use tokio::sync::mpsc;
use tokio::time::Instant;
use tracing::trace;

use crate::block::Block;
use crate::message::{BitswapMessage, BlockPresenceType};
use crate::protocol::ProtocolId;
use fetch::Fetch;
use query::{Get, Kind, Slot};
use timeouts::Timeouts;

pub(crate) use driver::Driver;
pub use fetch::FetchId;
use outbox::Outbox;
pub use outbox::Outgoing;

/// How long a request waits for its answer, counted from the moment its message was sent, before it counts
/// as a no.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// A change of the connection keep-alive a peer needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeepAlive {
    /// The peer has its first outstanding request.
    Protect(PeerId),
    /// The peer has no outstanding request left.
    Unprotect(PeerId),
}

/// The client state: fetches, per-cid gets, requests per peer and the outgoing queue.
#[derive(Debug, Default)]
pub struct Client {
    next_fetch: u64,
    next_seq: u64,
    fetches: RapidHashMap<FetchId, Fetch>,
    gets: RapidHashMap<Cid, Get>,
    by_peer: RapidHashMap<PeerId, RapidHashSet<Cid>>,
    deadlines: Timeouts,
    outbox: Outbox,
    keep_alive: Vec<KeepAlive>,
}

fn dedup<T: Hash + Eq + Copy>(items: Vec<T>) -> Vec<T> {
    let mut seen = RapidHashSet::default();
    items
        .into_iter()
        .filter(|item| seen.insert(*item))
        .collect()
}

impl Client {
    /// An idle client.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether nothing is pending anywhere.
    pub fn is_idle(&self) -> bool {
        self.fetches.is_empty()
            && self.gets.is_empty()
            && self.by_peer.is_empty()
            && self.deadlines.is_empty()
            && self.outbox.is_empty()
    }

    /// Starts fetching `cids` from `providers`. Each block arrives once on the receiver, which closes when
    /// every cid is resolved; cids no provider has are skipped.
    pub fn fetch(
        &mut self,
        cids: Vec<Cid>,
        providers: Vec<PeerId>,
    ) -> (FetchId, mpsc::Receiver<Block>) {
        let id = FetchId(self.next_fetch);
        self.next_fetch += 1;
        let cids = dedup(cids);
        let providers = dedup(providers);
        let (sender, receiver) = mpsc::channel(cids.len().max(1));
        if cids.is_empty() || providers.is_empty() {
            return (id, receiver);
        }
        let pending = cids.iter().copied().collect();
        self.fetches.insert(id, Fetch { sender, pending });
        for cid in cids {
            let mut get = self.gets.remove(&cid).unwrap_or_default();
            get.waiters.push(id);
            for (peer, kind) in get.provide(&providers) {
                self.issue(&mut get, cid, peer, kind);
            }
            self.gets.insert(cid, get);
        }
        (id, receiver)
    }

    /// Cancels a fetch; true when it was live.
    pub fn cancel(&mut self, id: FetchId) -> bool {
        let Some(fetch) = self.fetches.remove(&id) else {
            return false;
        };
        for cid in fetch.pending {
            let Some(mut get) = self.gets.remove(&cid) else {
                continue;
            };
            get.waiters.retain(|waiter| *waiter != id);
            if get.waiters.is_empty() {
                self.release(cid, get, None);
            } else {
                self.gets.insert(cid, get);
            }
        }
        true
    }

    /// Feeds a verified inbound message: blocks complete gets, presences answer requests.
    pub fn on_message(&mut self, peer: &PeerId, message: &BitswapMessage) {
        if self.gets.is_empty() {
            return;
        }
        for block in message.blocks() {
            if let Some(get) = self.gets.remove(block.cid()) {
                self.release(block.cid, get, Some((block, *peer)));
            }
        }
        for presence in message.block_presences() {
            self.answer(*peer, presence.cid, presence.typ == BlockPresenceType::Have);
        }
    }

    /// Counts every request outstanding to `peer` as a no.
    pub fn on_peer_disconnected(&mut self, peer: &PeerId) {
        self.outbox.forget(peer);
        let cids: Vec<Cid> = self
            .by_peer
            .get(peer)
            .map(|cids| cids.iter().copied().collect())
            .unwrap_or_default();
        for cid in cids {
            self.answer(*peer, cid, false);
        }
    }

    /// Re-announces the keep-alive of a peer that just became reachable over bitswap, since a protect sent
    /// earlier was dropped while the peer was not.
    pub fn on_responsive(&mut self, peer: &PeerId) {
        if self.by_peer.contains_key(peer) {
            self.keep_alive.push(KeepAlive::Protect(*peer));
        }
    }

    /// A send finished; on success every request it carried starts its response deadline, on failure it
    /// counts as a no.
    pub fn send_done(&mut self, peer: PeerId, requests: &[(Cid, u64)], ok: bool) {
        self.outbox.done(&peer);
        let sent_at = Instant::now();
        for (cid, seq) in requests {
            let Some(slot) = self
                .gets
                .get_mut(cid)
                .and_then(|get| get.requests.get_mut(&peer))
                .filter(|slot| slot.seq == *seq)
            else {
                continue;
            };
            if ok {
                let deadline = sent_at + REQUEST_TIMEOUT;
                slot.deadline = Some(deadline);
                self.deadlines.insert(deadline, *seq, peer, *cid);
            } else {
                self.answer(peer, *cid, false);
            }
        }
    }

    /// Counts every request past its deadline as a no.
    pub fn expire(&mut self) {
        let now = Instant::now();
        while let Some((peer, cid)) = self.deadlines.pop_expired(now) {
            trace!(%peer, %cid, "request timed out");
            self.answer(peer, cid, false);
        }
    }

    /// The earliest request deadline.
    pub fn next_deadline(&self) -> Option<Instant> {
        self.deadlines.next()
    }

    /// One message per idle peer with queued wants or cancels; `protocol` is the best protocol known for a
    /// peer.
    pub fn take_ready(
        &mut self,
        protocol: impl Fn(&PeerId) -> Option<ProtocolId>,
    ) -> Vec<Outgoing> {
        self.outbox.take_ready(protocol)
    }

    /// Keep-alive changes since the last call, in order.
    pub fn take_keep_alive(&mut self) -> Vec<KeepAlive> {
        std::mem::take(&mut self.keep_alive)
    }

    fn issue(&mut self, get: &mut Get, cid: Cid, peer: PeerId, kind: Kind) {
        let seq = self.next_seq;
        self.next_seq += 1;
        get.requests.insert(
            peer,
            Slot {
                kind,
                seq,
                deadline: None,
            },
        );
        let cids = self.by_peer.entry(peer).or_default();
        if cids.is_empty() {
            self.keep_alive.push(KeepAlive::Protect(peer));
        }
        cids.insert(cid);
        self.outbox.want(peer, cid, kind, seq);
    }

    fn forget_request(&mut self, peer: PeerId, cid: &Cid, slot: &Slot) {
        self.deadlines.remove(slot.deadline, slot.seq);
        if let Some(cids) = self.by_peer.get_mut(&peer) {
            cids.remove(cid);
            if cids.is_empty() {
                self.by_peer.remove(&peer);
                self.keep_alive.push(KeepAlive::Unprotect(peer));
            }
        }
    }

    /// Applies the answer to `peer`'s outstanding request for `cid`; a have from a block request is ignored.
    fn answer(&mut self, peer: PeerId, cid: Cid, have: bool) {
        let Some(mut get) = self.gets.remove(&cid) else {
            return;
        };
        let Some(slot) = get.requests.get(&peer).copied() else {
            self.gets.insert(cid, get);
            return;
        };
        if slot.kind == Kind::Block && have {
            self.gets.insert(cid, get);
            return;
        }
        get.requests.remove(&peer);
        self.forget_request(peer, &cid, &slot);
        if let Some(next) = get.answered(slot.kind, peer, have) {
            self.issue(&mut get, cid, next, Kind::Block);
        }
        if get.requests.is_empty() {
            self.release(cid, get, None);
        } else {
            self.gets.insert(cid, get);
        }
    }

    /// Ends a get: cancels what is still outstanding (except at the peer that delivered) and resolves every
    /// waiting fetch, with the block when there is one.
    fn release(&mut self, cid: Cid, mut get: Get, delivered: Option<(&Block, PeerId)>) {
        for (peer, slot) in get.requests.drain() {
            self.forget_request(peer, &cid, &slot);
            if delivered.is_none_or(|(_, from)| from != peer) {
                self.outbox.cancel(peer, cid);
            }
        }
        for id in get.waiters {
            let Some(fetch) = self.fetches.get_mut(&id) else {
                continue;
            };
            if let Some((block, _)) = delivered {
                if fetch.sender.try_send(block.clone()).is_err() {
                    trace!(%cid, "fetch receiver is gone");
                }
            }
            fetch.pending.remove(&cid);
            if fetch.pending.is_empty() {
                self.fetches.remove(&id);
            }
        }
    }
}
