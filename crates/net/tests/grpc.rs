//! The gRPC channel speaks HTTP/2 in the clear, with prior knowledge, and
//! rewrites each request to its endpoint.

mod support;

use std::time::Duration;

use bytes::Bytes;
use http::{Request, StatusCode, Version};
use http_body_util::{BodyExt, Full};
use net::grpc::{Channel, Endpoint};
use tower::ServiceExt;

#[nuclei::test]
async fn a_call_reaches_the_endpoint_over_http2() {
    let address = support::start(None).await;
    let channel: Channel<Full<Bytes>> =
        Endpoint::from_shared(format!("http://{address}"), edge::testing::edge())
            .unwrap()
            .connect_timeout(Duration::from_secs(5))
            .connect()
            .await
            .unwrap();

    let request = Request::post("/echo")
        .body(Full::new(Bytes::from_static(b"frame")))
        .unwrap();
    let response = channel.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.version(), Version::HTTP_2);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(body, Bytes::from_static(b"frame"));
}

#[nuclei::test]
async fn an_unreachable_endpoint_fails_at_connect() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);

    let connected = Endpoint::from_shared(format!("http://{address}"), edge::testing::edge())
        .unwrap()
        .connect::<Full<Bytes>>()
        .await;

    assert!(matches!(connected, Err(net::grpc::Error::Connect { .. })));
}

#[test]
fn an_endpoint_needs_a_scheme_and_a_host() {
    assert!(Endpoint::from_shared("localhost:50051", edge::testing::edge()).is_err());
}
