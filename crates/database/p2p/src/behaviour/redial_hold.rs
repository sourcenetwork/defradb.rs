//! Peers Kademlia lost its last connection to, which it must not redial yet.
//!
//! rust-libp2p's Kademlia bootstraps about 500ms after a new peer enters a
//! small routing table, and the bootstrap dials every known peer. After a
//! connection manager trim that redials, from either end, the peers the trim
//! just closed, and the next pass trims again (#3357). go-libp2p's DHT does
//! not dial on a new routing-table peer, so a pruned peer stays away from a Go
//! node. Holding Kademlia's own dials to a peer for go-libp2p connmgr's
//! silence period after its last connection closes restores that. Only
//! Kademlia's dials are held: an explicit connect, a replicator push or a
//! Bitswap fetch still reaches the peer, and the peer stays in the routing
//! table.

use rapidhash::RapidHashMap;

use crate::rt::time::Instant;
use libp2p::PeerId;

/// How long Kademlia holds its dials to a peer after its last connection
/// closes: go-libp2p connmgr's `silencePeriod`, the least time between trims.
pub(crate) const REDIAL_HOLD: std::time::Duration = std::time::Duration::from_secs(10);

#[derive(Debug, Default)]
pub(crate) struct RedialHold {
    until: RapidHashMap<PeerId, Instant>,
}

impl RedialHold {
    /// Hold dials to `peer` for [`REDIAL_HOLD`] from `now`, forgetting
    /// expired holds so the map stays as small as the peers lost recently.
    pub(crate) fn hold(&mut self, peer: PeerId, now: Instant) {
        self.forget_expired(now);
        self.until.insert(peer, now + REDIAL_HOLD);
    }

    /// Whether a dial to `peer` at `now` is held.
    pub(crate) fn is_held(&mut self, peer: &PeerId, now: Instant) -> bool {
        self.forget_expired(now);
        self.until.contains_key(peer)
    }

    fn forget_expired(&mut self, now: Instant) {
        self.until.retain(|_, until| *until > now);
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

        assert_eq!(hold.until.len(), 1);
        assert!(hold.is_held(&peer, now + REDIAL_HOLD));
    }
}
