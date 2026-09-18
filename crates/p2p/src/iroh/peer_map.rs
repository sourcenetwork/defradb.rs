//! Mapping between iroh `EndpointId` and transport `PeerId`, plus connection tracking.

use rapidhash::RapidHashMap;
use std::net::SocketAddr;

use iroh::endpoint::Connection;
use iroh::EndpointId;
use kovan::Atom;

use crate::transport::PeerId;

/// Parse a transport `PeerId` string into an iroh `EndpointId`.
pub fn parse_endpoint_id(peer_id: &PeerId) -> crate::error::Result<EndpointId> {
    peer_id
        .as_str()
        .parse::<EndpointId>()
        .map_err(|e| crate::error::Error::InvalidPeerId(e.to_string()))
}

/// Convert an iroh `EndpointId` into a transport `PeerId`.
pub fn endpoint_id_to_peer_id(id: &EndpointId) -> PeerId {
    PeerId::new(id.to_string())
}

/// Connection info for a connected peer.
#[derive(Debug, Clone)]
pub struct ConnectionInfo {
    pub remote_addr: Option<SocketAddr>,
    pub active_connections: u32,
    /// Handles to every live QUIC connection, used to hang up on
    /// `disconnect`. A peer can hold several concurrent connections (one per
    /// ALPN, dial + accept); all must be closed or the count never reaches
    /// zero and `PeerDisconnected` never fires. iroh `Connection` clones
    /// share the underlying QUIC connection, so closing a handle closes that
    /// connection.
    handles: Vec<Connection>,
}

/// Tracks connected peers and their connection info.
#[derive(Debug, Default, Clone)]
pub struct PeerMap {
    connections: RapidHashMap<EndpointId, ConnectionInfo>,
}

impl PeerMap {
    /// Increment connection count for a peer. Returns `true` if this is the
    /// first connection (0 -> 1), meaning PeerConnected should be emitted.
    ///
    /// `connection` is a handle to the underlying QUIC connection, retained so
    /// `disconnect` can hang up on the peer.
    pub fn increment_connections(
        &mut self,
        id: EndpointId,
        remote_addr: Option<SocketAddr>,
        connection: Connection,
    ) -> bool {
        if let Some(info) = self.connections.get_mut(&id) {
            info.active_connections += 1;
            if let Some(addr) = remote_addr {
                info.remote_addr = Some(addr);
            }
            // Already-closed handles would otherwise accumulate for a
            // long-lived peer whose connections churn.
            info.handles.retain(|conn| conn.close_reason().is_none());
            info.handles.push(connection);
            false
        } else {
            self.connections.insert(
                id,
                ConnectionInfo {
                    remote_addr,
                    active_connections: 1,
                    handles: vec![connection],
                },
            );
            true
        }
    }

    /// Take every retained connection handle for a peer without removing the
    /// connection-count entry. Returns an empty vec if the peer is unknown or
    /// no handles were retained. Used by `disconnect` to close the live
    /// connections; the count entry is then cleared by the stream tasks on
    /// `accept_bi` error.
    pub fn take_connections(&mut self, id: &EndpointId) -> Vec<Connection> {
        self.connections
            .get_mut(id)
            .map(|info| std::mem::take(&mut info.handles))
            .unwrap_or_default()
    }

    /// Decrement connection count for a peer. Returns `true` if the count
    /// reached zero (fully disconnected), meaning PeerDisconnected should be emitted.
    pub fn decrement_connections(&mut self, id: &EndpointId) -> bool {
        if let Some(info) = self.connections.get_mut(id) {
            info.active_connections = info.active_connections.saturating_sub(1);
            if info.active_connections == 0 {
                self.connections.remove(id);
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    pub fn remote_addr(&self, id: &EndpointId) -> Option<SocketAddr> {
        self.connections.get(id).and_then(|info| info.remote_addr)
    }

    pub fn connected_peers(&self) -> Vec<PeerId> {
        self.connections
            .keys()
            .map(endpoint_id_to_peer_id)
            .collect()
    }

    pub fn peer_addresses(&self) -> Vec<String> {
        self.connections
            .values()
            .filter_map(|info| info.remote_addr.map(|a: SocketAddr| a.to_string()))
            .collect()
    }

    /// Return all connected endpoint IDs.
    pub fn endpoint_ids(&self) -> Vec<EndpointId> {
        self.connections.keys().copied().collect()
    }
}

/// The peer map shared by every endpoint task, replaced as one unit on each
/// write: a count reaching zero removes the entry and a first count emits
/// `PeerConnected`, so the count and the entry's presence must change together.
#[derive(Debug, Default)]
pub struct SharedPeerMap(Atom<PeerMap>);

impl SharedPeerMap {
    pub fn new() -> Self {
        Self::default()
    }

    fn update<R>(&self, mut f: impl FnMut(&mut PeerMap) -> R) -> R {
        loop {
            let current = self.0.load();
            let mut next = PeerMap::clone(&current);
            let result = f(&mut next);
            if self.0.compare_and_swap(&current, next).is_ok() {
                return result;
            }
        }
    }

    pub fn increment_connections(
        &self,
        id: EndpointId,
        remote_addr: Option<SocketAddr>,
        connection: Connection,
    ) -> bool {
        self.update(|map| map.increment_connections(id, remote_addr, connection.clone()))
    }

    pub fn take_connections(&self, id: &EndpointId) -> Vec<Connection> {
        self.update(|map| map.take_connections(id))
    }

    pub fn decrement_connections(&self, id: &EndpointId) -> bool {
        self.update(|map| map.decrement_connections(id))
    }

    pub fn remote_addr(&self, id: &EndpointId) -> Option<SocketAddr> {
        self.0.peek(|map| map.remote_addr(id))
    }

    pub fn connected_peers(&self) -> Vec<PeerId> {
        self.0.peek(PeerMap::connected_peers)
    }

    pub fn peer_addresses(&self) -> Vec<String> {
        self.0.peek(PeerMap::peer_addresses)
    }

    pub fn endpoint_ids(&self) -> Vec<EndpointId> {
        self.0.peek(PeerMap::endpoint_ids)
    }
}
