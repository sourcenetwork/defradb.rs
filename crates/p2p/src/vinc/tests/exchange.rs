//! Peer exchange: peers of a peer connect to each other.

use super::{connect, run, TestNode, IO_TIMEOUT};
use crate::transport::P2PTransport;

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
