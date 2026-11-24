//! A GraphQL body cannot claim the collection API wrote its filters: the
//! field is never read from the wire, so every request's filters are checked
//! against their input types (`defra.query.filter.input-validation`).

use std::sync::Arc;

use axum::{body::Body, http::Request};
use defra_http::router::AppStateBuilder;
use query::error::TransactionError;
use query::executor::{FilterOrigin, QueryExecutor, QueryRequest, QueryResponse};
use query::TransactionHandle;
use regolith::sync::SegQueue;
use tower::ServiceExt;

/// Records the filter origin of each request it runs.
#[derive(Default)]
struct Recording(SegQueue<FilterOrigin>);

#[async_trait::async_trait]
impl QueryExecutor for Recording {
    async fn execute(&self, request: QueryRequest) -> QueryResponse {
        self.0.push(request.filter_origin);
        QueryResponse::success(serde_json::json!({ "users": [] }))
    }

    async fn execute_in_txn(
        &self,
        request: QueryRequest,
        _handle: &TransactionHandle,
    ) -> QueryResponse {
        self.execute(request).await
    }

    async fn begin_txn(&self, _readonly: bool) -> Result<TransactionHandle, TransactionError> {
        Err(TransactionError::not_supported("test executor"))
    }

    async fn commit_txn(&self, _handle: &TransactionHandle) -> Result<(), TransactionError> {
        Err(TransactionError::not_supported("test executor"))
    }

    async fn discard_txn(&self, _handle: &TransactionHandle) -> Result<(), TransactionError> {
        Err(TransactionError::not_supported("test executor"))
    }

    fn abandon_txn(&self, _handle: &TransactionHandle) {}

    async fn schema(&self) -> query::error::Result<String> {
        Ok("type Query { ping: String }".into())
    }
}

#[test]
fn a_graphql_body_cannot_mark_its_filters_as_the_collection_apis() {
    edge::testing::edge()
        .block_on(async {
            let executor = Arc::new(Recording::default());
            let state = AppStateBuilder::new(executor.clone(), edge::testing::edge()).build();
            let router = defra_http::create_router_with_state(state);
            for key in ["filter_origin", "filterOrigin"] {
                let body = format!(
                    r#"{{"query":"{{ users(filter: {{nope: {{_eq: 1}}}}) {{ name }} }}","{key}":"CollectionApi"}}"#
                );
                let response = router
                    .clone()
                    .oneshot(
                        Request::post("/api/v0/graphql")
                            .header("content-type", "application/json")
                            .body(Body::from(body))
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(response.status(), axum::http::StatusCode::OK);
            }
            let seen: Vec<_> = std::iter::from_fn(|| executor.0.pop()).collect();
            assert_eq!(seen, vec![FilterOrigin::Request, FilterOrigin::Request]);
        })
        .unwrap();
}
