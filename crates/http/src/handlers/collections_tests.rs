use super::*;
use crate::identity_extractor::ExtractIdentity;
use crate::mock::{
    FailingMockExecutor, MockCollectionManagementOperations, MockQueryExecutor, MockRestOperations,
};
use crate::router::{AppStateBuilder, CollectionManagementOperations};
use axum::body::{to_bytes, Body};
use axum::http::{Method, Request, StatusCode};
use query::executor::QueryExecutor;
use query::rest::RestOperations;
use serde_json::json;
use std::sync::Arc;
use tower::ServiceExt;

fn create_state() -> AppState {
    AppStateBuilder::new(
        Arc::new(MockQueryExecutor::new()) as Arc<dyn QueryExecutor>,
        edge::testing::edge(),
    )
    .with_rest(Arc::new(MockRestOperations::new()) as Arc<dyn RestOperations>)
    .with_collection_mgmt(Arc::new(MockCollectionManagementOperations::new()))
    .build()
}

fn create_failing_state() -> AppState {
    AppStateBuilder::new(
        Arc::new(FailingMockExecutor::with_schema_error("test error")) as Arc<dyn QueryExecutor>,
        edge::testing::edge(),
    )
    .build()
}

#[tokio::test]
async fn test_list_collections() {
    let state = create_state();
    let identity = ExtractIdentity::anonymous();
    let result = list_collections(
        State(state),
        identity,
        HeaderMap::new(),
        CollectionSelectorQuery::default(),
    )
    .await;
    assert!(result.is_ok());
    let response = result.unwrap();
    // The listing reads the stored versions, which is what the collection
    // management mock serves.
    assert!(response
        .iter()
        .any(|version| version.name == "MockCollection"));
}

#[tokio::test]
async fn test_list_collections_error() {
    let state = create_failing_state();
    let identity = ExtractIdentity::anonymous();
    let result = list_collections(
        State(state),
        identity,
        HeaderMap::new(),
        CollectionSelectorQuery::default(),
    )
    .await;
    assert!(result.is_err());
}

fn router_with_collection_mgmt() -> axum::Router {
    router_with_collection_mgmt_operations(Arc::new(MockCollectionManagementOperations::new()))
}

fn router_with_collection_mgmt_operations(
    operations: Arc<dyn CollectionManagementOperations>,
) -> axum::Router {
    let state = AppStateBuilder::new(
        Arc::new(MockQueryExecutor::new()) as Arc<dyn QueryExecutor>,
        edge::testing::edge(),
    )
    .with_collection_mgmt(operations)
    .build();
    crate::router::create_router_with_state(state)
}

fn router_with_rest() -> axum::Router {
    let state = AppStateBuilder::new(
        Arc::new(MockQueryExecutor::new()) as Arc<dyn QueryExecutor>,
        edge::testing::edge(),
    )
    .with_rest(Arc::new(MockRestOperations::new()) as Arc<dyn RestOperations>)
    .build();
    crate::router::create_router_with_state(state)
}

fn router_with_executor(executor: Arc<MockQueryExecutor>) -> axum::Router {
    let state =
        AppStateBuilder::new(executor as Arc<dyn QueryExecutor>, edge::testing::edge()).build();
    crate::router::create_router_with_state(state)
}

/// The status of `DELETE /collections/Users/truncate` with `body`.
fn truncate_users(router: axum::Router, body: &'static str) -> StatusCode {
    let request = Request::builder()
        .method(Method::DELETE)
        .uri("/api/v0/collections/Users/truncate")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    edge::testing::edge()
        .block_on(router.oneshot(request))
        .unwrap()
        .unwrap()
        .status()
}

#[test]
fn truncate_collection_hands_the_core_whatever_filter_it_gets() {
    let executor = Arc::new(MockQueryExecutor::new());
    let router = router_with_executor(executor.clone());

    for body in [
        r#"{"filter":{"name":{"_eq":"Alice"}}}"#,
        r#"{"filter":4}"#,
        "",
    ] {
        assert_eq!(truncate_users(router.clone(), body), StatusCode::OK);
    }

    assert_eq!(
        executor.truncated_collections(),
        vec![
            (
                "Users".to_owned(),
                Some(json!({"name": {"_eq": "Alice"}}).into())
            ),
            ("Users".to_owned(), Some(json!(4).into())),
            ("Users".to_owned(), None),
        ]
    );
}

#[test]
fn truncate_collection_rejects_a_body_without_a_filter() {
    for body in ["{}", r#"{"filter":null}"#, "not json"] {
        let executor = Arc::new(MockQueryExecutor::new());

        let status = truncate_users(router_with_executor(executor.clone()), body);

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(executor.truncated_collections().is_empty());
    }
}

#[tokio::test]
async fn collection_doc_ids_returns_paginated_metadata() {
    let router = router_with_rest();
    let response = router
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v0/collections/Users?limit=1&offset=0")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        value,
        json!({
            "doc_ids": ["bae-123"],
            "total": 2,
            "has_more": true,
            "offset": 0,
            "limit": 1
        })
    );
}

#[tokio::test]
async fn collection_doc_ids_uses_default_limit() {
    let router = router_with_rest();
    let response = router
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v0/collections/Users")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(value["doc_ids"], json!(["bae-123", "bae-456"]));
    assert_eq!(value["total"], json!(2));
    assert_eq!(value["has_more"], json!(false));
    assert_eq!(value["offset"], json!(0));
    assert_eq!(value["limit"], json!(100));
}

#[tokio::test]
async fn collection_doc_ids_rejects_limit_above_max() {
    let router = router_with_rest();
    let response = router
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v0/collections/Users?limit=1001")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn delete_collections_by_query_returns_ok_for_single_name() {
    let router = router_with_collection_mgmt();
    let response = router
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri("/api/v0/collections?name=Users")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn delete_collections_by_query_returns_ok_for_csv_names() {
    let router = router_with_collection_mgmt();
    let response = router
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri("/api/v0/collections?name=Users,Books&active-only=true")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn delete_collections_by_query_rejects_missing_name_param() {
    let router = router_with_collection_mgmt();
    let response = router
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri("/api/v0/collections")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn delete_collections_by_query_rejects_invalid_active_only() {
    let router = router_with_collection_mgmt();
    let response = router
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri("/api/v0/collections?name=Users&active-only=not-a-bool")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn patch_collection_rejects_migration_file_paths() {
    let operations = Arc::new(MockCollectionManagementOperations::new());
    let router = router_with_collection_mgmt_operations(operations.clone());
    let response = router
        .oneshot(
            Request::builder()
                .method(Method::PATCH)
                .uri("/api/v0/collections")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "Patch": [{
                            "op": "add",
                            "path": "/Users/Fields/-",
                            "value": {"Name": "age", "Kind": "Int"}
                        }],
                        "Migration": {"Lenses": [{"Path": "/tmp/migration.wasm"}]}
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
