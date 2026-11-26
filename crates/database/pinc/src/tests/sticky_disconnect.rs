//! A `DisconnectPeers` sticks on both sides: neither the node that hung up
//! nor the peer it hung up on dials the other back through peer exchange,
//! and an explicit dial still reaches it (#2992).

use std::time::{Duration, Instant};

use super::{connect, run, PincTransport, TestNode, IO_TIMEOUT};
use p2p::transport::{P2PTransport, PeerId};

/// How long peer exchange gets to dial back, were it going to.
const EXCHANGE_WINDOW: Duration = Duration::from_secs(1);

async fn wait_gone(transport: &PincTransport, peer: &PeerId) {
    let deadline = Instant::now() + IO_TIMEOUT;
    while transport.connected_peers().await.unwrap().contains(peer) {
        assert!(Instant::now() < deadline, "the peer is still connected");
        p2p::rt::time::sleep(Duration::from_millis(20)).await;
    }
}

/// b and c meet through the hub, then b hangs up on c. Then b (when
/// `b_refreshes`) or c reconnects to the hub, which hands only it a fresh
/// record of the other, so any dial that follows is that node's peer
/// exchange.
async fn hang_up_then_refresh(b_refreshes: bool) {
    let hub = TestNode::spawn().await;
    let b = TestNode::spawn().await;
    let c = TestNode::spawn().await;
    connect(&b.transport, &hub.transport).await;
    connect(&c.transport, &hub.transport).await;
    b.transport
        .poll_until_connected(&c.id, IO_TIMEOUT)
        .await
        .unwrap();

    b.transport.disconnect(&c.id).await.unwrap();
    wait_gone(&b.transport, &c.id).await;
    wait_gone(&c.transport, &b.id).await;

    let (node, who) = if b_refreshes {
        (&b.transport, "the node that hung up")
    } else {
        (&c.transport, "the peer hung up on")
    };
    node.disconnect(&hub.id).await.unwrap();
    wait_gone(node, &hub.id).await;
    connect(node, &hub.transport).await;
    p2p::rt::time::sleep(EXCHANGE_WINDOW).await;
    assert!(
        !b.transport.connected_peers().await.unwrap().contains(&c.id),
        "{who} dialed the other back through peer exchange"
    );

    let c_addrs = c.transport.listen_addresses().await.unwrap();
    b.transport.dial(&c.id, c_addrs).await.unwrap();
    c.transport
        .poll_until_connected(&b.id, IO_TIMEOUT)
        .await
        .unwrap();

    hub.shutdown().await;
    b.shutdown().await;
    c.shutdown().await;
}

#[test]
fn the_node_that_hung_up_does_not_dial_the_peer_back() {
    run(hang_up_then_refresh(true));
}

#[test]
fn the_peer_hung_up_on_does_not_dial_the_node_back() {
    run(hang_up_then_refresh(false));
}

/// b hangs up on c before it ever linked to it, then meets the hub, which
/// hands it a record of c: b's peer exchange leaves c alone all the same.
#[test]
fn a_hang_up_with_no_link_keeps_peer_exchange_away() {
    run(async {
        let hub = TestNode::spawn().await;
        let b = TestNode::spawn().await;
        let c = TestNode::spawn().await;
        connect(&c.transport, &hub.transport).await;

        b.transport.disconnect(&c.id).await.unwrap();
        connect(&b.transport, &hub.transport).await;
        p2p::rt::time::sleep(EXCHANGE_WINDOW).await;
        assert!(
            !b.transport.connected_peers().await.unwrap().contains(&c.id),
            "the node dialed a peer it hung up on with no link"
        );

        hub.shutdown().await;
        b.shutdown().await;
        c.shutdown().await;
    });
}
