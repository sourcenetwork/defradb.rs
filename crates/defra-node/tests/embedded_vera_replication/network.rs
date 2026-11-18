//! Connecting peers: replicators, subscriptions and parting.

use std::time::{Duration, Instant};

use super::peer::Peer;
use super::wait::pause;

impl Peer {
    pub async fn p2p_addr(&self) -> String {
        let ops = self.node.p2p_system().expect("p2p").ops();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let addrs = ops.listen_addresses().await.expect("listen addresses");
            if let Some(addr) = addrs
                .into_iter()
                .find(|addr| addr.contains("/p2p/") || addr.starts_with("endpoint"))
            {
                return addr;
            }
            assert!(Instant::now() < deadline, "no iroh listen address");
            pause().await;
        }
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
