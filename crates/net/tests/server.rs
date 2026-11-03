//! A server whose edge stops accepting finishes the requests in flight,
//! closes its idle keep-alive connections, and returns, over HTTP/1 and
//! HTTP/2; a client whose edge stops accepting finishes the requests in
//! flight and lets go of its idle connections.

mod support;

use std::convert::Infallible;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use http::{Request, Response};
use http_body_util::Full;
use hyper::body::Incoming;

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

#[test]
fn a_request_in_flight_at_close_opens_a_fresh_connection_and_finishes() {
    let edge = edge::testing::edge();
    edge.block_on(async {
        let scope = edge.scope();
        let second = support::serve_on(None, edge.clone()).await;
        let target = format!("http://{}/method", second.address);
        let inside = scope.edge().clone();
        let entered = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&entered);
        let service = tower::service_fn(move |_request: Request<Incoming>| {
            let inside = inside.clone();
            let target = target.clone();
            let counter = Arc::clone(&counter);
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                inside.closing().await;
                let fetched = Client::builder(inside)
                    .no_proxy()
                    .build()
                    .expect("a client")
                    .get(target)
                    .send()
                    .await
                    .expect("a fresh connection opens while the request is in flight")
                    .text()
                    .await
                    .expect("a body");
                Ok::<_, Infallible>(Response::new(Full::new(Bytes::from(fetched))))
            }
        });
        let listener = nuclei::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a listener");
        let address = listener.get_ref().local_addr().expect("an address");
        let first = scope
            .edge()
            .spawn(net::server::serve(
                listener,
                None,
                service,
                scope.edge().clone(),
            ))
            .within(Duration::MAX);
        let request = edge.spawn(async move {
            client()
                .get(format!("http://{address}/"))
                .send()
                .await?
                .text()
                .await
        });
        while entered.load(Ordering::SeqCst) == 0 {
            nuclei::time::sleep(Duration::from_millis(5)).await;
        }

        scope.close();

        assert_eq!(request.await.unwrap().unwrap(), "GET");
        let drained = scope.drain(LIMIT).await;
        assert!(drained.is_complete(), "{drained:?}");
        let ended = timeout(LIMIT, first).await.expect("the server returns");
        assert!(matches!(ended, Ok(Ok(()))), "{ended:?}");
    })
    .unwrap();
}
