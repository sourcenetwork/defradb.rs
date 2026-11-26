//! A broadcast larger than one topic message goes to every connected peer as
//! a direct PushLog request.

use bytes::Bytes;

use super::{connect, run, TestNode};
use crate::hub::MAX_TOPIC_MESSAGE;
use crate::PincResponseToken;
use p2p::message::{PushLogBroadcast, PushLogReply, PushLogRequest};
use p2p::topics::DefraTopic;
use p2p::transport::{P2PTransport, TransportEvent};

fn too_large() -> PushLogBroadcast {
    PushLogBroadcast::new(
        "doc-large".to_owned(),
        Bytes::from_static(&[1, 2, 3]),
        "collection-large".to_owned(),
        "creator".to_owned(),
        Bytes::from(vec![7; MAX_TOPIC_MESSAGE + 1]),
    )
}

async fn next_push(node: &mut TestNode) -> (PushLogRequest, PincResponseToken) {
    node.next(|event| match event {
        TransportEvent::PushLogRequest { request, token, .. } => Some((request, token)),
        _ => None,
    })
    .await
}

/// Each connected peer gets the push signed by the writer; one that does not
/// subscribe declines it, which is no failure, and a failure of one that
/// takes it fails the publish, naming the peer.
#[test]
fn a_broadcast_too_large_for_a_topic_message_is_pushed_to_each_connected_peer() {
    run(async {
        let writer = TestNode::spawn().await;
        let mut subscriber = TestNode::spawn().await;
        let mut stranger = TestNode::spawn().await;
        connect(&writer.transport, &subscriber.transport).await;
        connect(&writer.transport, &stranger.transport).await;
        let topic = DefraTopic::collection("collection-large");

        for fails in [false, true] {
            let transport = writer.transport.clone();
            let published = p2p::rt::spawn({
                let topic = topic.clone();
                async move { transport.publish(topic, too_large()).await }
            });
            let (request, token) = next_push(&mut subscriber).await;
            assert_eq!(request.block.len(), MAX_TOPIC_MESSAGE + 1);
            assert_eq!(request.sender_id, writer.id.as_str());
            assert!(request.signature.is_some(), "the push is signed");
            let reply = if fails {
                PushLogReply::error(&request.message_id, "merge failed")
            } else {
                PushLogReply::success(&request.message_id)
            };
            subscriber
                .transport
                .send_pushlog_response(token, reply)
                .await
                .unwrap();
            let (request, token) = next_push(&mut stranger).await;
            stranger
                .transport
                .send_pushlog_response(
                    token,
                    PushLogReply::error(&request.message_id, "access denied: not subscribed"),
                )
                .await
                .unwrap();
            let published = published.await;
            if fails {
                let error = published.expect_err("a subscriber's failure fails the publish");
                assert!(
                    error.to_string().contains(subscriber.id.as_str()),
                    "{error}"
                );
                assert!(error.to_string().contains("merge failed"), "{error}");
            } else {
                published.expect("every subscriber took the push");
            }
        }
        writer.shutdown().await;
        subscriber.shutdown().await;
        stranger.shutdown().await;
    });
}
