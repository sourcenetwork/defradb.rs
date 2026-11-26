//! Reject connections to peers explicitly disconnected by this node.

use std::convert::Infallible;
use std::task::{Context, Poll};

use libp2p::{
    core::{transport::PortUse, Endpoint},
    swarm::{
        dummy, ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour, THandler,
        THandlerInEvent, THandlerOutEvent, ToSwarm,
    },
    Multiaddr, PeerId,
};
use rapidhash::RapidHashSet;

/// Network behaviour that denies connections to explicitly disconnected peers.
#[derive(Default)]
pub struct DisconnectGate {
    peers: RapidHashSet<PeerId>,
}

impl DisconnectGate {
    pub(crate) fn block(&mut self, peer: PeerId) {
        self.peers.insert(peer);
    }

    pub(crate) fn unblock(&mut self, peer: &PeerId) {
        self.peers.remove(peer);
    }

    pub(crate) fn contains(&self, peer: &PeerId) -> bool {
        self.peers.contains(peer)
    }

    pub(crate) fn peers(&self) -> &RapidHashSet<PeerId> {
        &self.peers
    }

    fn check(&self, peer: &PeerId) -> Result<(), ConnectionDenied> {
        if self.contains(peer) {
            return Err(ConnectionDenied::new(std::io::Error::other(
                "peer explicitly disconnected",
            )));
        }
        Ok(())
    }
}

impl NetworkBehaviour for DisconnectGate {
    type ConnectionHandler = dummy::ConnectionHandler;
    type ToSwarm = Infallible;

    fn handle_established_inbound_connection(
        &mut self,
        _: ConnectionId,
        peer: PeerId,
        _: &Multiaddr,
        _: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        self.check(&peer)?;
        Ok(dummy::ConnectionHandler)
    }

    fn handle_pending_outbound_connection(
        &mut self,
        _: ConnectionId,
        peer: Option<PeerId>,
        _: &[Multiaddr],
        _: Endpoint,
    ) -> Result<Vec<Multiaddr>, ConnectionDenied> {
        if let Some(peer) = peer {
            self.check(&peer)?;
        }
        Ok(Vec::new())
    }

    fn handle_established_outbound_connection(
        &mut self,
        _: ConnectionId,
        peer: PeerId,
        _: &Multiaddr,
        _: Endpoint,
        _: PortUse,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        self.check(&peer)?;
        Ok(dummy::ConnectionHandler)
    }

    fn on_swarm_event(&mut self, _: FromSwarm) {}

    fn on_connection_handler_event(
        &mut self,
        _: PeerId,
        _: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        libp2p::core::util::unreachable(event)
    }

    fn poll(&mut self, _: &mut Context<'_>) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disconnected_peer_is_denied_until_unblocked() {
        let peer = PeerId::random();
        let other = PeerId::random();
        let id = ConnectionId::new_unchecked(1);
        let address: Multiaddr = "/ip4/127.0.0.1/tcp/1234".parse().unwrap();
        let mut gate = DisconnectGate::default();
        gate.block(peer);

        assert!(gate
            .handle_pending_outbound_connection(id, Some(peer), &[], Endpoint::Dialer)
            .is_err());
        assert!(gate
            .handle_established_inbound_connection(id, peer, &address, &address)
            .is_err());
        assert!(gate
            .handle_established_outbound_connection(
                id,
                peer,
                &address,
                Endpoint::Dialer,
                PortUse::Reuse
            )
            .is_err());
        assert!(gate
            .handle_pending_outbound_connection(id, Some(other), &[], Endpoint::Dialer)
            .is_ok());

        gate.unblock(&peer);
        assert!(gate
            .handle_pending_outbound_connection(id, Some(peer), &[], Endpoint::Dialer)
            .is_ok());
        assert!(gate
            .handle_established_inbound_connection(id, peer, &address, &address)
            .is_ok());
    }
}
