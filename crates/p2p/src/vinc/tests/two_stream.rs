//! A replicator push gets its ack on the request's own stream.

use std::time::Duration;

use rapidhash::{HashSetExt, RapidHashSet};

use super::{connect, next_two_stream_request, run, signed_request, TestNode, IO_TIMEOUT};
use crate::message::PushLogReply;
use crate::signing::sign_with_transport;
use crate::transport::P2PTransport;

#[test]
fn two_stream_request_receives_reply_on_request_stream() {
    run(async {
        let sender = TestNode::spawn().await;
        let mut receiver = TestNode::spawn().await;
        connect(&sender.transport, &receiver.transport).await;

        let request = signed_request(&sender.transport, "same-stream");
        let message_id = request.message_id.clone();
        let transport = sender.transport.clone();
        let target = receiver.id.clone();
        let send =
            crate::rt::spawn(
                async move { transport.send_two_stream_request(&target, request).await },
            );

        let (peer_id, request, token) = next_two_stream_request(&mut receiver).await;
        assert_eq!(&peer_id, &sender.id);
        assert!(request.supports_same_stream_reply);
        assert!(request.supports_retry_after);
        let mut reply = PushLogReply::success(&request.message_id);
        sign_with_transport(&receiver.transport, &mut reply).unwrap();
        receiver
            .transport
            .send_pushlog_response(token, reply)
            .await
            .unwrap();

        let reply = crate::rt::time::timeout(IO_TIMEOUT, send)
            .await
            .expect("the ack arrives")
            .unwrap();
        assert_eq!(reply.message_id, message_id);
        assert!(reply.err_message.is_none());
        sender.shutdown().await;
        receiver.shutdown().await;
    });
}

#[test]
fn disconnected_two_stream_requests_release_their_callers_before_the_reply_timeout() {
    run(async {
        let sender = TestNode::spawn().await;
        let mut receiver = TestNode::spawn().await;
        connect(&sender.transport, &receiver.transport).await;

        let mut sends = Vec::new();
        for index in 0..4 {
            let request = signed_request(&sender.transport, &format!("disconnect-{index}"));
            let transport = sender.transport.clone();
            let peer = receiver.id.clone();
            sends.push(crate::rt::spawn(async move {
                transport.send_two_stream_request(&peer, request).await
            }));
        }
        let mut tokens = Vec::new();
        for _ in 0..4 {
            let (_, _, token) = next_two_stream_request(&mut receiver).await;
            tokens.push(token);
        }
        receiver.transport.disconnect(&sender.id).await.unwrap();
        let completed = crate::rt::time::timeout(Duration::from_secs(2), async {
            for send in &mut sends {
                assert!(send.await.is_err());
            }
        })
        .await;
        drop(tokens);
        sender.shutdown().await;
        receiver.shutdown().await;
        assert!(
            completed.is_ok(),
            "a closed vinc connection held pending pushes until the reply timeout"
        );
    });
}

#[test]
fn concurrent_two_stream_fan_in_replies_on_request_streams() {
    const SENDERS: usize = 6;
    run(async {
        let mut receiver = TestNode::spawn().await;
        let mut senders = Vec::with_capacity(SENDERS);
        for index in 0..SENDERS {
            let sender = TestNode::spawn().await;
            connect(&sender.transport, &receiver.transport).await;
            let request = signed_request(&sender.transport, &format!("fan-in-{index}"));
            let target = receiver.id.clone();
            let transport = sender.transport.clone();
            let send = crate::rt::spawn(async move {
                transport.send_two_stream_request(&target, request).await
            });
            senders.push((sender, send));
        }

        let mut peers = RapidHashSet::with_capacity(SENDERS);
        for _ in 0..SENDERS {
            let (peer_id, request, token) = next_two_stream_request(&mut receiver).await;
            peers.insert(peer_id);
            let mut reply = PushLogReply::success(&request.message_id);
            sign_with_transport(&receiver.transport, &mut reply).unwrap();
            receiver
                .transport
                .send_pushlog_response(token, reply)
                .await
                .unwrap();
        }
        assert_eq!(peers.len(), SENDERS);

        crate::rt::time::timeout(IO_TIMEOUT, async {
            for (_, send) in &mut senders {
                let reply = send.await.unwrap();
                assert!(reply.err_message.is_none());
            }
        })
        .await
        .expect("fan-in replies arrive");

        for (sender, _) in senders {
            sender.shutdown().await;
        }
        receiver.shutdown().await;
    });
}
