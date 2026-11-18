//! Connecting peers: replicators, subscriptions and parting.

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use super::peer::Peer;
use super::wait::pause;

impl Peer {
    /// The address other peers dial this one at: an iroh ticket that names
    /// a direct address, as relay and discovery are off.
    pub async fn p2p_addr(&self) -> String {
        let ops = self.node.p2p_system().expect("p2p").ops();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(addr) = ops.shareable_address().await.expect("shareable address") {
                return addr;
            }
            assert!(Instant::now() < deadline, "no dialable iroh address");
            pause().await;
        }
    }

    /// The UDP port this peer's iroh endpoint is bound to.
    pub async fn bound_port(&self) -> u16 {
        let addr = self.p2p_addr().await;
        let (_, hints) = p2p::iroh::parse_public_peer_addr(&addr).expect("iroh address");
        hints
            .iter()
            .find_map(|hint| hint.as_str().parse::<SocketAddr>().ok())
            .unwrap_or_else(|| panic!("no direct address in {addr}"))
            .port()
    }

    /// Subscribe `to` to `Users` and have this peer push `Users` to it.
    pub async fn replicate_to(&self, to: &Peer) {
        to.node
            .p2p_system()
            .expect("p2p")
            .ops()
            .add_collections(vec!["Users".into()])
            .await
            .expect("subscription");
        let addr = to.p2p_addr().await;
        self.node
            .p2p_system()
            .expect("p2p")
            .ops()
            .add_replicator(
                vec!["Users".into()],
                Some(&addr),
                Default::default(),
                Vec::new(),
                None,
            )
            .await
            .expect("replicator");
    }

    /// Stop pushing `Users` to `other`, take `other` off the `Users` topic,
    /// and drop the connections between the two, so nothing `other` writes
    /// reaches a peer until it pushes to one.
    pub async fn part_from(&self, other: &Peer) {
        let ops = self.node.p2p_system().expect("p2p").ops();
        let other_ops = other.node.p2p_system().expect("p2p").ops();
        let (addr, other_addr) = (self.p2p_addr().await, other.p2p_addr().await);
        ops.remove_replicator(vec!["Users".into()], Some(&other_addr))
            .await
            .expect("replicator removed");
        other_ops
            .remove_collections(vec!["Users".into()])
            .await
            .expect("unsubscribed");
        ops.disconnect_peer(&other_addr)
            .await
            .expect("disconnected");
        other_ops
            .disconnect_peer(&addr)
            .await
            .expect("disconnected");
    }
}
