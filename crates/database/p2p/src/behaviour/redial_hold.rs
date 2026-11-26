//! Peers Kademlia lost its last connection to, which it must not redial yet.
//!
//! rust-libp2p's Kademlia bootstraps about 500ms after a new peer enters a
//! small routing table, and the bootstrap dials every known peer. After a
//! connection manager trim that redials, from either end, the peers the trim
//! just closed, and the next pass trims again (#3357). go-libp2p's DHT does
//! not dial on a new routing-table peer, so a pruned peer stays away from a Go
//! node. Holding Kademlia's own dials to a peer for go-libp2p connmgr's
//! silence period after its last connection closes restores that.
//!
//! The hold starts on every last-connection close, not only a trim's: the
//! pruned end only sees the remote close its connection, so it cannot tell a
//! trim from a restart or a network blip, and it must hold too. Only
//! Kademlia's dials are held: an explicit connect, a replicator push or a
//! Bitswap fetch still reaches the peer, and the peer stays in the routing
//! table.
//!
//! The table is fed by whichever peers disconnect, so it is bounded: past
//! [`MAX_HELD`] holds the oldest is dropped, and Kademlia may dial that peer
//! again, as it could before #3357.

use std::collections::VecDeque;

use rapidhash::RapidHashMap;

use crate::rt::time::Instant;
use libp2p::PeerId;

/// How long Kademlia holds its dials to a peer after its last connection
/// closes: go-libp2p connmgr's `silencePeriod`, the least time between trims.
pub(crate) const REDIAL_HOLD: std::time::Duration = std::time::Duration::from_secs(10);

/// The most holds kept at once, five times the default connection manager
/// high water, so a trim's pruned peers fit with room to spare.
pub(crate) const MAX_HELD: usize = 2048;

/// One hold taken: its peer, its deadline, and its number among all holds,
/// which tells a peer's latest hold from the earlier ones it replaced.
#[derive(Debug, Clone, Copy)]
struct Taken {
    peer: PeerId,
    until: Instant,
    number: u64,
}

#[derive(Debug, Default)]
pub(crate) struct RedialHold {
    /// Each held peer's latest hold: its deadline and number.
    until: RapidHashMap<PeerId, (Instant, u64)>,
    /// Every hold still kept, oldest first, replaced ones included; its
    /// deadlines rise because each is `now + REDIAL_HOLD` on a monotonic clock.
    taken: VecDeque<Taken>,
    next_number: u64,
    evicted: u64,
}

impl RedialHold {
    /// Hold dials to `peer` for [`REDIAL_HOLD`] from `now`, forgetting
    /// expired holds and, when the table is full, the oldest one.
    pub(crate) fn hold(&mut self, peer: PeerId, now: Instant) {
        self.forget_expired(now);
        if self.taken.len() >= MAX_HELD {
            if let Some(oldest) = self.taken.pop_front() {
                if self.forget(oldest) {
                    self.evicted += 1;
                }
            }
        }
        let taken = Taken {
            peer,
            until: now + REDIAL_HOLD,
            number: self.next_number,
        };
        self.next_number += 1;
        self.until.insert(peer, (taken.until, taken.number));
        self.taken.push_back(taken);
    }

    /// Whether a dial to `peer` at `now` is held.
    pub(crate) fn is_held(&mut self, peer: &PeerId, now: Instant) -> bool {
        self.forget_expired(now);
        self.until.get(peer).is_some_and(|(until, _)| *until > now)
    }

    /// How many peers are held, ignoring holds that ended but are not yet
    /// forgotten.
    pub(crate) fn len(&self) -> usize {
        self.until.len()
    }

    /// How many live holds were dropped to keep the table at [`MAX_HELD`].
    pub(crate) fn evicted(&self) -> u64 {
        self.evicted
    }

    fn forget_expired(&mut self, now: Instant) {
        while let Some(oldest) = self.taken.front().copied() {
            if oldest.until > now {
                break;
            }
            self.taken.pop_front();
            self.forget(oldest);
        }
    }

    /// Drop `taken`'s peer from the held set if `taken` is still its latest
    /// hold, and say whether it was.
    fn forget(&mut self, taken: Taken) -> bool {
        let latest = self
            .until
            .get(&taken.peer)
            .is_some_and(|latest| *latest == (taken.until, taken.number));
        if latest {
            self.until.remove(&taken.peer);
        }
        latest
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn a_hold_lasts_until_its_deadline() {
        let mut hold = RedialHold::default();
        let now = Instant::now();
        let peer = PeerId::random();

        hold.hold(peer, now);

        assert!(hold.is_held(&peer, now));
        assert!(hold.is_held(&peer, now + REDIAL_HOLD - Duration::from_millis(1)));
        assert!(!hold.is_held(&peer, now + REDIAL_HOLD));
        assert!(!hold.is_held(&PeerId::random(), now));
    }

    #[test]
    fn a_later_disconnect_restarts_the_hold() {
        let mut hold = RedialHold::default();
        let now = Instant::now();
        let peer = PeerId::random();

        hold.hold(peer, now);
        let later = now + Duration::from_secs(5);
        hold.hold(peer, later);

        assert!(hold.is_held(&peer, now + REDIAL_HOLD));
        assert!(!hold.is_held(&peer, later + REDIAL_HOLD));
    }

    #[test]
    fn expired_holds_are_forgotten() {
        let mut hold = RedialHold::default();
        let now = Instant::now();

        for _ in 0..8 {
            hold.hold(PeerId::random(), now);
        }
        let peer = PeerId::random();
        hold.hold(peer, now + REDIAL_HOLD);

        assert_eq!(hold.len(), 1);
        assert_eq!(hold.taken.len(), 1);
        assert!(hold.is_held(&peer, now + REDIAL_HOLD));
    }

    /// A full table drops its oldest hold for a new one, and stays full.
    #[test]
    fn a_full_table_evicts_its_oldest_hold() {
        let mut hold = RedialHold::default();
        let now = Instant::now();
        let peers: Vec<PeerId> = (0..MAX_HELD).map(|_| PeerId::random()).collect();
        for (offset, peer) in (0u64..).zip(&peers) {
            hold.hold(*peer, now + Duration::from_micros(offset));
        }
        assert_eq!(hold.len(), MAX_HELD);
        let at = now + Duration::from_secs(1);

        let newest = PeerId::random();
        hold.hold(newest, at);

        assert_eq!(hold.len(), MAX_HELD);
        assert_eq!(hold.taken.len(), MAX_HELD);
        assert_eq!(hold.evicted(), 1);
        assert!(!hold.is_held(&peers[0], at));
        assert!(hold.is_held(&peers[1], at));
        assert!(hold.is_held(&newest, at));
    }

    /// A peer cycling through disconnects fills the table with its own
    /// replaced holds; evicting those drops nothing still held.
    #[test]
    fn evicting_a_replaced_hold_keeps_the_peer_held() {
        let mut hold = RedialHold::default();
        let now = Instant::now();
        let peer = PeerId::random();
        hold.hold(peer, now);
        hold.hold(peer, now + Duration::from_micros(1));
        let other = PeerId::random();
        for offset in 2..MAX_HELD as u64 {
            hold.hold(other, now + Duration::from_micros(offset));
        }
        assert_eq!(hold.taken.len(), MAX_HELD);
        let at = now + Duration::from_secs(1);

        let newest = PeerId::random();
        hold.hold(newest, at);

        assert_eq!(hold.taken.len(), MAX_HELD);
        assert_eq!(hold.evicted(), 0);
        assert!(hold.is_held(&peer, at));
        assert!(hold.is_held(&other, at));
        assert!(hold.is_held(&newest, at));
    }
}
