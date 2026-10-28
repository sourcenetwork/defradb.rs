//! The HTTP client against a server on nuclei sockets.

mod support;

use std::time::Duration;

use futures::StreamExt;
use net::http::tls::Version as TlsVersion;
use net::http::{Client, StatusCode, Version};
use serde_json::{json, Value};

fn url(address: std::net::SocketAddr, path: &str) -> String {
    format!("http://{address}{path}")
}

fn plain() -> Client {
    Client::builder().no_proxy().build().expect("a client")
}

#[nuclei::test]
async fn a_json_body_goes_and_comes_back() {
    let address = support::start(None).await;
    let sent = json!({"name": "Inkheart", "pages": 534});

    let response = plain()
        .post(url(address, "/echo"))
        .json(&sent)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["x-method"], "POST");
    assert_eq!(response.json::<Value>().await.unwrap(), sent);
}

#[nuclei::test]
async fn a_query_is_appended_to_the_one_in_the_url() {
    let address = support::start(None).await;

    let text = plain()
        .get(url(address, "/query?a=1"))
        .query(&[("b", "two words")])
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    assert_eq!(text, "a=1&b=two+words");
}

#[nuclei::test]
async fn an_error_status_is_reported_with_the_status() {
    let address = support::start(None).await;

    let response = plain()
        .get(url(address, "/status/404"))
        .send()
        .await
        .unwrap();
    let error = response.error_for_status().unwrap_err();

    assert!(error.is_status());
    assert_eq!(error.status(), Some(StatusCode::NOT_FOUND));
}

#[nuclei::test]
async fn a_request_past_its_time_limit_fails_as_a_timeout() {
    let address = support::start(None).await;

    let error = plain()
        .get(url(address, "/slow"))
        .timeout(Duration::from_millis(100))
        .send()
        .await
        .unwrap_err();

    assert!(error.is_timeout());
    assert!(!error.is_connect());
}

#[nuclei::test]
async fn a_closed_port_is_a_connect_error() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);

    let error = plain().get(url(address, "/")).send().await.unwrap_err();

    assert!(error.is_connect());
}

#[nuclei::test]
async fn redirects_are_followed_and_change_the_method_as_the_status_says() {
    let address = support::start(None).await;
    let client = plain();
    let method_after = |code: u16| {
        let client = client.clone();
        async move {
            client
                .post(url(address, &format!("/redirect/{code}")))
                .body("payload")
                .send()
                .await
                .unwrap()
                .text()
                .await
                .unwrap()
        }
    };

    assert_eq!(method_after(302).await, "GET");
    assert_eq!(method_after(303).await, "GET");
    assert_eq!(method_after(307).await, "POST");
    assert_eq!(method_after(308).await, "POST");
}

#[nuclei::test]
async fn a_body_streams_in_pieces() {
    let address = support::start(None).await;

    let mut stream = plain()
        .post(url(address, "/echo"))
        .body(vec![7u8; 64 * 1024])
        .send()
        .await
        .unwrap()
        .bytes_stream();
    let mut received = 0;
    while let Some(piece) = stream.next().await {
        received += piece.unwrap().len();
    }

    assert_eq!(received, 64 * 1024);
}

#[nuclei::test]
async fn https_negotiates_http2_and_trusts_an_added_root() {
    let (config, root) = support::localhost_tls(&[b"h2", b"http/1.1"]);
    let address = support::start(Some(config)).await;
    let client = Client::builder()
        .no_proxy()
        .tls_built_in_root_certs(false)
        .add_root_certificate(root)
        .build()
        .unwrap();

    let response = client
        .get(format!("https://localhost:{}/method", address.port()))
        .send()
        .await
        .unwrap();

    assert_eq!(response.version(), Version::HTTP_2);
    assert_eq!(response.text().await.unwrap(), "GET");
}

#[nuclei::test]
async fn http1_only_stays_on_http_1_1_and_tls_versions_can_be_pinned() {
    let (config, root) = support::localhost_tls(&[b"h2", b"http/1.1"]);
    let address = support::start(Some(config)).await;
    let client = Client::builder()
        .no_proxy()
        .tls_built_in_root_certs(false)
        .add_root_certificate(root)
        .http1_only()
        .min_tls_version(TlsVersion::Tls12)
        .max_tls_version(TlsVersion::Tls12)
        .build()
        .unwrap();

    let response = client
        .get(format!("https://localhost:{}/method", address.port()))
        .send()
        .await
        .unwrap();

    assert_eq!(response.version(), Version::HTTP_11);
}

#[nuclei::test]
async fn an_untrusted_certificate_is_refused() {
    let (config, _root) = support::localhost_tls(&[b"http/1.1"]);
    let address = support::start(Some(config)).await;
    let client = Client::builder()
        .no_proxy()
        .tls_built_in_root_certs(false)
        .build()
        .unwrap();

    let error = client
        .get(format!("https://localhost:{}/method", address.port()))
        .send()
        .await
        .unwrap_err();

    assert!(error.is_connect());
}

#[test]
fn a_tls_version_range_with_no_version_is_refused() {
    let built = Client::builder()
        .min_tls_version(TlsVersion::Tls13)
        .max_tls_version(TlsVersion::Tls12)
        .build();

    assert!(built.is_err_and(|error| error.is_builder()));
}
