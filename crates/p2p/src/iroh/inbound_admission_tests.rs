//! A stream is refused before its body is read when its tag is unknown or its
//! message is larger than its tag allows, and reset when its body stalls.

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
            let (mut send, _recv) = connection.open_bi().await.ok()?;
            protocols::write_stream_tag(&mut send, &tag).await.ok()?;
            send.write_all(&len.to_be_bytes()).await.ok()?;
            let code = send.stopped().await.ok()??;
            Some(u32::try_from(code.into_inner()).expect("small stop code"))
        }
    });

    let (transport, endpoint_task) = spawn_transport().await;
    transport
        .dial(&bare_id, vec![PeerAddr::new(bare_addr.to_string())])
        .await
        .expect("dial bare peer");
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
async fn a_car_response_may_be_larger_than_a_general_message() {
    let over_car_limit = protocols::MAX_CAR_SIZE as u32 + 1;
    assert_eq!(
        stop_code_for(protocols::STREAM_CAR_RESP, over_car_limit, STOPPED).await,
        Some(3)
    );
    // Within the CAR limit the node admits the stream and waits for its body.
    let car_sized = protocols::MAX_MESSAGE_SIZE as u32 + 1;
    assert_eq!(
        stop_code_for(
            protocols::STREAM_CAR_RESP,
            car_sized,
            Duration::from_secs(1)
        )
        .await,
        None
    );
}

#[tokio::test]
async fn a_body_that_never_arrives_is_stopped_and_frees_its_room() {
    let deadline = super::inbound_budget::read_deadline(1);
    assert_eq!(
        stop_code_for(protocols::STREAM_IDENTITY, 1, deadline + STOPPED).await,
        Some(5)
    );
}
