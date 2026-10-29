use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::Request,
    http::{header, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};

use crate::{EmbeddedNode, HttpConfig};

async fn filter(request: Request, next: Next) -> Response {
    if request
        .headers()
        .get(header::HOST)
        .is_some_and(|host| host == "blocked.example")
    {
        return StatusCode::MISDIRECTED_REQUEST.into_response();
    }
    next.run(request).await
}

#[test]
fn http_config_wraps_native_extra_and_fallback_routes_once() {
    edge::testing::edge()
        .block_on(async {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let constructions = Arc::new(AtomicUsize::new(0));
            let observed = constructions.clone();
            let config = HttpConfig::with_listener(listener)
                .unwrap()
                .with_extra_routes(
                    Router::new().route("/application", get(|| async { "application" })),
                )
                .with_router_transform(Arc::new(move |router| {
                    observed.fetch_add(1, Ordering::SeqCst);
                    router.layer(middleware::from_fn(filter))
                }));
            let node = EmbeddedNode::builder()
                .with_http(config)
                .build()
                .await
                .unwrap();
            let client = net::http::Client::builder(edge::testing::edge())
                .no_proxy()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap();
            for path in [
                "/health-check",
                "/api/v0/version",
                "/api/v1/version/",
                "/application",
                "/missing/",
            ] {
                let response = client
                    .get(format!("http://{address}{path}"))
                    .header(header::HOST, "blocked.example")
                    .send()
                    .await
                    .unwrap();
                assert_eq!(response.status(), StatusCode::MISDIRECTED_REQUEST, "{path}");
                let response = client
                    .get(format!("http://{address}{path}"))
                    .send()
                    .await
                    .unwrap();
                assert_eq!(
                    response.status(),
                    if path == "/missing/" {
                        StatusCode::NOT_FOUND
                    } else {
                        StatusCode::OK
                    },
                    "{path}"
                );
            }
            assert_eq!(constructions.load(Ordering::SeqCst), 1);
            node.shutdown().await;
        })
        .unwrap();
}
