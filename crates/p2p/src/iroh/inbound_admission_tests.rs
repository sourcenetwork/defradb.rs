//! A stream is refused before its body is read when its tag is unknown, it is
//! a reply nobody asked that peer for, or its message is larger than its tag
//! allows; and reset when its body stalls.

use std::net::SocketAddr;
use std::time::Duration;

use super::mux_tests::spawn_transport;
use super::protocols;
use crate::transport::{P2PTransport, PeerAddr, PeerId};

const STOPPED: Duration = Duration::from_secs(5);

/// Dial a bare peer that opens one stream back with `tag` and a length prefix
/// of `len`, and return the code our node stopped that stream with, if it did
/// within `wait`.
async fn stop_code_for(tag: &[u8], len: u32, wait: Duration) -> Option<u32> {
    stop_code_for_trickle(tag, len, wait, None, false).await
}

/// Like `stop_code_for`, but after the length the opener sends one body byte
/// every `trickle`, never the whole body.
async fn stop_code_for_trickle(
    tag: &[u8],
    len: u32,
    wait: Duration,
    trickle: Option<Duration>,
    after_se_query: bool,
) -> Option<u32> {
    let bare = iroh::Endpoint::builder(iroh::endpoint::presets::Minimal)
        .relay_mode(iroh::RelayMode::Disabled)
        .alpns(vec![protocols::ALPN_MUX.to_vec()])
        .bind_addr("127.0.0.1:0".parse::<SocketAddr>().unwrap())
        .expect("bind addr")
        .bind()
        .await
        .expect("bind endpoint");
    let bare_id = PeerId::new(bare.id().to_string());
    let bare_addr = *bare.addr().ip_addrs().next().expect("listener address");

    let tag = tag.to_vec();
    let opener = n0_future::task::spawn({
        let bare = bare.clone();
        async move {
            let connection = bare.accept().await?.await.ok()?;
            // Our node's request stream opens only after it expects the reply.
            let _request = if after_se_query {
                Some(connection.accept_bi().await.ok()?)
            } else {
                None
            };
            let (mut send, _recv) = connection.open_bi().await.ok()?;
            protocols::write_stream_tag(&mut send, &tag).await.ok()?;
            send.write_all(&len.to_be_bytes()).await.ok()?;
            if let Some(every) = trickle {
                for _ in 1..len {
                    tokio::time::sleep(every).await;
                    if send.write_all(&[0]).await.is_err() {
                        break;
                    }
                }
            }
            let code = send.stopped().await.ok()??;
            Some(u32::try_from(code.into_inner()).expect("small stop code"))
        }
    });

    let (transport, endpoint_task) = spawn_transport().await;
    transport
        .dial(&bare_id, vec![PeerAddr::new(bare_addr.to_string())])
        .await
        .expect("dial bare peer");
    if after_se_query {
        let request = crate::message::QuerySEArtifactsRequest::new("collection", vec![]);
        let transport = transport.clone();
        let bare_id = bare_id.clone();
        n0_future::task::spawn(async move {
            let _ = transport.send_se_query_request(&bare_id, request).await;
        });
    }
    let code = n0_future::time::timeout(wait, opener)
        .await
        .ok()
        .and_then(|opened| opened.expect("opener task"));

    transport.shutdown().await.unwrap();
    endpoint_task.await.unwrap();
    code
}

#[tokio::test]
async fn an_unknown_tag_is_stopped() {
    assert_eq!(
        stop_code_for(b"/defra-iroh/nope/0.1", 1, STOPPED).await,
        Some(2)
    );
}

#[tokio::test]
async fn an_oversized_message_is_stopped_before_its_body_is_read() {
    let too_large = protocols::MAX_MESSAGE_SIZE as u32 + 1;
    assert_eq!(
        stop_code_for(protocols::STREAM_PUSHLOG, too_large, STOPPED).await,
        Some(3)
    );
}

#[tokio::test]
async fn a_reply_nobody_asked_for_is_stopped_before_its_length_is_read() {
    assert_eq!(
        stop_code_for(protocols::STREAM_SE_QUERY_RESP, 1, STOPPED).await,
        Some(6)
    );
}

#[tokio::test]
async fn a_reply_to_our_request_is_admitted() {
    // Admitted, the node waits for a body that never comes instead of stopping.
    assert_eq!(
        stop_code_for_trickle(
            protocols::STREAM_SE_QUERY_RESP,
            1,
            Duration::from_secs(1),
            None,
            true,
        )
        .await,
        None
    );
}

#[tokio::test]
async fn a_car_reply_on_a_stream_of_its_own_is_an_unknown_tag() {
    // Every peer sends CAR blocks back on the request's stream.
    assert_eq!(
        stop_code_for(b"/defra-iroh/car/0.1/resp", u32::MAX, STOPPED).await,
        Some(2)
    );
}

#[tokio::test]
async fn a_body_that_never_arrives_is_stopped_and_frees_its_room() {
    let deadline = super::inbound_budget::BODY_IDLE_DEADLINE;
    assert_eq!(
        stop_code_for(protocols::STREAM_IDENTITY, 1, deadline + STOPPED).await,
        Some(5)
    );
}

#[tokio::test]
async fn a_body_that_trickles_in_is_stopped_and_frees_its_room() {
    let idle = super::inbound_budget::BODY_IDLE_DEADLINE;
    let len = 1000;
    assert!(Duration::from_secs(2) < idle);
    assert_eq!(
        stop_code_for_trickle(
            protocols::STREAM_IDENTITY,
            len,
            super::inbound_budget::body_deadline(len as usize) + STOPPED,
            Some(Duration::from_secs(2)),
            false,
        )
        .await,
        Some(5)
    );
}
