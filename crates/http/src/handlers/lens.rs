//! Lens migration endpoint handlers.
//!
//! Each calls the core through the executor, which checks NAC.
//!
//! Security: File-path-based WASM loading is rejected via HTTP unless
//! dev_mode is enabled. Only inline module bytes are allowed.

use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use serde::Serialize;

use crate::error::HttpError;
use crate::handlers::txn_header::call_context;
use crate::identity_extractor::ExtractIdentity;
use crate::router::AppState;

/// The ID of a stored lens.
#[derive(Debug, Serialize)]
pub struct SetMigrationResponse {
    #[serde(rename = "lensId")]
    pub lens_id: String,
}

/// A lens configuration from the wire; outside dev mode it may not name a
/// file on the server's disk.
fn lens_config(state: &AppState, body: &str) -> Result<lens::LensConfig, HttpError> {
    let config: lens::LensConfig = serde_json::from_str(body)
        .map_err(|e| HttpError::BadRequest(format!("invalid lens config JSON: {e}")))?;
    if !state.dev_mode {
        config
            .validate_for_http()
            .map_err(|e| HttpError::BadRequest(e.to_string()))?;
    }
    Ok(config)
}

/// Set the migration between two collection versions: Go's `LensConfig`.
///
/// POST /api/v0/collections/migrations
pub async fn set_migration(
    State(state): State<AppState>,
    identity: ExtractIdentity,
    headers: HeaderMap,
    body: String,
) -> Result<Json<SetMigrationResponse>, HttpError> {
    let context = call_context(&identity, &headers)?;
    lens_config(&state, &body)?;
    let lens_id = state.executor.set_migration_as(&context, &body).await?;
    Ok(Json(SetMigrationResponse { lens_id }))
}

/// Go's request body for storing a lens.
#[derive(Debug, serde::Deserialize)]
pub struct AddLensRequest {
    pub lens: serde_json::Value,
}

/// Store a lens: `{"lens": <Go's model.Lens>}`.
///
/// POST /api/v0/lens
pub async fn add_lens(
    State(state): State<AppState>,
    identity: ExtractIdentity,
    headers: HeaderMap,
    Json(body): Json<AddLensRequest>,
) -> Result<Json<SetMigrationResponse>, HttpError> {
    let context = call_context(&identity, &headers)?;
    let config = lens_config(&state, &body.lens.to_string())?;
    let lens_id = state.executor.add_lens_as(&context, config).await?;
    Ok(Json(SetMigrationResponse { lens_id }))
}

/// Every stored lens, by content ID: `{"lenses": {...}}`.
///
/// GET /api/v0/lens
pub async fn list_lenses(
    State(state): State<AppState>,
    identity: ExtractIdentity,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, HttpError> {
    let context = call_context(&identity, &headers)?;
    let listed = state.executor.list_lenses_as(&context).await?;
    let lenses: serde_json::Value =
        serde_json::from_str(&listed).map_err(|e| HttpError::Internal(e.to_string()))?;
    Ok(Json(serde_json::json!({ "lenses": lenses })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::State;
    use axum::http::{HeaderMap, HeaderValue};
    use query::executor::QueryExecutor;
    use std::sync::Arc;

    use crate::handlers::graphql::TX_HEADER_NAME;
    use crate::identity_extractor::ExtractIdentity;
    use crate::mock::MockQueryExecutor;
    use crate::router::AppStateBuilder;

    #[test]
    fn test_set_migration_response_serialize() {
        let response = SetMigrationResponse {
            lens_id: "lens-123".to_string(),
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("lensId"));
        assert!(json.contains("lens-123"));
    }

    #[test]
    fn set_migration_uses_tx_header_without_global_lens_ops() {
        workers::block_on(async {
        let state = AppStateBuilder::new(
            Arc::new(MockQueryExecutor::new()) as Arc<dyn QueryExecutor>,
            edge::testing::edge(),
        )
        .build();
        let mut headers = HeaderMap::new();
        headers.insert(TX_HEADER_NAME, HeaderValue::from_static("123"));

        let response = set_migration(
            State(state),
            ExtractIdentity::anonymous(),
            headers,
            r#"{"SourceCollectionVersionID":"v1","DestinationCollectionVersionID":"v2","Lenses":[]}"#
                .to_string(),
        )
        .await
        .expect("set migration via transaction header should succeed");

        assert_eq!(response.0.lens_id, "mock-transform-id");
        })
        .unwrap()
    }
}

#[cfg(test)]
mod finished_txn_tests {
    use std::sync::Arc;

    use axum::extract::State;
    use axum::http::{HeaderMap, HeaderValue};
    use query::executor::QueryExecutor;

    use super::set_migration;
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
            .block_on(set_migration(
                State(state),
                ExtractIdentity::anonymous(),
                headers,
                "{}".to_owned(),
            ))
            .unwrap();
        assert!(
            matches!(result, Err(HttpError::Conflict(_))),
            "{:?}",
            result.err()
        );
    }
}
