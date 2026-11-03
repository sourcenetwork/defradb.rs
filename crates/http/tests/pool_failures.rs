//! A request whose work on the worker pool panics answers with an internal
//! error; only a pool that is shutting down asks the caller to retry.

mod common;

use async_trait::async_trait;
use axum::routing::get;
use axum::Router;
use common::*;
use defra_http::pool_layer::OnPool;
use defra_http::router::AppStateBuilder;
use query::error::{Result, TransactionError};
use query::executor::{QueryExecutor, QueryRequest, QueryResponse};
use query::TransactionHandle;

/// An executor whose every query panics, as a read that hits a bug does.
struct PanickingExecutor;

#[async_trait]
impl QueryExecutor for PanickingExecutor {
    fn abandon_txn(&self, _handle: &TransactionHandle) {}

    async fn execute(&self, _request: QueryRequest) -> QueryResponse {
        panic!("a read failed")
    }

    async fn execute_in_txn(
        &self,
        request: QueryRequest,
        _handle: &TransactionHandle,
    ) -> QueryResponse {
        self.execute(request).await
    }

    async fn begin_txn(
        &self,
        _readonly: bool,
    ) -> std::result::Result<TransactionHandle, TransactionError> {
        Err(TransactionError::execution("no transactions here"))
    }

    async fn commit_txn(
        &self,
        _handle: &TransactionHandle,
    ) -> std::result::Result<(), TransactionError> {
        Err(TransactionError::execution("no transactions here"))
    }

    async fn discard_txn(
        &self,
        _handle: &TransactionHandle,
    ) -> std::result::Result<(), TransactionError> {
        Err(TransactionError::execution("no transactions here"))
    }

    async fn schema(&self) -> Result<String> {
        Ok(String::new())
    }
}

fn pool() -> workers::Pool {
    workers::Pool::start(workers::Config::unplaced(1)).unwrap()
}

fn graphql_on(pool: &workers::Pool) -> Router {
    let state = AppStateBuilder::new(Arc::new(PanickingExecutor), edge::testing::edge())
        .with_pool(Some(pool.clone()))
        .build();
    defra_http::create_router_with_state(state)
}

fn read() -> Call {
    Call::post("/api/v0/graphql").json(r#"{"query": "query { User { name } }"}"#)
}

async fn panics() -> &'static str {
    panic!("a route failed")
}

fn route_on(pool: &workers::Pool) -> Router {
    Router::new()
        .route("/api/v0/collections", get(panics))
        .route_layer(OnPool::new(pool.clone()))
}

fn collections() -> Call {
    Call::post("/api/v0/collections").method(Method::GET)
}

fn assert_internal((status, body): (StatusCode, String)) {
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
    assert!(!body.contains("shutting down"), "{body}");
}

fn assert_shutting_down((status, body): (StatusCode, String)) {
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert!(body.contains("shutting down"), "{body}");
}

#[tokio::test]
async fn a_graphql_read_that_panics_on_the_pool_is_an_internal_error() {
    let pool = pool();
    assert_internal(read().send_to(graphql_on(&pool)).await);
    assert_internal(read().send_to(graphql_on(&pool)).await);
    pool.shutdown_blocking().unwrap();
}

#[tokio::test]
async fn a_graphql_request_to_a_stopped_pool_asks_for_a_retry() {
    let pool = pool();
    pool.shutdown_blocking().unwrap();
    assert_shutting_down(read().send_to(graphql_on(&pool)).await);
}

#[tokio::test]
async fn a_pooled_route_that_panics_is_an_internal_error() {
    let pool = pool();
    assert_internal(collections().send_to(route_on(&pool)).await);
    pool.shutdown_blocking().unwrap();
}

#[tokio::test]
async fn a_pooled_route_to_a_stopped_pool_asks_for_a_retry() {
    let pool = pool();
    pool.shutdown_blocking().unwrap();
    assert_shutting_down(collections().send_to(route_on(&pool)).await);
}
