//! A publish too large for gossip reaches the topic's neighbours on the
//! PushLog stream, and the publisher hears how it went.

use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use bytes::Bytes;
use iroh::SecretKey;

use crate::iroh::gossip::MAX_BROADCAST;
use crate::iroh::{spawn_endpoint, IrohDiscoveryConfig, IrohEndpointConfig, IrohTransport};
use crate::message::{PushLogBroadcast, PushLogReply};
use crate::rt::time::timeout;
use crate::sync::DispatchReceiver;
use crate::topics::DefraTopic;
use crate::transport::{P2PTransport, TransportEvent};

use super::*;

type Events = DispatchReceiver<TransportEvent<iroh::endpoint::SendStream>>;

async fn node() -> (IrohTransport, Events, edge::Task<()>) {
    let secret_key = SecretKey::generate();
    let config = IrohEndpointConfig {
        secret_key: secret_key.clone(),
        node_identity: None,
        relay_mode: crate::iroh::IrohRelayModeConfig::Disabled,
        discovery: IrohDiscoveryConfig::Disabled,
        bind_port: None,
        bind_addr: Some(IpAddr::V4(Ipv4Addr::LOCALHOST)),
        max_concurrent_multipath_paths: None,
        allowlist: Default::default(),
    };
    let (command_tx, events, _replicators, task) =
        spawn_endpoint(config, edge::testing::edge()).await.unwrap();
    (IrohTransport::new(command_tx, secret_key), events, task)
}

/// Two connected nodes, neighbours on `topic`.
async fn neighbours(topic: &DefraTopic) -> [(IrohTransport, Events, edge::Task<()>); 2] {
    let (a, b) = (node().await, node().await);
    a.0.dial(b.0.local_peer_id(), b.0.listen_addresses().await.unwrap())
        .await
        .unwrap();
    a.0.poll_until_connected(b.0.local_peer_id(), Duration::from_secs(5))
        .await
        .unwrap();
    a.0.subscribe(topic.clone()).await.unwrap();
    b.0.subscribe(topic.clone()).await.unwrap();
    timeout(Duration::from_secs(10), async {
        loop {
            let peers = a.0.topic_peers(topic.clone()).await.unwrap();
            if peers.contains(b.0.local_peer_id()) {
                return;
            }
            crate::rt::yield_now().await;
        }
    })
    .await
    .expect("the nodes become neighbours on the topic");
    [a, b]
}

fn update(block_len: usize) -> PushLogBroadcast {
    PushLogBroadcast::new(
        "doc".to_string(),
        Bytes::from_static(&[1, 2, 3]),
        "collection".to_string(),
        "creator".to_string(),
        Bytes::from(vec![7; block_len]),
    )
}

/// Answers the first PushLog request `events` brings with `answer`, and
/// returns what it carried.
async fn answer_push(
    transport: &IrohTransport,
    events: &mut Events,
    answer: impl Fn(&str) -> PushLogReply,
) -> crate::message::PushLogRequest {
    loop {
        let event = timeout(Duration::from_secs(10), events.recv())
            .await
            .expect("a PushLog request arrives")
            .expect("event channel open");
        if let TransportEvent::PushLogRequest { request, token, .. } = event {
            let reply = answer(&request.message_id);
            transport.send_pushlog_response(token, reply).await.unwrap();
            return request;
        }
    }
}

async fn stop(nodes: [(IrohTransport, Events, edge::Task<()>); 2]) {
    for (transport, _, task) in nodes {
        transport.shutdown().await.unwrap();
        task.await.unwrap();
    }
}

/// A block bigger than gossip carries still reaches the subscribed peer,
/// from the publisher itself.
#[nuclei::test(threads = 0)]
async fn an_update_too_large_for_gossip_reaches_a_subscribed_peer() {
    let topic = DefraTopic::collection("collection");
    let mut nodes = neighbours(&topic).await;
    let msg = update(3 * MAX_BROADCAST);
    let [(a, ..), (b, b_events, _)] = &mut nodes;
    let (published, request) = futures::join!(
        a.publish(topic.clone(), msg.clone()),
        answer_push(b, b_events, PushLogReply::success),
    );
    published.expect("the publish succeeds once the peer took the block");
    assert_eq!(request.block, msg.block);
    assert_eq!(request.cid, msg.cid);
    stop(nodes).await;
}

/// A peer that refuses the direct push makes the publish fail.
#[nuclei::test(threads = 0)]
async fn a_refused_direct_push_is_reported_to_the_publisher() {
    let topic = DefraTopic::collection("collection");
    let mut nodes = neighbours(&topic).await;
    let [(a, ..), (b, b_events, _)] = &mut nodes;
    let (published, _) = futures::join!(
        a.publish(topic.clone(), update(3 * MAX_BROADCAST)),
        answer_push(b, b_events, |id| PushLogReply::error(id, "no room")),
    );
    let error = published.expect_err("the refusal comes back to the publisher");
    assert!(error.to_string().contains("no room"), "{error}");
    stop(nodes).await;
}

/// A document topic's publish pushes to nobody: the collection topic's
/// publish reaches every peer a direct push is for, so a peer gets one push.
#[nuclei::test(threads = 0)]
async fn a_document_topic_publish_leaves_the_push_to_the_collection_topic() {
    let collection = DefraTopic::collection("collection");
    let mut nodes = neighbours(&collection).await;
    let document = DefraTopic::document("doc");
    let [(a, ..), (b, ..)] = &mut nodes;
    a.subscribe(document.clone()).await.unwrap();
    b.subscribe(document.clone()).await.unwrap();
    timeout(Duration::from_secs(10), async {
        while !a
            .topic_peers(document.clone())
            .await
            .unwrap()
            .contains(b.local_peer_id())
        {
            crate::rt::yield_now().await;
        }
    })
    .await
    .expect("neighbours on the document topic too");
    a.publish(document, update(3 * MAX_BROADCAST))
        .await
        .expect("nobody left to push to on the document topic");
    stop(nodes).await;
}

/// More subscribers than gossip keeps as neighbours: a small update reaches
/// every one (each takes a relayed head from the connected origin), so the
/// large one must too, not only the active view.
#[nuclei::test(threads = 0)]
async fn an_update_too_large_for_gossip_reaches_every_connected_subscriber() {
    const SUBSCRIBERS: usize = 7;
    let topic = DefraTopic::collection("collection");
    let publisher = node().await;
    let mut subscribers = Vec::new();
    for _ in 0..SUBSCRIBERS {
        let subscriber = node().await;
        publisher
            .0
            .dial(
                subscriber.0.local_peer_id(),
                subscriber.0.listen_addresses().await.unwrap(),
            )
            .await
            .unwrap();
        publisher
            .0
            .poll_until_connected(subscriber.0.local_peer_id(), Duration::from_secs(5))
            .await
            .unwrap();
        subscriber.0.subscribe(topic.clone()).await.unwrap();
        subscribers.push(subscriber);
    }
    publisher.0.subscribe(topic.clone()).await.unwrap();
    let neighbours = publisher.0.topic_peers(topic.clone()).await.unwrap();
    assert!(
        neighbours.len() < SUBSCRIBERS,
        "the active view holds {} of {SUBSCRIBERS}",
        neighbours.len()
    );

    let msg = update(3 * MAX_BROADCAST);
    let answers = futures::future::join_all(
        subscribers
            .iter_mut()
            .map(|(transport, events, _)| answer_push(transport, events, PushLogReply::success)),
    );
    let (published, requests) = futures::join!(publisher.0.publish(topic, msg.clone()), answers);
    published.expect("every subscriber took the block");
    assert_eq!(requests.len(), SUBSCRIBERS);
    assert!(requests.iter().all(|request| request.block == msg.block));

    for (transport, _, task) in subscribers.into_iter().chain([publisher]) {
        transport.shutdown().await.unwrap();
        task.await.unwrap();
    }
}

/// A connected peer that does not subscribe to the collection refuses the
/// push as access denied. It was never owed the update, so the publish
/// succeeds.
#[nuclei::test(threads = 0)]
async fn a_peer_that_does_not_subscribe_is_not_owed_the_update() {
    let topic = DefraTopic::collection("collection");
    let mut nodes = neighbours(&topic).await;
    let [(a, ..), (b, b_events, _)] = &mut nodes;
    let (published, _) = futures::join!(
        a.publish(topic.clone(), update(3 * MAX_BROADCAST)),
        answer_push(b, b_events, |id| PushLogReply::error(
            id,
            "access denied: not authorized for collection collection"
        )),
    );
    published.expect("a peer that does not subscribe is not a failure");
    stop(nodes).await;
}

/// A raw publish (a KMS reply) too large for gossip is refused with a typed
/// error, and one that fits is taken.
#[nuclei::test(threads = 0)]
async fn a_raw_publish_too_large_for_gossip_is_refused() {
    let (transport, _events, task) = node().await;
    let error = transport
        .publish_raw("raw".to_string(), vec![0; MAX_BROADCAST + 1])
        .await
        .expect_err("over the budget");
    assert!(
        matches!(
            error,
            Error::GossipTooLarge { size, limit } if size == MAX_BROADCAST + 1 && limit == MAX_BROADCAST
        ),
        "{error:?}"
    );
    transport
        .publish_raw("raw".to_string(), vec![0; MAX_BROADCAST])
        .await
        .expect("at the budget");
    transport.shutdown().await.unwrap();
    task.await.unwrap();
}
