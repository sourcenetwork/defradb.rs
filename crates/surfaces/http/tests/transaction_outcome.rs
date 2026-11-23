//! The transaction outcome route's client-visible mapping.

use std::sync::Arc;

use async_trait::async_trait;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    response::Response,
    Router,
};
use defra_http::handlers::tx_outcome;
use defra_http::router::AppStateBuilder;
use query::error::{QueryError, Result, TransactionError};
use query::executor::{QueryExecutor, QueryRequest, QueryResponse};
use query::txn::TransactionOutcome;
use query::TransactionHandle;
use tower::ServiceExt;

#[derive(Debug)]
struct OutcomeExecutor {
    outcome: Option<TransactionOutcome>,
}

impl OutcomeExecutor {
    fn committed() -> Self {
        Self {
            outcome: Some(TransactionOutcome::Committed),
        }
    }

    fn unknown() -> Self {
        Self { outcome: None }
    }
}

#[async_trait]
impl QueryExecutor for OutcomeExecutor {
    fn abandon_txn(&self, _handle: &TransactionHandle) {}

    async fn add_collection_as(
        &self,
        _context: &defra_core::RequestContext,
        _sdl: &str,
    ) -> std::result::Result<Vec<schema::CollectionVersion>, TransactionError> {
        Err(TransactionError::not_supported("outcome test"))
    }

    async fn set_migration_as(
        &self,
        _context: &defra_core::RequestContext,
        _config: &str,
    ) -> std::result::Result<String, TransactionError> {
        Err(TransactionError::not_supported("outcome test"))
    }

    async fn execute(&self, _request: QueryRequest) -> QueryResponse {
        QueryResponse::error("outcome test")
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
        Err(TransactionError::not_supported("outcome test"))
    }

    async fn commit_txn(
        &self,
        _handle: &TransactionHandle,
    ) -> std::result::Result<(), TransactionError> {
        Err(TransactionError::not_supported("outcome test"))
    }

    async fn discard_txn(
        &self,
        _handle: &TransactionHandle,
    ) -> std::result::Result<(), TransactionError> {
        Err(TransactionError::not_supported("outcome test"))
    }

    async fn schema(&self) -> Result<String> {
        Err(QueryError::internal("outcome test"))
    }

    async fn txn_outcome_as(
        &self,
        _context: &defra_core::RequestContext,
        _handle: &TransactionHandle,
    ) -> std::result::Result<TransactionOutcome, TransactionError> {
        self.outcome
            .ok_or_else(|| TransactionError::not_found("transaction '1-1' not found"))
    }
}

fn router(executor: OutcomeExecutor) -> Router {
    Router::new()
        .route("/tx/{id}", axum::routing::get(tx_outcome))
        .with_state(AppStateBuilder::new(Arc::new(executor), edge::testing::edge()).build())
}

async fn get(router: Router, path: &str) -> (StatusCode, String) {
    let response: Response = router
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(path)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, String::from_utf8(body.to_vec()).unwrap())
}

#[nuclei::test]
async fn a_known_outcome_is_mapped_to_its_wire_name() {
    edge::compat(async move {
        let (status, body) = get(router(OutcomeExecutor::committed()), "/tx/1-1").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, r#"{"outcome":"committed"}"#);
    })
    .await;
}

#[nuclei::test]
async fn an_unowned_or_missing_outcome_is_unknown() {
    edge::compat(async move {
        let (status, body) = get(router(OutcomeExecutor::unknown()), "/tx/1-1").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, r#"{"outcome":"unknown"}"#);
    })
    .await;
}

#[nuclei::test]
async fn a_malformed_id_is_bad_request() {
    edge::compat(async move {
        let (status, body) = get(router(OutcomeExecutor::unknown()), "/tx/concurrent").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body.contains("invalid transaction id"), "{body}");
    })
    .await;
}
