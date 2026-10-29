//! The swarm's transports, assembled as libp2p's `SwarmBuilder` assembles
//! them (relay, then WebSocket, then TCP and QUIC under DNS, each
//! authenticated with Noise and multiplexed with Yamux, all within the
//! connection timeout).
//!
//! TCP, and the TCP under WebSocket, are nuclei sockets
//! (`net::libp2p::TcpTransport`). libp2p's QUIC and DNS exist only on tokio,
//! so the swarm and its connections run inside `edge::compat` ([`Executor`]).

use std::future::Future;
use std::pin::Pin;

use libp2p::core::muxing::StreamMuxerBox;
use libp2p::core::transport::timeout::TransportTimeout;
use libp2p::core::transport::{Boxed, Transport as _};
use libp2p::core::upgrade::Version;
use libp2p::identity::Keypair;
use libp2p::{noise, relay, yamux, PeerId};
use net::libp2p::{authenticated_tcp, TcpTransport, CONNECTION_TIMEOUT};

use crate::error::{Error, Result};

/// The swarm's executor: each connection's task runs on the network edge,
/// in the tokio context QUIC and DNS need, until the swarm closes it or the
/// edge stops accepting.
pub(crate) struct Executor {
    edge: edge::Edge,
}

impl Executor {
    pub(crate) fn new(edge: edge::Edge) -> Self {
        Self { edge }
    }
}

impl libp2p::swarm::Executor for Executor {
    fn exec(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>) {
        self.edge.spawn_service(edge::compat(future)).detach();
    }
}

/// Every transport of the swarm, as one.
pub(crate) type SwarmTransport = Boxed<(PeerId, StreamMuxerBox)>;

/// The transport, and the relay client's behaviour when `relay` is on.
pub(crate) fn build(
    keypair: &Keypair,
    relay: bool,
) -> Result<(SwarmTransport, Option<relay::client::Behaviour>)> {
    let noise = || noise::Config::new(keypair).map_err(|e| Error::Transport(e.to_string()));
    let dns_error = |e: std::io::Error| Error::Transport(e.to_string());

    let tcp = authenticated_tcp(keypair).map_err(|e| Error::Transport(e.to_string()))?;
    let quic = libp2p::quic::tokio::Transport::new(libp2p::quic::Config::new(keypair))
        .map(|(peer, muxer), _| (peer, StreamMuxerBox::new(muxer)));
    let direct = libp2p::dns::tokio::Transport::system(
        tcp.or_transport(quic).map(|either, _| either.into_inner()),
    )
    .map_err(dns_error)?;
    let websocket = libp2p::websocket::Config::new(
        libp2p::dns::tokio::Transport::system(TcpTransport::default()).map_err(dns_error)?,
    )
    .upgrade(Version::V1Lazy)
    .authenticate(noise()?)
    .multiplex(yamux::Config::default())
    .map(|(peer, muxer), _| (peer, StreamMuxerBox::new(muxer)));
    let transport = websocket
        .or_transport(direct)
        .map(|either, _| either.into_inner());

    if !relay {
        return Ok((
            TransportTimeout::new(transport, CONNECTION_TIMEOUT).boxed(),
            None,
        ));
    }
    let (relayed, behaviour) = relay::client::new(keypair.public().to_peer_id());
    let relayed = relayed
        .upgrade(Version::V1Lazy)
        .authenticate(noise()?)
        .multiplex(yamux::Config::default())
        .map(|(peer, muxer), _| (peer, StreamMuxerBox::new(muxer)));
    let transport = relayed
        .or_transport(transport)
        .map(|either, _| either.into_inner());
    Ok((
        TransportTimeout::new(transport, CONNECTION_TIMEOUT).boxed(),
        Some(behaviour),
    ))
}
