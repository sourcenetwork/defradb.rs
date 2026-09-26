use std::{sync::Arc, time::Duration};

use axum::{body::Body, http::Request};
use defra_http::{router::AppStateBuilder, MockQueryExecutor};
use events::{Bus, ChannelBus, Message, ReplicatorCompletedData};
use futures::StreamExt;
use serde_json::json;
use tower::ServiceExt;

#[tokio::test]
async fn replay_outcomes_reach_the_event_stream() {
    let bus = Arc::new(ChannelBus::new());
    let state = AppStateBuilder::new(Arc::new(MockQueryExecutor::new()))
        .with_event_bus(bus.clone())
        .build();
    let response = defra_http::create_router_with_state(state)
        .oneshot(
            Request::builder()
                .uri("/api/v0/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let mut stream = response.into_body().into_data_stream();
    for (skipped, error) in [(false, None), (true, None), (false, Some("replay failed"))] {
        bus.publish(Message::replicator_completed_with_data(
            ReplicatorCompletedData {
                peer_id: "peer-a".into(),
                collections: vec!["Note".into()],
                skipped,
                error: error.map(str::to_owned),
            },
        ));
        let frame = tokio::time::timeout(Duration::from_secs(5), stream.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let frame = std::str::from_utf8(&frame).unwrap();
        let data = frame
            .lines()
            .find_map(|line| line.strip_prefix("data: "))
            .unwrap();
        let value: serde_json::Value = serde_json::from_str(data).unwrap();
        assert_eq!(
            value,
            json!({
                "name": "replicator-completed",
                "data": {"peer_id": "peer-a", "collections": ["Note"], "skipped": skipped, "error": error},
            })
        );
    }
    bus.publish(Message::replicator_completed());
    let frame = tokio::time::timeout(Duration::from_secs(5), stream.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let frame = std::str::from_utf8(&frame).unwrap();
    let data = frame
        .lines()
        .find_map(|line| line.strip_prefix("data: "))
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(data).unwrap();
    assert_eq!(value, json!({"name": "replicator-completed", "data": {}}));
}
