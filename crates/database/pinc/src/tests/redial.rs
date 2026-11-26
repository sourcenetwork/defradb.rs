//! A dial to a peer that stopped fails and leaves the peer out of the
//! connected set, although pinc hands a dial over before the peer answers
//! when its access was allowed lately.

use std::time::{Duration, Instant};

use super::{connect, run, PincTransport, TestNode, IO_TIMEOUT};
use p2p::transport::{P2PTransport, PeerId};

/// Wait, with a bounded backoff, until `transport` no longer counts `peer` as
/// connected.
async fn wait_gone(transport: &PincTransport, peer: &PeerId) {
    let deadline = Instant::now() + IO_TIMEOUT;
    let mut pause = Duration::from_millis(10);
    loop {
        let peers = transport.connected_peers().await.unwrap();
        if !peers.contains(peer) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the stopped peer is still connected"
        );
        p2p::rt::time::sleep(pause).await;
        pause = (pause * 2).min(Duration::from_millis(200));
    }
}

#[test]
fn a_dial_to_a_peer_that_stopped_fails_and_it_stays_disconnected() {
    run(async {
        let a = TestNode::spawn().await;
        let b = TestNode::spawn().await;
        connect(&a.transport, &b.transport).await;
        let b_id = b.id.clone();
        let b_addrs = b.transport.listen_addresses().await.unwrap();
        b.shutdown().await;
        wait_gone(&a.transport, &b_id).await;
        // The allow for b is cached, so pinc hands this dial over at once.
        let dialed = a.transport.dial(&b_id, b_addrs).await;
        assert!(dialed.is_err(), "a dial to a stopped peer succeeded");
        let peers = a.transport.connected_peers().await.unwrap();
        assert!(!peers.contains(&b_id), "a stopped peer counts as connected");
        a.shutdown().await;
    });
}
