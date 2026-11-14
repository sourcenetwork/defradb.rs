//! A stream is stopped before its body is read when its tag is unknown or
//! its message is too large, and an admitted body is held until handled.
//!
//! The peer is a bare vinc endpoint on a thread of its own, so the
//! assertions are about what the node does with bytes on the wire.

use std::rc::Rc;
use std::time::Duration;

use super::{loopback, run, seed, TestNode};
use crate::transport::{P2PTransport, TransportEvent};
use crate::vinc::endpoint::NucleiIo;
use crate::vinc::serve::{RESET_MESSAGE_TOO_LARGE, RESET_UNKNOWN_SERVICE};
use crate::vinc::services::{self, ALPN_MUX};
use crate::vinc::VincTransport;

/// What the bare peer saw after writing `tag` and a length of `len`: the
/// code the node stopped the stream with, or `None` when it did not.
async fn stop_code(node: &VincTransport, tag: &'static [u8], len: u32) -> Option<u64> {
    let addrs = super::super::addr::socket_addrs(&node.listen_addresses().await.unwrap());
    let target = super::super::addr::node_id(node.local_peer_id()).unwrap();
    let seed = seed();
    edge::testing::edge()
        .spawn_pinned(move || async move {
            let socket = nuclei::Handle::<std::net::UdpSocket>::bind(loopback())
                .await
                .unwrap();
            let local = socket.local_addr().unwrap();
            let ep = vinc::Vinc::builder()
                .seed(&seed)
                .unwrap()
                .alpn(ALPN_MUX)
                .local_addr(local)
                .limits(vinc::Limits {
                    connections: 2,
                    ..vinc::Limits::default()
                })
                .build_io(NucleiIo {
                    socket: Rc::new(socket),
                })
                .unwrap();
            let runner = ep.clone();
            let _run = nuclei::spawn_local(async move {
                let _ = runner.run().await;
            });
            let mut to = vinc::NodeAddr::new(target);
            to.direct = addrs;
            let connection = ep.connect(to, ALPN_MUX).await.unwrap();
            let (mut send, _recv) = connection.open_bi_tagged(tag).await.unwrap();
            send.write_all(&len.to_be_bytes()).await.unwrap();
            // Keep writing until the node stops the stream, or give up.
            let code = crate::rt::time::timeout(Duration::from_secs(3), async {
                loop {
                    match send.write(&[0u8; 1024]).await {
                        Ok(_) => crate::rt::time::sleep(Duration::from_millis(10)).await,
                        Err(error) => {
                            return match error.kind() {
                                vinc::ErrorKind::Stopped(code) | vinc::ErrorKind::Reset(code) => {
                                    Some(code)
                                }
                                _ => None,
                            }
                        }
                    }
                }
            })
            .await
            .ok()
            .flatten();
            ep.close().await;
            code
        })
        .await
        .unwrap()
}

#[test]
fn an_unknown_tag_is_stopped() {
    run(async {
        let node = TestNode::spawn().await;
        assert_eq!(
            stop_code(&node.transport, b"/defra-vinc/nope/0.1", 16).await,
            Some(RESET_UNKNOWN_SERVICE)
        );
        node.shutdown().await;
    });
}

#[test]
fn an_oversized_message_is_stopped_before_its_body_is_read() {
    run(async {
        let node = TestNode::spawn().await;
        let too_large = u32::try_from(services::MAX_MANAGE_MSG_SIZE + 1).unwrap();
        assert_eq!(
            stop_code(&node.transport, services::STREAM_MANAGE, too_large).await,
            Some(RESET_MESSAGE_TOO_LARGE)
        );
        let snapshot = node.transport.inbound_budget().await.unwrap();
        let control = snapshot
            .iter()
            .find(|class| class.class == "control")
            .unwrap();
        assert_eq!(control.oversized_resets, 1);
        node.shutdown().await;
    });
}

#[test]
fn an_admitted_body_holds_its_room_until_its_event_is_handled() {
    run(async {
        let a = TestNode::spawn().await;
        let mut b = TestNode::spawn().await;
        super::connect(&a.transport, &b.transport).await;
        let push = super::signed_request(&a.transport, "held");
        let transport = a.transport.clone();
        let target = b.id.clone();
        let _sent =
            crate::rt::spawn(async move { transport.send_two_stream_request(&target, push).await });
        b.next(|event| matches!(event, TransportEvent::TwoStreamRequest { .. }).then_some(()))
            .await;
        // The lease rides with the event in the dispatch queue; the receiver
        // took the event out, so the room is free again.
        let snapshot = b.transport.inbound_budget().await.unwrap();
        let replication = snapshot
            .iter()
            .find(|class| class.class == "replication")
            .unwrap();
        assert!(replication.held_bytes_high_water > 0);
        a.shutdown().await;
        b.shutdown().await;
    });
}

/// The bare peer opens a stream and never writes its tag, then asks for the
/// node's identity on a second stream: whether that answer came back.
async fn answered_past_a_silent_stream(node: &VincTransport) -> bool {
    let addrs = super::super::addr::socket_addrs(&node.listen_addresses().await.unwrap());
    let target = super::super::addr::node_id(node.local_peer_id()).unwrap();
    let seed = seed();
    edge::testing::edge()
        .spawn_pinned(move || async move {
            let socket = nuclei::Handle::<std::net::UdpSocket>::bind(loopback())
                .await
                .unwrap();
            let local = socket.local_addr().unwrap();
            let ep = vinc::Vinc::builder()
                .seed(&seed)
                .unwrap()
                .alpn(ALPN_MUX)
                .local_addr(local)
                .limits(vinc::Limits {
                    connections: 2,
                    ..vinc::Limits::default()
                })
                .build_io(NucleiIo {
                    socket: Rc::new(socket),
                })
                .unwrap();
            let runner = ep.clone();
            let _run = nuclei::spawn_local(async move {
                let _ = runner.run().await;
            });
            let me = vinc::node_to_string(&ep.id());
            let mut to = vinc::NodeAddr::new(target);
            to.direct = addrs;
            let connection = ep.connect(to, ALPN_MUX).await.unwrap();
            // A stream that promises a ten-byte tag and never sends it.
            let (mut silent, _silent_recv) = connection.open_bi().await.unwrap();
            silent.write_all(&[10]).await.unwrap();
            let (mut send, mut recv) = connection
                .open_bi_tagged(services::STREAM_IDENTITY)
                .await
                .unwrap();
            let request = crate::message::IdentityRequest::new(me);
            let body = crate::vinc::wire::encode(&request).unwrap();
            crate::vinc::wire::write_payload(&mut send, &body)
                .await
                .unwrap();
            send.finish().await.unwrap();
            let answered = crate::rt::time::timeout(Duration::from_secs(3), async {
                let len = crate::vinc::wire::read_length(&mut recv).await.ok()?;
                let mut body = vec![0u8; len];
                recv.read_exact(&mut body).await.ok()?;
                let response: crate::message::IdentityResponse =
                    crate::vinc::wire::decode(body).ok()?;
                Some(
                    response.err_message.as_deref()
                        == Some(crate::message::IDENTITY_UNCONFIGURED_ERROR),
                )
            })
            .await
            .ok()
            .flatten()
            .unwrap_or(false);
            ep.close().await;
            answered
        })
        .await
        .unwrap()
}

#[test]
fn a_stream_that_never_names_its_tag_does_not_hold_up_the_next() {
    run(async {
        let node = TestNode::spawn().await;
        assert!(answered_past_a_silent_stream(&node.transport).await);
        node.shutdown().await;
    });
}
