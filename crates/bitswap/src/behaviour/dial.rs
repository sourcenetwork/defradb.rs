//! Dial bookkeeping and the execution of network requests inside the behaviour.

use std::time::Instant;

use libp2p::swarm::dial_opts::{DialOpts, PeerCondition};
use libp2p::swarm::{DialError, NotifyHandler, ToSwarm};
use libp2p::PeerId;
use tokio::sync::oneshot;
use tracing::{debug, trace};

use super::{Bitswap, BitswapEvent, DIAL_BACK_OFF};
use crate::handler::BitswapHandlerIn;
use crate::network::{DialResult, OutEvent};
use crate::peer_state::PeerState;
use crate::store::Store;

impl<S: Store> Bitswap<S> {
    /// Whether the swarm still holds a connection to `peer`.
    pub(super) fn is_connected(&self, peer: &PeerId) -> bool {
        self.connections
            .get(peer)
            .is_some_and(|connections| !connections.is_empty())
    }

    /// Hands every waiter on `peer` the outcome of a dial that will not produce a `ConnectionEstablished`
    /// of its own.
    pub(super) fn resolve_dials(&mut self, peer: &PeerId, outcome: DialResult) {
        for (id, sender) in self.dials.remove(peer).into_iter().flatten() {
            if sender.send(outcome.clone()).is_err() {
                debug!(id, "dial waiter is gone");
            }
        }
    }

    pub(super) fn on_dial_failure(&mut self, peer: PeerId, error: &DialError) {
        // A dial outcome says nothing about connections that already exist: tearing the peer down here
        // would leave bitswap's view disagreeing with the swarm's, and the refused dial that would have
        // repaired it is what landed here.
        let connected = self.is_connected(&peer);
        let condition_false = matches!(error, DialError::DialPeerConditionFalse { .. });
        if matches!(error, DialError::Denied { .. }) {
            self.pause_dialing = true;
            if !connected {
                self.set_peer_state(&peer, PeerState::Disconnected);
            }
        } else if !condition_false && !connected {
            self.set_peer_state(&peer, PeerState::DialFailure(Instant::now()));
        }

        trace!(%peer, ?error, "dial failure");
        if connected {
            let protocol = self.negotiated_protocol(&peer);
            self.resolve_dials(&peer, Ok(protocol));
        } else if condition_false {
            // A dial is already in flight: its own `ConnectionEstablished` resolves these waiters, and
            // failing them here would also fail the caller that started it. Every behaviour's refused dial
            // lands here because `FromSwarm` is broadcast.
            trace!(%peer, "dial already in flight, waiters left pending");
        } else {
            self.resolve_dials(&peer, Err(error.to_string()));
        }
    }

    pub(super) fn answer_dial(
        &mut self,
        peer: PeerId,
        response: oneshot::Sender<DialResult>,
        id: usize,
    ) -> Option<ToSwarm<BitswapEvent, BitswapHandlerIn>> {
        let reply = |response: oneshot::Sender<DialResult>, result: DialResult| {
            if response.send(result).is_err() {
                debug!(id, "dial response dropped");
            }
        };

        match self.peers.get(&peer).copied() {
            Some(PeerState::Responsive(_, protocol)) => reply(response, Ok(Some(protocol))),
            Some(PeerState::Connected(_)) => reply(response, Ok(None)),
            Some(PeerState::DialFailure(dialed)) if dialed.elapsed() < DIAL_BACK_OFF => {
                debug!(id, %peer, "peer is in dial back-off");
                reply(response, Err(format!("dial:{id}: undialable peer")));
            }
            _ if self.pause_dialing => {
                debug!(id, %peer, "dialing paused");
                reply(response, Err(format!("dial:{id}: dialing paused")));
            }
            _ => {
                self.dials.entry(peer).or_default().push((id, response));
                // Only dial a peer that is not already being talked to: an unconditional dial opened a
                // second connection to reachable peers, and a redundant dial that failed put the peer in
                // a ten-minute back-off.
                return Some(ToSwarm::Dial {
                    opts: DialOpts::peer_id(peer)
                        .condition(PeerCondition::DisconnectedAndNotDialing)
                        .build(),
                });
            }
        }
        None
    }

    pub(super) fn handle_out_event(
        &mut self,
        event: OutEvent,
    ) -> Option<ToSwarm<BitswapEvent, BitswapHandlerIn>> {
        match event {
            OutEvent::Dial { peer, response, id } => self.answer_dial(peer, response, id),
            OutEvent::SendMessage {
                peer,
                message,
                response,
            } => {
                debug!(%peer, "send message");
                Some(ToSwarm::NotifyHandler {
                    peer_id: peer,
                    handler: NotifyHandler::Any,
                    event: BitswapHandlerIn::Message(message, response),
                })
            }
            // Keep-alive is per connection and the recorded id can name a closed one, so the handler is
            // addressed through `Any` and only for peers known to speak bitswap.
            OutEvent::Protect { peer } => self.responsive(&peer).then(|| ToSwarm::NotifyHandler {
                peer_id: peer,
                handler: NotifyHandler::Any,
                event: BitswapHandlerIn::Protect,
            }),
            OutEvent::Unprotect { peer, response } => {
                let responsive = self.responsive(&peer);
                if response.send(responsive).is_err() {
                    debug!(%peer, "unprotect response dropped");
                }
                responsive.then(|| ToSwarm::NotifyHandler {
                    peer_id: peer,
                    handler: NotifyHandler::Any,
                    event: BitswapHandlerIn::Unprotect,
                })
            }
        }
    }

    pub(super) fn responsive(&self, peer: &PeerId) -> bool {
        matches!(self.peers.get(peer), Some(PeerState::Responsive(_, _)))
    }
}
