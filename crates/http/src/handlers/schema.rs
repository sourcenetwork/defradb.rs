//! Schema endpoint handlers.
//!
//! These handlers provide HTTP access to schema management:
//! - Add schema (POST /schema)
//!
//! All endpoints enforce NAC permissions when NAC is enabled.

use axum::{extract::State, http::HeaderMap, Json};

use crate::error::{http_error_from_backend_message, HttpError};
use crate::handlers::txn_header::txn_context;
use crate::identity_extractor::ExtractIdentity;
use crate::request_scope::bound;
use crate::router::AppState;
use schema::CollectionVersion;

/// Add a schema (Go-compatible format).
///
/// POST /api/v0/collections
///
/// Body: SDL text (text/plain)
/// Returns: Array of created CollectionVersions
///
/// The core requires `CollectionPatch` permission when NAC is enabled.
pub async fn add_collection(
    State(state): State<AppState>,
    identity: ExtractIdentity,
    headers: HeaderMap,
    body: String,
) -> Result<Json<Vec<CollectionVersion>>, HttpError> {
    if let Some(context) = txn_context(&identity, &headers)? {
        let collections = state
            .executor
            .add_collection_as(&context, &body)
            .await
            .map_err(HttpError::from)?;

        return Ok(Json(collections));
    }

    let schema_ops = bound(state.require_schema()?, &identity.context());

    let collections = schema_ops
        .add_collection(&body)
        .await
        .map_err(http_error_from_backend_message)?;

    Ok(Json(collections))
}

#[cfg(test)]
mod finished_txn_tests {
    use std::sync::Arc;

    use axum::extract::State;
    use axum::http::{HeaderMap, HeaderValue};
    use query::executor::QueryExecutor;

    use super::add_collection;
    use crate::error::HttpError;
    use crate::identity_extractor::ExtractIdentity;
    use crate::mock::FailingMockExecutor;
    use crate::router::AppStateBuilder;

    #[test]
    fn a_finished_transaction_answers_conflict() {
        let executor = Arc::new(FailingMockExecutor::with_finished_txn()) as Arc<dyn QueryExecutor>;
        let state = AppStateBuilder::new(executor, edge::testing::edge())
            .with_dev_mode(true)
            .build();
        let mut headers = HeaderMap::new();
        headers.insert("x-defradb-tx", HeaderValue::from_static("1"));
        let result = edge::testing::edge()
            .block_on(add_collection(
                State(state),
                ExtractIdentity::anonymous(),
                headers,
                "type User { name: String }".to_owned(),
            ))
            .unwrap();
        assert!(
            matches!(result, Err(HttpError::Conflict(_))),
            "{:?}",
            result.err()
        );
    }
}
