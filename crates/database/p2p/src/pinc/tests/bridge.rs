//! The reply bridge: a token answers its stream once, a dropped token
//! resets it, and a stream whose connection ended takes no reply.

use std::time::Duration;

use super::{connect, next_two_stream_request, run, signed_request, TestNode, IO_TIMEOUT};
use crate::message::PushLogReply;
use crate::transport::P2PTransport;

#[test]
fn a_reply_after_the_connection_ended_is_an_error_not_a_write() {
    run(async {
        let a = TestNode::spawn().await;
        let mut b = TestNode::spawn().await;
        connect(&a.transport, &b.transport).await;
        let push = signed_request(&a.transport, "late");
        let transport = a.transport.clone();
        let target = b.id.clone();
        let sent =
            crate::rt::spawn(async move { transport.send_two_stream_request(&target, push).await });
        let (_, request, token) = next_two_stream_request(&mut b).await;

        b.transport.disconnect(&a.id).await.unwrap();
        assert!(crate::rt::time::timeout(IO_TIMEOUT, sent)
            .await
            .unwrap()
            .is_err());
        let late = crate::rt::time::timeout(
            IO_TIMEOUT,
            b.transport
                .send_pushlog_response(token, PushLogReply::success(&request.message_id)),
        )
        .await
        .expect("a late reply answers at once");
        assert!(late.is_err(), "the stream went with its connection");
        a.shutdown().await;
        b.shutdown().await;
    });
}

#[test]
fn many_dropped_tokens_leave_no_stream_behind() {
    run(async {
        let a = TestNode::spawn().await;
        let mut b = TestNode::spawn().await;
        connect(&a.transport, &b.transport).await;
        let mut sends = Vec::new();
        for index in 0..16 {
            let push = signed_request(&a.transport, &format!("drop-{index}"));
            let transport = a.transport.clone();
            let target = b.id.clone();
            sends.push(crate::rt::spawn(async move {
                transport.send_two_stream_request(&target, push).await
            }));
        }
        for _ in 0..16 {
            let (_, _, token) = next_two_stream_request(&mut b).await;
            drop(token);
        }
        crate::rt::time::timeout(Duration::from_secs(5), async {
            for send in &mut sends {
                assert!(send.await.is_err(), "each dropped token resets its stream");
            }
        })
        .await
        .expect("every requester hears its reset");
        // The connection carries on.
        let push = signed_request(&a.transport, "after-drops");
        let transport = a.transport.clone();
        let target = b.id.clone();
        let sent =
            crate::rt::spawn(async move { transport.send_two_stream_request(&target, push).await });
        let (_, request, token) = next_two_stream_request(&mut b).await;
        b.transport
            .send_pushlog_response(token, PushLogReply::success(&request.message_id))
            .await
            .unwrap();
        sent.await.unwrap();
        a.shutdown().await;
        b.shutdown().await;
    });
}
