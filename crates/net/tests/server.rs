//! A server whose edge stops accepting finishes the requests in flight,
//! closes its idle keep-alive connections, and returns, over HTTP/1 and
//! HTTP/2; a client whose edge stops accepting finishes the requests in
//! flight and lets go of its idle connections.

mod support;

use std::time::Duration;

use net::http::{Client, Version};
use nuclei::time::timeout;

const LIMIT: Duration = Duration::from_secs(10);

fn client() -> Client {
    Client::builder(edge::testing::edge())
        .no_proxy()
        .build()
        .expect("a client")
}

#[test]
fn a_closing_server_finishes_the_requests_in_flight_then_returns() {
    let edge = edge::testing::edge();
    edge.block_on(async {
        let scope = edge.scope();
        let served = support::serve_on(None, scope.edge().clone()).await;
        let address = served.address;
        let idle = client();
        let answer = idle.get(format!("http://{address}/method")).send().await;
        assert_eq!(answer.unwrap().text().await.unwrap(), "GET");
        let slow = edge.spawn(async move {
            client()
                .get(format!("http://{address}/slow"))
                .send()
                .await?
                .text()
                .await
        });
        served.slow_request_started().await;

        scope.close();

        let ended = timeout(LIMIT, served.task)
            .await
            .expect("the server returns once its connections end");
        assert!(matches!(ended, Ok(Ok(()))), "{ended:?}");
        assert_eq!(slow.await.unwrap().unwrap(), "late");
    })
    .unwrap();
}

#[test]
fn a_closing_server_finishes_the_http2_streams_in_flight_then_returns() {
    let edge = edge::testing::edge();
    edge.block_on(async {
        let scope = edge.scope();
        let (config, root) = support::localhost_tls(&[b"h2"]);
        let served = support::serve_on(Some(config), scope.edge().clone()).await;
        let url = format!("https://localhost:{}/slow", served.address.port());
        let reader = Client::builder(edge.clone())
            .no_proxy()
            .tls_built_in_root_certs(false)
            .add_root_certificate(root)
            .build()
            .expect("a client");
        let slow = edge.spawn(async move {
            let response = reader.get(url).send().await?;
            let version = response.version();
            Ok::<_, net::http::Error>((version, response.text().await?))
        });
        served.slow_request_started().await;

        scope.close();

        let ended = timeout(LIMIT, served.task)
            .await
            .expect("the server returns once its streams end");
        assert!(matches!(ended, Ok(Ok(()))), "{ended:?}");
        let (version, text) = slow.await.unwrap().unwrap();
        assert_eq!(version, Version::HTTP_2);
        assert_eq!(text, "late");
    })
    .unwrap();
}

#[test]
fn a_closing_client_finishes_the_request_in_flight_and_frees_its_idle_connections() {
    let edge = edge::testing::edge();
    edge.block_on(async {
        let served = support::serve_on(None, edge.clone()).await;
        let scope = edge.scope();
        let pooled = Client::builder(scope.edge().clone())
            .no_proxy()
            .build()
            .expect("a client");
        let url = format!("http://{}/slow", served.address);
        let request = pooled.clone();
        let slow = edge.spawn(async move { request.get(url).send().await?.text().await });
        served.slow_request_started().await;

        scope.close();

        assert_eq!(slow.await.unwrap().unwrap(), "late");
        let drained = scope.drain(LIMIT).await;
        assert!(drained.is_complete(), "{drained:?}");
        drop(pooled);
    })
    .unwrap();
}
