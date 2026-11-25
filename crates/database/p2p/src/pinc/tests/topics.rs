//! Topic pubsub through a pinc hub: JOINED and LEFT, head hints, raw topics,
//! the message size limit, and a node on no mesh.

use bytes::Bytes;

use super::{connect, loopback, run, seed, TestNode, IO_TIMEOUT};
use crate::message::PushLogBroadcast;
use crate::pinc::{spawn_hub, PincEndpointConfig};
use crate::topics::DefraTopic;
use crate::transport::{P2PTransport, PeerId, TransportEvent};

async fn hub() -> String {
    let (ticket, task) = spawn_hub(*seed(), loopback(), edge::testing::edge())
        .await
        .unwrap();
    // ponytail: the hub runs until the test edge closes with the process.
    std::mem::forget(task);
    ticket
}

async fn node_on(hub: &str) -> TestNode {
    node_as(hub, seed()).await
}

async fn node_as(hub: &str, identity: zeroize::Zeroizing<[u8; 32]>) -> TestNode {
    let mut config = PincEndpointConfig::new(identity, loopback());
    config.max_connections = 8;
    config.hubs = vec![hub.to_owned()];
    TestNode::with_config(config).await
}

async fn joined(node: &mut TestNode, topic: &str) -> PeerId {
    node.next(|event| match event {
        TransportEvent::PeerSubscribed { peer_id, topic: on } if on == topic => Some(peer_id),
        _ => None,
    })
    .await
}

async fn head_hint(node: &mut TestNode) -> (PeerId, String, PushLogBroadcast) {
    node.next(|event| match event {
        TransportEvent::GossipMessage {
            propagation_source,
            topic,
            message,
            ..
        } => Some((propagation_source, topic, message)),
        _ => None,
    })
    .await
}

/// Whether `node` takes a head hint within a short wait.
async fn heard(node: &mut TestNode) -> bool {
    crate::rt::time::timeout(std::time::Duration::from_millis(50), async {
        loop {
            if let Some(TransportEvent::GossipMessage { .. }) = node.events.recv().await {
                break;
            }
        }
    })
    .await
    .is_ok()
}

fn broadcast(block: Vec<u8>) -> PushLogBroadcast {
    PushLogBroadcast::new(
        "doc-1".to_owned(),
        Bytes::from(vec![1, 2, 3]),
        "collection1".to_owned(),
        "creator1".to_owned(),
        Bytes::from(block),
    )
}

#[test]
fn connected_subscribers_see_each_other_join_and_leave() {
    run(async {
        let hub = hub().await;
        let mut a = node_on(&hub).await;
        let mut b = node_on(&hub).await;
        let topic = DefraTopic::collection("c1");
        assert!(a.transport.subscribe(topic.clone()).await.unwrap());
        assert!(b.transport.subscribe(topic.clone()).await.unwrap());
        connect(&a.transport, &b.transport).await;
        assert_eq!(joined(&mut a, &topic.to_string()).await, b.id);
        assert_eq!(joined(&mut b, &topic.to_string()).await, a.id);
        assert_eq!(
            a.transport.topic_peers(topic.clone()).await.unwrap(),
            vec![b.id.clone()]
        );
        assert!(b.transport.unsubscribe(topic.clone()).await.unwrap());
        let left = a
            .next(|event| match event {
                TransportEvent::PeerUnsubscribed { peer_id, .. } => Some(peer_id),
                _ => None,
            })
            .await;
        assert_eq!(left, b.id);
        a.shutdown().await;
        b.shutdown().await;
    });
}

/// A node that only published on a document topic hears who joins and
/// leaves it, as on gossipsub, but takes none of its messages.
#[test]
fn a_publisher_that_never_subscribed_sees_subscribers_join_and_leave() {
    run(async {
        let hub = hub().await;
        let mut a = node_on(&hub).await;
        let b = node_on(&hub).await;
        let c = node_on(&hub).await;
        let topic = DefraTopic::document("doc-1");
        connect(&a.transport, &b.transport).await;
        a.transport
            .publish(topic.clone(), broadcast(vec![1]))
            .await
            .unwrap();
        assert!(b.transport.subscribe(topic.clone()).await.unwrap());
        assert_eq!(joined(&mut a, &topic.to_string()).await, b.id);
        // The hub pushes c's message to a, a member while it holds the topic.
        c.transport
            .publish(topic.clone(), broadcast(vec![2]))
            .await
            .unwrap();
        assert!(b.transport.unsubscribe(topic.clone()).await.unwrap());
        let left = a
            .next(|event| match event {
                TransportEvent::GossipMessage { .. } => {
                    panic!("a took a message of a topic it never subscribed to")
                }
                TransportEvent::PeerUnsubscribed { peer_id, .. } => Some(peer_id),
                _ => None,
            })
            .await;
        assert_eq!(left, b.id);
        a.shutdown().await;
        b.shutdown().await;
        c.shutdown().await;
    });
}

/// A head hint arrives with its publisher as the provider, its block over
/// iroh's 4 KiB gossip limit and under pinc's 64 KiB.
#[test]
fn a_head_hint_with_a_block_over_4_kib_reaches_a_connected_subscriber() {
    run(async {
        let hub = hub().await;
        let a = node_on(&hub).await;
        let mut b = node_on(&hub).await;
        let topic = DefraTopic::document("doc-1");
        b.transport.subscribe(topic.clone()).await.unwrap();
        a.transport.subscribe(topic.clone()).await.unwrap();
        connect(&a.transport, &b.transport).await;
        joined(&mut b, &topic.to_string()).await;
        let block = vec![7; 32 * 1024];
        a.transport
            .publish(topic.clone(), broadcast(block.clone()))
            .await
            .unwrap();
        let (source, on, message) = head_hint(&mut b).await;
        assert_eq!(source, a.id);
        assert_eq!(on, topic.to_string());
        assert_eq!(message.block.as_ref(), block.as_slice());
        assert_eq!(message.authenticated_source_peer_id(), Some(a.id.as_str()));
        a.shutdown().await;
        b.shutdown().await;
    });
}

#[test]
fn a_head_hint_over_the_topic_message_limit_is_refused() {
    run(async {
        let hub = hub().await;
        let a = node_on(&hub).await;
        let result = a
            .transport
            .publish(DefraTopic::document("doc-1"), broadcast(vec![7; 65 * 1024]))
            .await;
        assert!(result.is_err(), "{result:?}");
        a.shutdown().await;
    });
}

/// The KMS shape: a raw request on one topic, answered on the requester's own
/// response topic, neither decoded as a PushLog.
#[test]
fn raw_topics_carry_a_request_and_its_answer_as_bytes() {
    run(async {
        let hub = hub().await;
        let mut a = node_on(&hub).await;
        let mut b = node_on(&hub).await;
        let answers = format!("encryption/{}/_response", a.id);
        b.transport.subscribe(DefraTopic::Encryption).await.unwrap();
        b.transport
            .register_pubsub_rpc_topic("encryption".to_owned())
            .await
            .unwrap();
        a.transport.subscribe_raw(answers.clone()).await.unwrap();
        connect(&a.transport, &b.transport).await;
        a.transport
            .publish_raw("encryption".to_owned(), b"doc-1?".to_vec())
            .await
            .unwrap();
        let (from, request) = b
            .next(|event| match event {
                TransportEvent::GossipRawMessage {
                    source,
                    topic,
                    data,
                    ..
                } if topic == "encryption" => Some((source, data)),
                _ => None,
            })
            .await;
        assert_eq!((from, request.as_slice()), (a.id.clone(), &b"doc-1?"[..]));
        b.transport
            .publish_raw(answers.clone(), b"key".to_vec())
            .await
            .unwrap();
        let answer = a
            .next(|event| match event {
                TransportEvent::GossipRawMessage { topic, data, .. } if topic == answers => {
                    Some(data)
                }
                _ => None,
            })
            .await;
        assert_eq!(answer, b"key");
        a.shutdown().await;
        b.shutdown().await;
    });
}

/// A gossip mesh reaches only peers connected to it; a hub reaches every
/// member. A node with no peer session takes nothing the hub hands it.
#[test]
fn a_node_with_no_peer_takes_no_head_hint() {
    run(async {
        let hub = hub().await;
        let a = node_on(&hub).await;
        let mut b = node_on(&hub).await;
        let mut lonely = node_on(&hub).await;
        let topic = DefraTopic::document("doc-1");
        for transport in [&a.transport, &b.transport, &lonely.transport] {
            transport.subscribe(topic.clone()).await.unwrap();
        }
        connect(&a.transport, &b.transport).await;
        joined(&mut b, &topic.to_string()).await;
        a.transport
            .publish(topic.clone(), broadcast(vec![1]))
            .await
            .unwrap();
        head_hint(&mut b).await;
        let heard = crate::rt::time::timeout(IO_TIMEOUT, async {
            while let Some(event) = lonely.events.recv().await {
                if matches!(event, TransportEvent::GossipMessage { .. }) {
                    return true;
                }
            }
            false
        })
        .await;
        assert!(
            !matches!(heard, Ok(true)),
            "a node with no peer took a head hint"
        );
        a.shutdown().await;
        b.shutdown().await;
        lonely.shutdown().await;
    });
}

/// A subscribe no hub confirms fails, and takes its topic back, so the
/// coordinator's retry joins afresh and waits again instead of answering at
/// once from the join the failed one left behind.
#[test]
fn a_subscribe_no_hub_confirms_fails_and_its_retry_waits_again() {
    run(async {
        // A plain endpoint named as the hub: it never confirms a join.
        let silent = TestNode::spawn().await;
        let listening = silent.transport.listen_addresses().await.unwrap();
        let hub = crate::pinc::best_shareable_public_addr(&silent.id, &listening).unwrap();
        let a = node_on(&hub).await;
        let topic = DefraTopic::collection("c1");
        assert!(a.transport.subscribe(topic.clone()).await.is_err());
        assert!(a.transport.subscribe(topic.clone()).await.is_err());
        a.shutdown().await;
        silent.shutdown().await;
    });
}

/// A node that comes back, as the same identity, after the hub's log of its
/// topic moved past it hears that the topic's messages may be missing.
#[test]
fn a_node_back_after_the_hub_log_moved_past_it_hears_its_topic_was_missed() {
    run(async {
        let hub = hub().await;
        let topic = DefraTopic::document("doc-1");
        let name = topic.to_string();
        let publisher = node_on(&hub).await;
        // The watcher takes messages only while it has a peer; that peer is
        // not the publisher, so the burst goes through the hub's log.
        let bystander = node_on(&hub).await;
        let mut watcher = node_on(&hub).await;
        connect(&bystander.transport, &watcher.transport).await;
        let identity = seed();
        let mut first = node_as(&hub, identity.clone()).await;
        connect(&bystander.transport, &first.transport).await;
        assert!(first.transport.subscribe(topic.clone()).await.unwrap());
        assert!(watcher.transport.subscribe(topic.clone()).await.unwrap());
        // The hub keeps the first node's place in its log once it has taken a
        // message: publish until both subscribers have one.
        let (mut first_heard, mut watcher_heard) = (false, false);
        for _ in 0..200 {
            publisher
                .transport
                .publish(topic.clone(), broadcast(b"before".to_vec()))
                .await
                .unwrap();
            first_heard = first_heard || heard(&mut first).await;
            watcher_heard = watcher_heard || heard(&mut watcher).await;
            if first_heard && watcher_heard {
                break;
            }
        }
        assert!(
            first_heard && watcher_heard,
            "no message reached the subscribers"
        );
        first.shutdown().await;
        // More head hints than the hub's log holds (2,048), none to the node
        // that stopped; the watcher hearing the last shows the hub logged them.
        for k in 0..2_200u32 {
            publisher
                .transport
                .publish(topic.clone(), broadcast(k.to_be_bytes().to_vec()))
                .await
                .unwrap();
        }
        publisher
            .transport
            .publish(topic.clone(), broadcast(b"last".to_vec()))
            .await
            .unwrap();
        // The watcher's event queue may drop part of the burst (Event::Lost), so
        // it waits for the last message or for the burst to go quiet.
        crate::rt::time::timeout(std::time::Duration::from_secs(60), async {
            loop {
                let quiet = std::time::Duration::from_secs(2);
                match crate::rt::time::timeout(quiet, watcher.events.recv()).await {
                    Ok(Some(TransportEvent::GossipMessage { message, .. }))
                        if message.block.as_ref() == b"last" =>
                    {
                        break;
                    }
                    Ok(Some(_)) => {}
                    Ok(None) | Err(_) => break,
                }
            }
        })
        .await
        .expect("the burst reached the watcher");
        let mut again = node_as(&hub, identity).await;
        assert!(again.transport.subscribe(topic.clone()).await.unwrap());
        let missed = crate::rt::time::timeout(std::time::Duration::from_secs(60), async {
            loop {
                match again.events.recv().await {
                    Some(TransportEvent::TopicMissed { topic, .. }) => break Some(topic),
                    Some(_) => {}
                    None => break None,
                }
            }
        })
        .await
        .expect("a missed report within a minute");
        assert_eq!(missed, Some(name));
        again.shutdown().await;
        watcher.shutdown().await;
        bystander.shutdown().await;
        publisher.shutdown().await;
    });
}
