//! Peer exchange: peers of a peer connect to each other.

use super::{connect, hub_with, loopback, run, seed, TestNode, IO_TIMEOUT};
use crate::PincEndpointConfig;
use p2p::transport::P2PTransport;

#[test]
fn two_peers_of_one_node_connect_to_each_other() {
    run(async {
        let hub = TestNode::spawn().await;
        let b = TestNode::spawn().await;
        let c = TestNode::spawn().await;
        connect(&b.transport, &hub.transport).await;
        connect(&c.transport, &hub.transport).await;
        c.transport
            .poll_until_connected(&b.id, IO_TIMEOUT)
            .await
            .unwrap();
        b.transport
            .poll_until_connected(&c.id, IO_TIMEOUT)
            .await
            .unwrap();
        hub.shutdown().await;
        b.shutdown().await;
        c.shutdown().await;
    });
}

/// Nodes that name a pinc hub sign their records only once pinc settles them
/// on it, seconds after they connect: peers of a peer meet before that.
#[test]
fn two_peers_of_one_node_connect_before_their_records_are_signed() {
    run(async {
        let ticket = hub_with(false).await;
        let on_hub = || {
            let mut config = PincEndpointConfig::new(seed(), loopback());
            config.max_connections = 8;
            config.hubs = vec![ticket.clone()];
            TestNode::with_config(config)
        };
        let middle = on_hub().await;
        let b = on_hub().await;
        let c = on_hub().await;
        connect(&b.transport, &middle.transport).await;
        connect(&c.transport, &middle.transport).await;
        c.transport
            .poll_until_connected(&b.id, IO_TIMEOUT)
            .await
            .unwrap();
        b.transport
            .poll_until_connected(&c.id, IO_TIMEOUT)
            .await
            .unwrap();
        middle.shutdown().await;
        b.shutdown().await;
        c.shutdown().await;
    });
}
