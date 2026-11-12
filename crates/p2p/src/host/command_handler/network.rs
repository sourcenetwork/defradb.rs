//! Network commands: Listen, Dial, PeerAddresses.

use rapidhash::RapidHashSet;

use ::bitswap::Store;
use libp2p::{Multiaddr, PeerId};
use tracing::debug;

use crate::error::{Error, Result};

use super::super::p2p_host::P2PHost;

/// libp2p's `TransportError::Other` Displays as an empty string — the io
/// error is only reachable through `source()` — so `to_string()` on a failed
/// listen erased the "Address already in use" detail that the test harness
/// keys its fresh-port start retry on (#1501). Surface the io error directly.
/// `addr` as a full multiaddr of peer `pid`. A dialer stores the address it
/// dialed, which already ends in `/p2p/<pid>`; identify later replaces it with
/// a bare listen address. Either way the peer ID appears once.
fn peer_address(addr: &Multiaddr, pid: &PeerId) -> String {
    match addr.iter().last() {
        Some(libp2p::multiaddr::Protocol::P2p(_)) => addr.to_string(),
        _ => format!("{addr}/p2p/{pid}"),
    }
}

fn format_listen_error(error: &libp2p::TransportError<std::io::Error>) -> String {
    match error {
        libp2p::TransportError::MultiaddrNotSupported(addr) => {
            format!("multiaddr not supported: {addr}")
        }
        libp2p::TransportError::Other(io_error) => io_error.to_string(),
    }
}

impl<S: Store> P2PHost<S> {
    pub(super) fn handle_listen(
        &mut self,
        addr: Multiaddr,
        response: crate::rt::oneshot::Sender<Result<()>>,
    ) {
        let result = self
            .swarm
            .listen_on(addr.clone())
            .map(|_| ())
            .map_err(|e| Error::Transport(format_listen_error(&e)));
        if response.send(result).is_err() {
            debug!(addr = %addr, "Listen command response dropped - caller cancelled");
        }
    }

    pub(super) fn handle_dial(
        &mut self,
        peer_id: PeerId,
        addrs: Vec<Multiaddr>,
        response: crate::rt::oneshot::Sender<Result<()>>,
    ) {
        let result = self.dial_peer(peer_id, addrs);
        if response.send(result).is_err() {
            debug!(peer_id = %peer_id, "Dial command response dropped - caller cancelled");
        }
    }

    pub(super) fn handle_disconnect(
        &mut self,
        peer_id: PeerId,
        response: crate::rt::oneshot::Sender<Result<()>>,
    ) {
        self.peer_addrs.remove(&peer_id);
        self.swarm.behaviour_mut().kademlia.remove_peer(&peer_id);
        // `disconnect_peer_id` returns `Err(())` when the peer is not connected.
        // Disconnect is idempotent: hanging up on an absent peer is success.
        if self.swarm.disconnect_peer_id(peer_id).is_ok() {
            self.pending_disconnects
                .entry(peer_id)
                .or_default()
                .push(response);
        } else if response.send(Ok(())).is_err() {
            debug!(peer_id = %peer_id, "Disconnect command response dropped - caller cancelled");
        }
    }

    pub(super) fn handle_peer_connection(
        &self,
        peer_id: PeerId,
        response: crate::rt::oneshot::Sender<Option<crate::transport::PeerConnectionInfo>>,
    ) {
        let open = self.connection_manager.peer_connections(&peer_id);
        let info =
            open.first().map(
                |&(path, dialed_by_us, _)| crate::transport::PeerConnectionInfo {
                    path,
                    remote_address: self.peer_addrs.get(&peer_id).map(|addr| addr.to_string()),
                    open_connections: open.len() as u32,
                    rtt_micros: None,
                    bytes_sent: None,
                    bytes_received: None,
                    dialed_by_us: Some(dialed_by_us),
                    connected_for_secs: open.iter().map(|&(_, _, at)| at.elapsed().as_secs()).max(),
                },
            );
        if response.send(info).is_err() {
            debug!("PeerConnection command response dropped - caller cancelled");
        }
    }

    /// Answer `ActivePeers` once every connected peer that dialed us has
    /// identified. Until then its address is the connection's source port,
    /// not its listen address (#2774); the reply waits for the peer's Identify,
    /// whose failure or timeout, or the peer's disconnect, also ends the wait.
    pub(super) fn handle_peer_addresses(
        &mut self,
        response: crate::rt::oneshot::Sender<Vec<String>>,
    ) {
        self.pending_peer_addresses.push(response);
        self.answer_peer_addresses();
    }

    /// The peer's Identify arrived or failed: release any `ActivePeers` reply
    /// it held.
    pub(in crate::host) fn on_identify_settled(&mut self, peer_id: PeerId) {
        self.unidentified.remove(&peer_id);
        self.answer_peer_addresses();
    }

    /// Send the held `ActivePeers` replies if no connected peer still awaits
    /// its Identify.
    pub(in crate::host) fn answer_peer_addresses(&mut self) {
        if self.pending_peer_addresses.is_empty()
            || self
                .unidentified
                .iter()
                .any(|peer| self.swarm.is_connected(peer))
        {
            return;
        }
        // Build full multiaddrs for connected peers (matches Go's ActivePeers).
        let connected: RapidHashSet<PeerId> = self.swarm.connected_peers().cloned().collect();
        let addrs: Vec<String> = connected
            .iter()
            .filter_map(|pid| self.peer_addrs.get(pid).map(|addr| peer_address(addr, pid)))
            .collect();
        for response in self.pending_peer_addresses.drain(..) {
            if response.send(addrs.clone()).is_err() {
                debug!("PeerAddresses command response dropped - caller cancelled");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{format_listen_error, peer_address};

    #[test]
    fn a_peer_address_names_its_peer_once() {
        let pid = libp2p::PeerId::random();
        let bare: libp2p::Multiaddr = "/ip4/127.0.0.1/tcp/4001".parse().unwrap();
        let dialed: libp2p::Multiaddr = format!("/ip4/127.0.0.1/tcp/4001/p2p/{pid}")
            .parse()
            .unwrap();
        let want = format!("/ip4/127.0.0.1/tcp/4001/p2p/{pid}");
        assert_eq!(peer_address(&bare, &pid), want);
        assert_eq!(peer_address(&dialed, &pid), want);
    }

    #[test]
    fn listen_error_surfaces_the_bind_failure() {
        let eaddrinuse = if cfg!(target_os = "linux") { 98 } else { 48 };
        let error = libp2p::TransportError::Other(std::io::Error::from_raw_os_error(eaddrinuse));

        let formatted = format_listen_error(&error);

        // The harness retries a node start with fresh ports only when it can
        // see this phrase in the node's output (#1501).
        assert!(
            formatted.to_lowercase().contains("address already in use"),
            "expected the bind failure detail, got: {formatted:?}"
        );
    }

    #[test]
    fn listen_error_names_the_unsupported_multiaddr() {
        let addr: libp2p::Multiaddr = "/ip4/127.0.0.1/tcp/9171".parse().unwrap();
        let error = libp2p::TransportError::<std::io::Error>::MultiaddrNotSupported(addr);

        assert_eq!(
            format_listen_error(&error),
            "multiaddr not supported: /ip4/127.0.0.1/tcp/9171"
        );
    }
}
