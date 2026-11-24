//! Go-compatible encrypted index endpoint handlers.
//!
//! Route pattern: /api/v0/collections/{name}/encrypted-indexes

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::Deserialize;

use crate::error::{http_error_from_backend_message, HttpError};
use crate::handlers::txn_header::call_context;
use crate::identity_extractor::ExtractIdentity;
use crate::nac_guard::require_permission;
use crate::request_scope::bound;
use crate::router::{AppState, EncryptedIndexInfo, NodePermission};
use crate::validation::validate_identifier;

/// Go-compatible request to add an encrypted index.
#[derive(Debug, Deserialize)]
pub struct GoAddEncryptedIndexRequest {
    /// Field name to create the encrypted index on.
    #[serde(rename = "FieldName")]
    pub field_name: Option<String>,
}

/// Add an encrypted index (Go-compatible route).
///
/// POST /api/v0/collections/{name}/encrypted-indexes
pub async fn go_add_encrypted_index(
    State(state): State<AppState>,
    identity: ExtractIdentity,
    headers: HeaderMap,
    Path(collection): Path<String>,
    body: Option<Json<GoAddEncryptedIndexRequest>>,
) -> Result<Json<EncryptedIndexInfo>, HttpError> {
    let ops = bound(
        state.require_encrypted_index()?,
        &call_context(&identity, &headers)?,
    );

    validate_identifier(&collection).map_err(|_| {
        HttpError::BadRequest(format!(
            "invalid collection name '{}': must match [A-Za-z_][A-Za-z0-9_]*",
            collection
        ))
    })?;

    // Field name can come from body or path segment
    let field_name = body
        .and_then(|b| b.field_name.clone())
        .ok_or_else(|| HttpError::BadRequest("field_name is required".into()))?;

    let info = ops
        .add_encrypted_index(&collection, &field_name)
        .await
        .map_err(http_error_from_backend_message)?;

    Ok(Json(info))
}

/// List encrypted indexes for a collection (Go-compatible route).
///
/// GET /api/v0/collections/{name}/encrypted-indexes
pub async fn go_list_encrypted_indexes(
    State(state): State<AppState>,
    identity: ExtractIdentity,
    headers: HeaderMap,
    Path(collection): Path<String>,
) -> Result<Json<Vec<EncryptedIndexInfo>>, HttpError> {
    let ops = bound(
        state.require_encrypted_index()?,
        &call_context(&identity, &headers)?,
    );

    validate_identifier(&collection).map_err(|_| {
        HttpError::BadRequest(format!(
            "invalid collection name '{}': must match [A-Za-z_][A-Za-z0-9_]*",
            collection
        ))
    })?;

    let indexes = ops
        .list_encrypted_indexes(Some(&collection))
        .await
        .map_err(http_error_from_backend_message)?;

    Ok(Json(indexes))
}

/// List all encrypted indexes across all collections (Go-compatible route),
/// in the transaction the `x-defradb-tx` header names when given.
///
/// GET /api/v0/encrypted-indexes
pub async fn go_list_all_encrypted_indexes(
    State(state): State<AppState>,
    identity: ExtractIdentity,
    headers: HeaderMap,
) -> Result<Json<std::collections::BTreeMap<String, Vec<EncryptedIndexInfo>>>, HttpError> {
    require_permission(&state, &identity, NodePermission::EncryptedIndexListAll).await?;

    let ops = bound(
        state.require_encrypted_index()?,
        &call_context(&identity, &headers)?,
    );

    let indexes = ops
        .list_encrypted_indexes(None)
        .await
        .map_err(HttpError::Internal)?;

    // Go answers with the indexes by collection name.
    let mut by_collection = std::collections::BTreeMap::<String, Vec<EncryptedIndexInfo>>::new();
    for index in indexes {
        by_collection
            .entry(index.collection.clone())
            .or_default()
            .push(index);
    }
    Ok(Json(by_collection))
}

/// Delete an encrypted index (Go-compatible route).
///
/// DELETE /api/v0/collections/{name}/encrypted-indexes/{field}
pub async fn go_delete_encrypted_index(
    State(state): State<AppState>,
    identity: ExtractIdentity,
    headers: HeaderMap,
    Path((collection, field)): Path<(String, String)>,
) -> Result<StatusCode, HttpError> {
    let ops = bound(
        state.require_encrypted_index()?,
        &call_context(&identity, &headers)?,
    );

    validate_identifier(&collection).map_err(|_| {
        HttpError::BadRequest(format!(
            "invalid collection name '{}': must match [A-Za-z_][A-Za-z0-9_]*",
            collection
        ))
    })?;

    ops.delete_encrypted_index(&collection, &field)
        .await
        .map_err(http_error_from_backend_message)?;

    Ok(StatusCode::OK)
}
