//! Every Defra protocol shares one vinc connection per peer, and a failed
//! stream leaves the connection up.

use super::{connect, next_two_stream_request, run, signed_request, TestNode, IO_TIMEOUT};
use crate::message::{DocSyncReply, DocSyncRequest, PushLogReply, PushSEArtifactsRequest};
use crate::signing::sign_with_transport;
use crate::transport::{P2PTransport, TransportEvent};
use crate::vinc::VincTransport;

fn cid_for_test() -> cid::Cid {
    use multihash_codetable::{Code, MultihashDigest};
    cid::Cid::new_v1(0x71, Code::Sha2_256.digest(b"vinc mux test"))
}

async fn open_connections(node: &VincTransport, peer: &VincTransport) -> u32 {
    node.peer_connection(peer.local_peer_id())
        .await
        .unwrap()
        .expect("the peer is connected")
        .open_connections
}

#[test]
fn every_protocol_shares_one_connection() {
    run(async {
        let a = TestNode::spawn().await;
        let mut b = TestNode::spawn().await;
        connect(&a.transport, &b.transport).await;

        // A push, answered on its stream.
        let push = signed_request(&a.transport, "mux");
        let transport = a.transport.clone();
        let target = b.id.clone();
        let sent =
            crate::rt::spawn(async move { transport.send_two_stream_request(&target, push).await });
        let (_, request, token) = next_two_stream_request(&mut b).await;
        let mut ack = PushLogReply::success(&request.message_id);
        sign_with_transport(&b.transport, &mut ack).unwrap();
        b.transport.send_pushlog_response(token, ack).await.unwrap();
        sent.await.unwrap();

        // A document sync, answered on its stream and handed back as an event.
        let transport = a.transport.clone();
        let target = b.id.clone();
        let doc_sync = crate::rt::spawn(async move {
            transport
                .send_doc_sync_request(&target, DocSyncRequest::new(vec!["doc-1".into()]))
                .await
        });
        let token = b
            .next(|event| match event {
                TransportEvent::DocSyncRequest {
                    token: Some(token), ..
                } => Some(token),
                _ => None,
            })
            .await;
        b.transport
            .send_doc_sync_response_token(token, DocSyncReply::success("", Vec::new()))
            .await
            .unwrap();
        doc_sync.await.unwrap();

        // A one-way searchable-encryption push.
        a.transport
            .send_se_artifacts(
                &b.id,
                PushSEArtifactsRequest::new("collection1", Vec::new()),
            )
            .await
            .unwrap();
        b.next(|event| matches!(event, TransportEvent::SEArtifactsReceived { .. }).then_some(()))
            .await;

        // An identity challenge to a node with no identity.
        assert_eq!(a.transport.get_peer_identity(&b.id).await.unwrap(), None);

        assert_eq!(open_connections(&a.transport, &b.transport).await, 1);
        assert_eq!(open_connections(&b.transport, &a.transport).await, 1);
        a.shutdown().await;
        b.shutdown().await;
    });
}

#[test]
fn a_car_fetch_rides_the_same_connection_at_bulk_urgency() {
    run(async {
        let mut a = TestNode::spawn().await;
        let mut b = TestNode::spawn().await;
        connect(&a.transport, &b.transport).await;
        let root = cid_for_test();

        let transport = a.transport.clone();
        let target = b.id.clone();
        let fetch =
            crate::rt::spawn(async move { transport.send_car_request(&target, root).await });
        let token = b
            .next(|event| match event {
                TransportEvent::CarFetchRequest {
                    request,
                    token: Some(token),
                    ..
                } => (request.root_cid == root).then_some(token),
                _ => None,
            })
            .await;
        b.transport
            .send_car_response_token(token, b"not really a car".to_vec())
            .await
            .unwrap();
        fetch.await.unwrap();
        let car = a
            .next(|event| match event {
                TransportEvent::CarFetchResponse {
                    root_cid, car_data, ..
                } => (root_cid == root).then_some(car_data),
                _ => None,
            })
            .await;
        assert_eq!(car, b"not really a car");
        assert_eq!(open_connections(&a.transport, &b.transport).await, 1);
        a.shutdown().await;
        b.shutdown().await;
    });
}

#[test]
fn a_reset_stream_does_not_tear_down_the_connection() {
    run(async {
        let a = TestNode::spawn().await;
        let mut b = TestNode::spawn().await;
        connect(&a.transport, &b.transport).await;

        // B drops the token: A's push fails on its own stream only.
        let push = signed_request(&a.transport, "dropped");
        let transport = a.transport.clone();
        let target = b.id.clone();
        let sent =
            crate::rt::spawn(async move { transport.send_two_stream_request(&target, push).await });
        let (_, _, token) = next_two_stream_request(&mut b).await;
        drop(token);
        let failed = crate::rt::time::timeout(IO_TIMEOUT, sent)
            .await
            .expect("a dropped token resets the stream at once");
        assert!(failed.is_err());

        // The next push on the same connection succeeds.
        let push = signed_request(&a.transport, "after");
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
        assert_eq!(open_connections(&a.transport, &b.transport).await, 1);
        a.shutdown().await;
        b.shutdown().await;
    });
}
