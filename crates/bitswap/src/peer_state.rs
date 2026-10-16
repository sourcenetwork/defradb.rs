//! What the behaviour knows about each peer, and the waiters for a dial in flight.

use std::time::Instant;

use libp2p::swarm::ConnectionId;
use rapidhash::{RapidHashMap, RapidHashSet};
use tokio::sync::oneshot;

use crate::network::DialResult;
use crate::protocol::ProtocolId;
use libp2p::PeerId;

/// Connection state of a peer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PeerState {
    Connected(ConnectionId),
    Responsive(ConnectionId, ProtocolId),
    Disconnected,
    DialFailure(Instant),
}

impl PeerState {
    pub(crate) fn is_connected(self) -> bool {
        matches!(self, PeerState::Connected(_) | PeerState::Responsive(_, _))
    }
}

pub(crate) type Dials = RapidHashMap<PeerId, Vec<(usize, oneshot::Sender<DialResult>)>>;
pub(crate) type Connections = RapidHashMap<PeerId, RapidHashSet<ConnectionId>>;
