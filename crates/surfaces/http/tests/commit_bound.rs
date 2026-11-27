//! A transaction commit is bounded by the server, not by a client's idle clock.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use defra_http::{Server, ServerConfig};
use query::error::{Result, TransactionError};
use query::executor::{QueryExecutor, QueryRequest, QueryResponse};
use query::TransactionHandle;
use regolith::sync::Event;
use tower::ServiceExt;

struct HeldCommit {
    hold_begin: bool,
    started: Event,
    release: Event,
    completed: Event,
}

impl HeldCommit {
    fn new() -> Arc<Self> {
        Self::with_hold_begin(false)
    }

    fn holding_begin() -> Arc<Self> {
        Self::with_hold_begin(true)
    }

    fn with_hold_begin(hold_begin: bool) -> Arc<Self> {
        Arc::new(Self {
            hold_begin,
            started: Event::new(),
            release: Event::new(),
            completed: Event::new(),
        })
    }
}

#[async_trait]
impl QueryExecutor for HeldCommit {
    fn abandon_txn(&self, _handle: &TransactionHandle) {}

    async fn execute(&self, _request: QueryRequest) -> QueryResponse {
        QueryResponse::success(serde_json::json!({}))
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
        if self.hold_begin {
            self.started.set();
            self.release.wait().await;
            self.completed.set();
        }
        Ok(TransactionHandle::new("42".to_owned()))
    }

    async fn commit_txn(
        &self,
        _handle: &TransactionHandle,
    ) -> std::result::Result<(), TransactionError> {
        self.started.set();
        self.release.wait().await;
        self.completed.set();
        Ok(())
    }

    async fn discard_txn(
        &self,
        _handle: &TransactionHandle,
    ) -> std::result::Result<(), TransactionError> {
        Ok(())
    }

    async fn schema(&self) -> Result<String> {
        Ok("type Query { ping: String }".to_owned())
    }
}

fn router(commit: Arc<HeldCommit>, request_timeout: u64) -> axum::Router {
    Server::from_arc_with_config(
        commit,
        ServerConfig {
            request_timeout,
            commit_timeout_ms: 1,
            ..ServerConfig::default()
        },
        edge::testing::edge(),
    )
    .router()
    .unwrap()
}

#[test]
fn a_commit_past_its_bound_answers_unknown_and_keeps_running() {
    edge::testing::edge()
        .block_on(async {
            let commit = HeldCommit::new();
            let response = router(Arc::clone(&commit), 0)
                .oneshot(Request::post("/api/v0/tx/42").body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
            let body = axum::body::to_bytes(response.into_body(), 1024)
                .await
                .unwrap();
            let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(value["outcome"], "unknown");

            commit.release.set();
            nuclei::time::timeout(Duration::from_secs(5), commit.completed.wait())
                .await
                .unwrap();
        })
        .unwrap();
}

#[test]
fn the_bound_is_only_for_transaction_commits() {
    edge::testing::edge()
        .block_on(async {
            let commit = HeldCommit::new();
            let response = router(Arc::clone(&commit), 0)
                .oneshot(Request::post("/api/v0/tx").body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let body = axum::body::to_bytes(response.into_body(), 1024)
                .await
                .unwrap();
            let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(value["id"], 42);
        })
        .unwrap();
}

#[test]
fn a_begin_is_not_bounded_as_a_commit() {
    edge::testing::edge()
        .block_on(async {
            let commit = HeldCommit::holding_begin();
            let router = router(Arc::clone(&commit), 0);
            let begin = nuclei::spawn(
                router.oneshot(Request::post("/api/v0/tx").body(Body::empty()).unwrap()),
            );
            commit.started.wait().await;
            let timed_out = nuclei::time::timeout(Duration::from_millis(50), begin)
                .await
                .is_err();
            assert!(timed_out, "a begin was treated as a commit");
            commit.release.set();
            nuclei::time::timeout(Duration::from_secs(5), commit.completed.wait())
                .await
                .unwrap();
        })
        .unwrap();
}
