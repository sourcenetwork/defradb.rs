//! A request stays admitted on the server's edge until the work it started on
//! the worker pool ends. Neither the request timeout nor the edge's task
//! deadline ends its admission first, so a drain of that edge's scope (a
//! node's shutdown) waits for the write before it stops P2P.

use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use defra_http::{Server, ServerConfig};
use edge::{Edge, EdgeConfig, EdgeRuntime};
use query::error::{Result, TransactionError};
use query::executor::{QueryExecutor, QueryRequest, QueryResponse};
use query::TransactionHandle;
use regolith::sync::Notify;
use tower::ServiceExt;

/// A query that starts, waits for `release`, then ends.
#[derive(Default)]
struct Held {
    started: Notify,
    release: Notify,
    ended: AtomicBool,
}

/// An executor whose every query is the one [`Held`] query.
struct HeldExecutor(Arc<Held>);

#[async_trait]
impl QueryExecutor for HeldExecutor {
    fn abandon_txn(&self, _handle: &TransactionHandle) {}

    async fn execute(&self, _request: QueryRequest) -> QueryResponse {
        self.0.started.notify_one();
        self.0.release.notified().await;
        self.0.ended.store(true, Ordering::SeqCst);
        QueryResponse::error("released")
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

/// Holds a GraphQL write on the pool past `held`, drains the scope of
/// `network` the server runs on, and returns the request's status once the
/// write was released.
fn drain_a_held_write(network: &Edge, config: ServerConfig, held: Duration) -> StatusCode {
    let pool = workers::Pool::start(workers::Config::unplaced(1)).unwrap();
    let scope = network.scope();
    let query = Arc::new(Held::default());
    let router = Server::with_config(
        HeldExecutor(Arc::clone(&query)),
        config,
        scope.edge().clone(),
    )
    .with_pool(Some(pool.clone()))
    .router()
    .unwrap();
    let status = network
        .block_on(async {
            let request = Request::post("/api/v0/graphql")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"query": "mutation { add_User(input: {name: \"a\"}) { _docID } }"}"#,
                ))
                .unwrap();
            let response = nuclei::spawn(router.oneshot(request));
            query.started.notified().await;
            nuclei::time::sleep(held).await;

            let drain = scope.drain(Duration::from_secs(10));
            let mut drain = std::pin::pin!(drain);
            assert!(
                nuclei::time::timeout(Duration::from_millis(300), &mut drain)
                    .await
                    .is_err(),
                "the drain ended while the request's write still ran on the pool"
            );
            query.release.notify_one();
            assert!(drain.await.is_complete(), "the drain timed out");
            assert!(query.ended.load(Ordering::SeqCst));
            response.join().await.unwrap().unwrap().status()
        })
        .unwrap();
    pool.shutdown_blocking().unwrap();
    status
}

#[test]
fn a_timed_out_request_stays_admitted_until_its_pool_work_ends() {
    let config = ServerConfig {
        request_timeout: 1,
        ..ServerConfig::default()
    };
    let status = drain_a_held_write(&edge::testing::edge(), config, Duration::from_millis(1500));
    assert_eq!(status, StatusCode::REQUEST_TIMEOUT);
}

#[test]
fn the_edge_deadline_does_not_end_a_requests_admission() {
    let runtime = EdgeRuntime::start(EdgeConfig {
        threads: NonZeroUsize::new(2),
        deadline: Duration::from_secs(1),
        ..EdgeConfig::default()
    })
    .unwrap();
    let config = ServerConfig {
        request_timeout: 0,
        ..ServerConfig::default()
    };
    let status = drain_a_held_write(runtime.edge(), config, Duration::from_millis(1500));
    assert_eq!(status, StatusCode::OK);
    runtime.shutdown().unwrap();
}
