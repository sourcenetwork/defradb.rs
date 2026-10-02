//! The libp2p behaviour: tracks peers and connections, executes network requests, feeds the server and
//! drives the client.
//!
//! The behaviour owns every peer, connection, dial and fetch map directly (`&mut self`); the server runs in
//! its own tasks and is reached through channels.

mod client;
mod dial;

use std::task::{Context, Poll};
use std::time::Duration;

use libp2p::core::transport::PortUse;
use libp2p::core::Endpoint;
use libp2p::swarm::{
    ConnectionClosed, ConnectionDenied, ConnectionId, DialFailure, FromSwarm, NetworkBehaviour,
    THandler, THandlerInEvent, THandlerOutEvent, ToSwarm,
};
use libp2p::{Multiaddr, PeerId};
use rapidhash::RapidHashMap;
use tracing::{debug, trace};

use crate::client::{Client, Driver};
use crate::handler::{BitswapHandler, HandlerEvent};
use crate::message::BitswapMessage;
use crate::network::{Network, OutEvents};
use crate::peer_state::{Connections, Dials, PeerState};
use crate::protocol::{ProtocolConfig, ProtocolId};
use crate::server::{Server, ServerConfig};
use crate::store::Store;

const DIAL_BACK_OFF: Duration = Duration::from_secs(10 * 60);
const MAX_EVENTS_PER_POLL: usize = 50;

/// Behaviour configuration.
#[derive(Debug)]
pub struct Config {
    /// If no server config is set, the server is disabled.
    pub server: Option<ServerConfig>,
    /// Protocols spoken and the message size limit.
    pub protocol: ProtocolConfig,
    /// How long an idle connection is kept.
    pub idle_timeout: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            server: Some(ServerConfig::default()),
            protocol: ProtocolConfig::default(),
            idle_timeout: Duration::from_secs(30),
        }
    }
}

/// Events the behaviour reports to the swarm; none yet.
#[derive(Debug)]
pub enum BitswapEvent {}

/// The bitswap network behaviour.
#[derive(Debug)]
pub struct Bitswap<S: Store> {
    network: Network,
    out_events: OutEvents,
    protocol_config: ProtocolConfig,
    idle_timeout: Duration,
    peers: RapidHashMap<PeerId, PeerState>,
    /// Every live connection per peer, from the swarm's own established/closed events. `peers` records one
    /// connection and one protocol; this records reachability, which is what deciding whether to dial and
    /// whether a peer is still usable depends on.
    connections: Connections,
    dials: Dials,
    /// Set when dialing is disabled because the connection limit was reached.
    pause_dialing: bool,
    server: Option<Server>,
    client: Client,
    driver: Driver,
    _store: std::marker::PhantomData<S>,
}

impl<S: Store> Bitswap<S> {
    /// Builds the behaviour and spawns the server tasks on the current tokio runtime, which must exist.
    pub fn new(self_id: PeerId, store: S, config: Config) -> Self {
        let (network, out_events) = Network::new(self_id);
        let driver = Driver::new(network.clone());
        let server = config
            .server
            .map(|server_config| Server::new(network.clone(), store, server_config));

        Bitswap {
            network,
            out_events,
            protocol_config: config.protocol,
            idle_timeout: config.idle_timeout,
            peers: Default::default(),
            connections: Default::default(),
            dials: Default::default(),
            pause_dialing: false,
            server,
            client: Client::new(),
            driver,
            _store: std::marker::PhantomData,
        }
    }

    /// A handle for sending messages and dialing through this behaviour.
    pub fn network(&self) -> &Network {
        &self.network
    }

    /// Called on identify events from the swarm, informing us about the protocols of a peer.
    pub fn on_identify(&mut self, peer: &PeerId, protocols: &[String]) {
        if let Some(PeerState::Connected(connection)) = self.peers.get(peer).copied() {
            let best = protocols
                .iter()
                .filter_map(|s| ProtocolId::try_from_str(s))
                .max();
            if let Some(best) = best {
                self.set_peer_state(peer, PeerState::Responsive(connection, best));
            }
        }
    }

    fn peer_connected(&mut self, peer: PeerId) {
        self.client.on_responsive(&peer);
        if let Some(server) = &self.server {
            server.peer_connected(peer);
        }
    }

    fn peer_disconnected(&mut self, peer: PeerId) {
        self.client.on_peer_disconnected(&peer);
        if let Some(server) = &self.server {
            server.peer_disconnected(peer);
        }
    }

    fn on_message(&mut self, peer: &PeerId, message: &BitswapMessage) {
        self.client.on_message(peer, message);
    }

    fn receive_message(&mut self, peer: PeerId, message: BitswapMessage) {
        self.on_message(&peer, &message);
        if let Some(server) = &mut self.server {
            if server.try_receive_message(peer, message).is_err() {
                debug!(%peer, "server inbound queue full, dropping message");
            }
        }
    }

    /// Points a known peer's record at `connection` without changing whether it counts as connected or
    /// responsive.
    fn refresh_recorded_connection(&mut self, peer: &PeerId, connection: ConnectionId) {
        if let Some(state) = self.peers.get_mut(peer) {
            *state = match *state {
                PeerState::Responsive(_, protocol) => PeerState::Responsive(connection, protocol),
                _ => PeerState::Connected(connection),
            };
        }
    }

    /// The protocol negotiated with `peer`, if one is known.
    fn negotiated_protocol(&self, peer: &PeerId) -> Option<ProtocolId> {
        match self.peers.get(peer) {
            Some(PeerState::Responsive(_, protocol)) => Some(*protocol),
            _ => None,
        }
    }

    fn set_peer_state(&mut self, peer: &PeerId, new_state: PeerState) {
        let peer = *peer;
        let old_state = self.peers.get(&peer).copied();
        if old_state == Some(new_state) {
            return;
        }

        // Additional connections go through `refresh_recorded_connection` so they neither strand the
        // peer on the first id nor demote one already known to be responsive.
        if new_state == PeerState::Disconnected {
            self.peers.remove(&peer);
        } else {
            self.peers.insert(peer, new_state);
        }

        match new_state {
            PeerState::DialFailure(_) | PeerState::Disconnected => {
                if old_state.is_none_or(PeerState::is_connected) {
                    self.peer_disconnected(peer);
                }
            }
            PeerState::Connected(_) => {}
            PeerState::Responsive(_, _) => self.peer_connected(peer),
        }
    }

    fn new_handler(&self) -> BitswapHandler {
        BitswapHandler::new(self.protocol_config.clone(), self.idle_timeout)
    }

    fn on_connection_established(
        &mut self,
        peer: PeerId,
        connection: ConnectionId,
        other_established: usize,
    ) {
        trace!(%peer, other_established, "connection established");
        self.connections.entry(peer).or_default().insert(connection);
        if other_established == 0 {
            self.set_peer_state(&peer, PeerState::Connected(connection));
        } else {
            // An additional connection refreshes the recorded id without re-announcing a peer that is
            // already counted as connected.
            self.refresh_recorded_connection(&peer, connection);
        }
        self.pause_dialing = false;

        // A dial is satisfied the moment the peer is reachable, not once a bitswap substream negotiated.
        let protocol = self.negotiated_protocol(&peer);
        self.resolve_dials(&peer, Ok(protocol));
    }

    fn on_connection_closed(
        &mut self,
        peer: PeerId,
        connection: ConnectionId,
        remaining_established: usize,
    ) {
        self.pause_dialing = false;
        if let Some(connections) = self.connections.get_mut(&peer) {
            connections.remove(&connection);
            if connections.is_empty() {
                self.connections.remove(&peer);
            }
        }
        // While other connections remain the peer stays connected. The recorded id may now name a closed
        // connection, which is why messages are dispatched to any live connection rather than to that id.
        if remaining_established == 0 {
            self.set_peer_state(&peer, PeerState::Disconnected);
        }
    }
}

impl<S: Store> NetworkBehaviour for Bitswap<S> {
    type ConnectionHandler = BitswapHandler;
    type ToSwarm = BitswapEvent;

    fn handle_established_inbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _local_addr: &Multiaddr,
        _remote_addr: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(self.new_handler())
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _addr: &Multiaddr,
        _role_override: Endpoint,
        _port_use: PortUse,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(self.new_handler())
    }

    fn on_swarm_event(&mut self, event: FromSwarm) {
        match event {
            FromSwarm::ConnectionEstablished(info) => {
                self.on_connection_established(
                    info.peer_id,
                    info.connection_id,
                    info.other_established,
                );
            }
            FromSwarm::ConnectionClosed(ConnectionClosed {
                peer_id,
                connection_id,
                remaining_established,
                ..
            }) => self.on_connection_closed(peer_id, connection_id, remaining_established),
            FromSwarm::DialFailure(DialFailure {
                peer_id: Some(peer_id),
                error,
                ..
            }) => self.on_dial_failure(peer_id, error),
            _ => {}
        }
    }

    fn on_connection_handler_event(
        &mut self,
        peer_id: PeerId,
        connection: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        match event {
            HandlerEvent::Message {
                mut message,
                protocol,
            } => {
                self.set_peer_state(&peer_id, PeerState::Responsive(connection, protocol));
                message.verify_blocks();
                self.receive_message(peer_id, message);
            }
            HandlerEvent::FailedToSendMessage { error } => {
                debug!(%peer_id, %error, "substream failed");
            }
        }
    }

    fn poll(&mut self, cx: &mut Context) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        self.poll_client(cx);
        for _ in 0..MAX_EVENTS_PER_POLL {
            match self.out_events.poll_next(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(event) => {
                    if let Some(action) = self.handle_out_event(event) {
                        return Poll::Ready(action);
                    }
                }
            }
        }

        // The budget ran out with events possibly left, so ask to be polled again.
        cx.waker().wake_by_ref();
        Poll::Pending
    }
}
