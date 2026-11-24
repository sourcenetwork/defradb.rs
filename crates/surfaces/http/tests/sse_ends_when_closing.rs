//! An open SSE stream ends with its `complete` event once the server's edge
//! stops admitting requests, so the request drain does not wait out its
//! deadline for it (#2779; proofs/tla/ServerDrain.tla EndStream).

use std::{sync::Arc, time::Duration};

use axum::{body::Body, http::Request};
use defra_http::{router::AppStateBuilder, MockQueryExecutor};
use events::ChannelBus;
use futures::StreamExt;
use query::executor::QueryExecutor;
use tower::ServiceExt;

/// Opens `request` on a server whose edge is `scope`'s, closes the scope, and
/// returns everything the stream sent before it ended.
async fn body_after_closing(request: Request<Body>) -> String {
    let scope = edge::testing::edge().scope();
    let state = AppStateBuilder::new(
        Arc::new(MockQueryExecutor::new()) as Arc<dyn QueryExecutor>,
        scope.edge().clone(),
    )
    .with_event_bus(Arc::new(ChannelBus::new()))
    .build();
    let response = defra_http::create_router_with_state(state)
        .oneshot(request)
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let mut stream = response.into_body().into_data_stream();
    scope.close();
    let mut body = String::new();
    loop {
        let frame = nuclei::time::timeout(Duration::from_secs(5), stream.next())
            .await
            .expect("an open stream must end once its edge closes");
        match frame {
            Some(frame) => body.push_str(std::str::from_utf8(&frame.unwrap()).unwrap()),
            None => return body,
        }
    }
}

#[test]
fn a_graphql_subscription_ends_when_its_edge_closes() {
    let body = edge::testing::edge()
        .block_on(body_after_closing(
            Request::post("/api/v0/graphql")
                .header("content-type", "application/json")
                .header("accept", "text/event-stream")
                .body(Body::from(
                    r#"{"query":"subscription { users { _docID name } }"}"#,
                ))
                .unwrap(),
        ))
        .unwrap();
    assert_eq!(body, "event: complete\ndata: {}\n\n");
}

#[test]
fn the_event_stream_ends_when_its_edge_closes() {
    let body = edge::testing::edge()
        .block_on(body_after_closing(
            Request::get("/api/v0/events").body(Body::empty()).unwrap(),
        ))
        .unwrap();
    assert_eq!(body, "event: complete\ndata: {}\n\n");
}
