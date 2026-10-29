//! A request still running at the server's time limit is answered with 408;
//! one done in time gets its own answer.

use std::convert::Infallible;

use axum::body::{Body, Bytes};
use axum::http::{Request, StatusCode};
use defra_http::{MockQueryExecutor, Server, ServerConfig};
use tower::ServiceExt;

fn limited() -> axum::Router {
    Server::with_config(
        MockQueryExecutor::new(),
        ServerConfig {
            request_timeout: 1,
            ..ServerConfig::default()
        },
        edge::testing::edge(),
    )
    .router()
    .unwrap()
}

fn query(body: Body) -> Request<Body> {
    Request::post("/api/v0/graphql")
        .header("content-type", "application/json")
        .body(body)
        .unwrap()
}

#[nuclei::test]
async fn a_request_done_in_time_gets_its_answer() {
    let response = limited()
        .oneshot(query(Body::from(r#"{"query":"{__typename}"}"#)))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[nuclei::test]
async fn a_request_past_the_limit_is_answered_with_408() {
    let never_sent = Body::from_stream(futures::stream::pending::<Result<Bytes, Infallible>>());

    let response = limited().oneshot(query(never_sent)).await.unwrap();

    assert_eq!(response.status(), StatusCode::REQUEST_TIMEOUT);
}
