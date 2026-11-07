//! libp2p on nuclei: a TCP transport on nuclei sockets ([`TcpTransport`]),
//! the same transport authenticated with Noise and multiplexed with Yamux,
//! an executor that runs a swarm's connection tasks on the network edge,
//! and a swarm of all three.

mod interfaces;
mod tcp;

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use edge::Edge;
use libp2p::core::muxing::StreamMuxerBox;
use libp2p::core::transport::timeout::TransportTimeout;
use libp2p::core::transport::{Boxed, Transport as _};
use libp2p::core::upgrade::Version;
use libp2p::identity::Keypair;
use libp2p::swarm::NetworkBehaviour;
use libp2p::{noise, yamux, PeerId, Swarm};

pub use tcp::Transport as TcpTransport;

/// libp2p's default: a connection is established within this time, or the
/// dial fails.
pub const CONNECTION_TIMEOUT: Duration = Duration::from_secs(10);

/// A swarm's connection tasks, run on the network edge it was given until
/// the swarm closes them or the edge stops accepting.
pub struct Executor {
    edge: Edge,
}

impl Executor {
    pub fn new(edge: Edge) -> Self {
        Self { edge }
    }
}

impl libp2p::swarm::Executor for Executor {
    fn exec(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>) {
        self.edge.spawn_service(future).detach();
    }
}

/// TCP on nuclei sockets, authenticated with Noise and multiplexed with
/// Yamux, as libp2p's `SwarmBuilder::with_tcp` assembles it.
pub fn authenticated_tcp(
    keypair: &Keypair,
) -> Result<Boxed<(PeerId, StreamMuxerBox)>, noise::Error> {
    Ok(TcpTransport::default()
        .upgrade(Version::V1Lazy)
        .authenticate(noise::Config::new(keypair)?)
        .multiplex(yamux::Config::default())
        .map(|(peer, muxer), _| (peer, StreamMuxerBox::new(muxer)))
        .boxed())
}

/// A swarm of `behaviour` on [`authenticated_tcp`] whose connections run on
/// `edge`, closing a connection idle for `idle`.
pub fn swarm<B>(
    keypair: &Keypair,
    behaviour: B,
    idle: Duration,
    edge: Edge,
) -> Result<Swarm<B>, noise::Error>
where
    B: NetworkBehaviour,
{
    let transport = TransportTimeout::new(authenticated_tcp(keypair)?, CONNECTION_TIMEOUT).boxed();
    Ok(Swarm::new(
        transport,
        behaviour,
        keypair.public().to_peer_id(),
        libp2p::swarm::Config::with_executor(Executor::new(edge))
            .with_idle_connection_timeout(idle),
    ))
}
