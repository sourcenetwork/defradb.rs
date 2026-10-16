mod common;

use axum::body::Body;
use axum::http::header::{AUTHORIZATION, HOST};
use axum::http::Request;
use defra_http::router::AppStateBuilder;
use defra_http::{MockNodeAcpOperations, MockQueryExecutor};
use events::{Bus, ChannelBus, Message, ReplicatorCompletedData};
use futures::StreamExt;
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;

use common::{NodePermission, StatusCode, TEST_HOST};

async fn frame(can_list: bool, message: Message) -> serde_json::Value {
    let (owner, _) = common::nac_owner();
    let (caller, token) = common::nac_owner();
    let mut nac = MockNodeAcpOperations::enabled_with_owner(owner);
    for permission in [
        NodePermission::DocumentRead,
        NodePermission::P2pPeerInfo,
        NodePermission::DacStatus,
    ] {
        nac = nac.with_grant(caller.clone(), permission);
    }
    if can_list {
        nac = nac.with_grant(caller, NodePermission::P2pReplicatorList);
    }
    let bus = Arc::new(ChannelBus::new());
    let state = AppStateBuilder::new(Arc::new(MockQueryExecutor::new()))
        .with_nac(Arc::new(nac))
        .with_event_bus(bus.clone())
        .build();
    let response = defra_http::create_router_with_state(state)
        .oneshot(
            Request::builder()
                .uri("/api/v0/events")
                .header(HOST, TEST_HOST)
                .header(AUTHORIZATION, token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    bus.publish(message);
    let mut body = response.into_body().into_data_stream();
    let bytes = tokio::time::timeout(Duration::from_secs(1), body.next())
        .await
        .expect("completion event must be delivered")
        .unwrap()
        .unwrap();
    let frame = std::str::from_utf8(&bytes).unwrap();
    let data = frame
        .lines()
        .find_map(|line| line.strip_prefix("data: "))
        .expect("SSE data line");
    serde_json::from_str(data).unwrap()
}

fn completion() -> Message {
    Message::replicator_completed_with_data(ReplicatorCompletedData {
        peer_id: "private-peer".into(),
        collections: vec!["PrivateCollection".into()],
        skipped: false,
        error: Some("private replay error".into()),
    })
}

#[tokio::test]
async fn wildcard_redacts_replicator_details_without_list_permission() {
    assert_eq!(
        frame(false, completion()).await,
        serde_json::json!({"name": "replicator-completed", "data": {}})
    );
}

#[tokio::test]
async fn wildcard_includes_replicator_details_with_list_permission() {
    let frame = frame(true, completion()).await;
    assert_eq!(frame["name"], "replicator-completed");
    assert_eq!(frame["data"]["peer_id"], "private-peer");
    assert_eq!(
        frame["data"]["collections"],
        serde_json::json!(["PrivateCollection"])
    );
    assert_eq!(frame["data"]["skipped"], false);
    assert_eq!(frame["data"]["error"], "private replay error");
}

#[tokio::test]
async fn legacy_completion_stays_payloadless_for_both_permissions() {
    for can_list in [false, true] {
        assert_eq!(
            frame(can_list, Message::replicator_completed()).await,
            serde_json::json!({"name": "replicator-completed", "data": {}})
        );
    }
}
