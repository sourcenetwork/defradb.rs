#![cfg(feature = "test-utils")]

//! A pubsub_rpc request that reaches a peer through a relay names its
//! publisher, not the relay (#2773).
//!
//! The asker answers nothing it did not ask, so a reply addressed to the
//! relay's `_response` topic is lost. Here the relay is the only path: the
//! hosts form a line, asker - relay - responder. Only the relay listens, so
//! the asker and the responder can never dial each other (the DHT learns only
//! their outbound ports), and every copy the responder gets was relayed.

use std::time::Duration;

use nuclei::time::timeout;
use p2p::testutil::MockBitswapStore;
use p2p::{HostEvent, P2PHost, P2PHostHandle, PeerId};

const TOPIC: &str = "sync-branchable";

async fn host(listen: bool) -> (P2PHostHandle, nuclei::sync::mpsc::Receiver<HostEvent>) {
    let (host, handle, events, _replicators) =
        P2PHost::new(MockBitswapStore::new(), edge::testing::edge())
            .await
            .unwrap();
    nuclei::spawn(host.run()).detach();
    handle.subscribe_raw(TOPIC.to_string()).await.unwrap();
    handle
        .register_pubsub_rpc_topic(TOPIC.to_string())
        .await
        .unwrap();
    if listen {
        handle
            .listen("/ip4/127.0.0.1/tcp/0".parse().unwrap())
            .await
            .unwrap();
    }
    (handle, events)
}

async fn connect(from: &P2PHostHandle, to: &P2PHostHandle) {
    let addr = to.listen_addresses().await.unwrap().remove(0);
    from.dial(to.local_peer_id_cached(), vec![addr])
        .await
        .unwrap();
}

/// The first raw message on `TOPIC` the host reports, with who relayed it and
/// who published it.
async fn next_request(events: &mut nuclei::sync::mpsc::Receiver<HostEvent>) -> (PeerId, PeerId) {
    loop {
        match events.recv().await.expect("event channel closed") {
            HostEvent::GossipRawMessage {
                propagation_source,
                source,
                topic,
                ..
            } if topic == TOPIC => return (propagation_source, source),
            _ => continue,
        }
    }
}

#[nuclei::test]
async fn a_relayed_request_names_its_publisher() {
    let (asker, _asker_events) = host(false).await;
    let (relay, _relay_events) = host(true).await;
    let (responder, mut responder_events) = host(false).await;
    connect(&asker, &relay).await;
    connect(&responder, &relay).await;

    // The relay forwards only to its mesh, which forms on gossipsub's
    // heartbeat; a request published before then reaches the relay alone. Each
    // publish is a new message, so the first the responder reports is the
    // first that crossed the relay.
    let received = timeout(Duration::from_secs(20), async {
        loop {
            let _ = asker
                .publish_raw(TOPIC.to_string(), b"request".to_vec())
                .await;
            if let Ok(received) = timeout(
                Duration::from_millis(500),
                next_request(&mut responder_events),
            )
            .await
            {
                return received;
            }
        }
    })
    .await
    .expect("the responder never got the request through the relay");

    let (propagation_source, source) = received;
    assert_eq!(propagation_source, relay.local_peer_id_cached());
    assert_eq!(source, asker.local_peer_id_cached());

    for handle in [asker, relay, responder] {
        handle.shutdown().await.unwrap();
    }
}
