//! Collection REST endpoint handlers.
//!
//! These handlers extract identity from the Authorization header and pass it
//! to the REST operations layer for ACP (Access Control Policy) enforcement.
//!
//! All endpoints enforce NAC permissions when NAC is enabled.

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use query::rest::{CollectionDocIdsPage, CollectionDocIdsPagination};
use rapidhash::RapidHashMap;
use serde::{Deserialize, Serialize};

use crate::error::{http_error_from_backend_message, HttpError};
use crate::handlers::collection_selector::CollectionSelectorQuery;
use crate::handlers::txn_header::{call_context, rest_for_request};
use crate::identity_extractor::ExtractIdentity;
use crate::nac_guard::require_permission;
use crate::request_scope::bound;
use crate::router::{AppState, NodePermission};

const DEFAULT_DOC_IDS_LIMIT: usize = 100;
const MAX_DOC_IDS_LIMIT: usize = 1000;

/// Response for listing document IDs in a collection.
#[derive(Debug, Clone, Serialize)]
pub struct CollectionDocIdsResponse {
    pub doc_ids: Vec<String>,
    pub total: usize,
    pub has_more: bool,
    pub offset: usize,
    pub limit: usize,
}

impl From<CollectionDocIdsPage> for CollectionDocIdsResponse {
    fn from(page: CollectionDocIdsPage) -> Self {
        Self {
            has_more: page.has_more(),
            doc_ids: page.doc_ids,
            total: page.total,
            offset: page.offset,
            limit: page.limit,
        }
    }
}

fn parse_collection_doc_ids_pagination(
    params: &RapidHashMap<String, String>,
) -> Result<CollectionDocIdsPagination, HttpError> {
    let limit = match params.get("limit") {
        Some(raw) => {
            let limit = raw.parse::<usize>().map_err(|_| {
                HttpError::BadRequest(format!("'limit' must be a positive integer, got '{}'", raw))
            })?;
            if limit == 0 || limit > MAX_DOC_IDS_LIMIT {
                return Err(HttpError::BadRequest(format!(
                    "'limit' must be between 1 and {}",
                    MAX_DOC_IDS_LIMIT
                )));
            }
            limit
        }
        None => DEFAULT_DOC_IDS_LIMIT,
    };

    let offset = match params.get("offset") {
        Some(raw) => raw.parse::<usize>().map_err(|_| {
            HttpError::BadRequest(format!(
                "'offset' must be a non-negative integer, got '{}'",
                raw
            ))
        })?,
        None => 0,
    };

    Ok(CollectionDocIdsPagination { limit, offset })
}

/// The collection versions Go's selectors pick.
///
/// GET /api/v0/collections?name=Users&version_id=..&collection_id=..&get_inactive=true
pub async fn list_collections(
    State(state): State<AppState>,
    identity: ExtractIdentity,
    headers: HeaderMap,
    query: CollectionSelectorQuery,
) -> Result<Json<Vec<schema::CollectionVersion>>, HttpError> {
    let context = call_context(&identity, &headers)?;
    let collections = state
        .executor
        .get_collections_as(&context, &query.into_selector())
        .await?;
    Ok(Json(collections))
}

/// Get document IDs in a collection.
///
/// GET /api/v0/collections/{name}?limit=100&offset=0
///
/// Returns a bounded page of document IDs and pagination metadata.
///
/// Requires `CollectionGet` permission when NAC is enabled.
pub async fn get_collection_doc_ids(
    State(state): State<AppState>,
    identity: ExtractIdentity,
    headers: HeaderMap,
    Path(name): Path<String>,
    Query(params): Query<RapidHashMap<String, String>>,
) -> Result<Json<CollectionDocIdsResponse>, HttpError> {
    require_permission(&state, &identity, NodePermission::CollectionGet).await?;

    let pagination = parse_collection_doc_ids_pagination(&params)?;
    let rest = rest_for_request(&state, &identity, &headers)?;

    match rest
        .get_collection_doc_ids_page(&name, pagination, identity.did())
        .await
    {
        Ok(page) => Ok(Json(page.into())),
        Err(e) => {
            tracing::warn!(collection = %name, error = %e, "Failed to list collection document IDs");
            Err(e.into())
        }
    }
}

/// Go's request body for patching the collections: the patch, as its text
/// or as the array itself, and the lens that migrates to the new versions.
#[derive(Debug, serde::Deserialize)]
pub struct PatchCollectionRequest {
    #[serde(rename = "Patch")]
    pub patch: serde_json::Value,
    #[serde(rename = "Migration", default)]
    pub migration: Option<lens::LensConfig>,
}

/// Patch the collections.
///
/// PATCH /api/v0/collections
pub async fn patch_collection(
    State(state): State<AppState>,
    identity: ExtractIdentity,
    headers: HeaderMap,
    Json(body): Json<PatchCollectionRequest>,
) -> Result<StatusCode, HttpError> {
    let context = call_context(&identity, &headers)?;
    let patch = match body.patch {
        serde_json::Value::String(text) => text,
        ops => ops.to_string(),
    };
    if !state.dev_mode {
        if let Some(migration) = &body.migration {
            migration
                .validate_for_http()
                .map_err(|e| HttpError::BadRequest(e.to_string()))?;
        }
    }
    state
        .executor
        .patch_collection_as(&context, &patch, body.migration)
        .await?;
    Ok(StatusCode::OK)
}

/// Set the active collection version: the body is its version ID.
///
/// POST /api/v0/collections/default
pub async fn set_active(
    State(state): State<AppState>,
    identity: ExtractIdentity,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<StatusCode, HttpError> {
    let context = call_context(&identity, &headers)?;
    let version_id = String::from_utf8(body.to_vec())
        .map_err(|_| HttpError::BadRequest("invalid UTF-8".into()))?;
    state
        .executor
        .set_active_collection_version_as(&context, version_id.trim())
        .await?;
    Ok(StatusCode::OK)
}

#[derive(Deserialize)]
struct TruncateCollectionRequest {
    filter: Option<serde_json::Value>,
}

/// Truncate all or matching documents in a collection.
///
/// DELETE /api/v0/collections/{name}/truncate
///
/// Deletes all documents, heads, blocks, and index entries while
/// preserving the collection schema.
///
/// Requires `CollectionTruncate` permission when NAC is enabled.
pub async fn truncate_collection(
    State(state): State<AppState>,
    identity: ExtractIdentity,
    Path(name): Path<String>,
    body: axum::body::Bytes,
) -> Result<Json<()>, HttpError> {
    require_permission(&state, &identity, NodePermission::CollectionTruncate).await?;

    let collection_mgmt = bound(state.require_collection_mgmt()?, &identity.context());

    let filter = if body.is_empty() {
        None
    } else {
        let request: TruncateCollectionRequest = serde_json::from_slice(&body)
            .map_err(|error| HttpError::BadRequest(format!("invalid request body: {error}")))?;
        match request.filter {
            Some(serde_json::Value::Object(filter)) => Some(serde_json::Value::Object(filter)),
            Some(serde_json::Value::Null) | None => {
                return Err(HttpError::BadRequest("filter is required".into()))
            }
            Some(_) => return Err(HttpError::BadRequest("filter must be an object".into())),
        }
    };

    collection_mgmt
        .truncate_collection(&name, filter)
        .await
        .map_err(http_error_from_backend_message)?;

    Ok(Json(()))
}

/// Describe a collection by name.
///
/// GET /api/v0/collections/{name}/describe
///
/// Returns the CollectionVersion JSON for the named collection, or 404.
///
/// Requires `CollectionGet` permission when NAC is enabled.
pub async fn describe_collection(
    State(state): State<AppState>,
    identity: ExtractIdentity,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, HttpError> {
    require_permission(&state, &identity, NodePermission::CollectionGet).await?;

    let collection_mgmt = bound(state.require_collection_mgmt()?, &identity.context());

    match collection_mgmt.get_collection_by_name(&name).await {
        Ok(Some(cv)) => {
            let val = serde_json::to_value(&cv)
                .map_err(|e| HttpError::Internal(format!("serialization error: {}", e)))?;
            Ok(Json(val))
        }
        Ok(None) => Err(HttpError::NotFound(format!(
            "collection '{}' not found",
            name
        ))),
        Err(e) => Err(http_error_from_backend_message(e)),
    }
}

/// Check if a collection exists.
///
/// GET /api/v0/collections/{name}/exists
///
/// Returns `{"exists": true/false}`.
///
/// Requires `CollectionGet` permission when NAC is enabled.
pub async fn collection_exists(
    State(state): State<AppState>,
    identity: ExtractIdentity,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, HttpError> {
    require_permission(&state, &identity, NodePermission::CollectionGet).await?;

    let collection_mgmt = bound(state.require_collection_mgmt()?, &identity.context());

    let exists = collection_mgmt
        .has_collection(&name)
        .await
        .map_err(http_error_from_backend_message)?;

    Ok(Json(serde_json::json!({ "exists": exists })))
}

/// Delete multiple collection versions.
///
/// DELETE /api/v0/collections/versions
///
/// Body: JSON array of version ID strings.
///
/// Requires `CollectionPatch` permission when NAC is enabled.
pub async fn delete_collection_versions(
    State(state): State<AppState>,
    identity: ExtractIdentity,
    Json(version_ids): Json<Vec<String>>,
) -> Result<Json<serde_json::Value>, HttpError> {
    require_permission(&state, &identity, NodePermission::CollectionPatch).await?;

    let collection_mgmt = bound(state.require_collection_mgmt()?, &identity.context());

    collection_mgmt
        .delete_collection_versions(version_ids)
        .await
        .map_err(http_error_from_backend_message)?;

    Ok(Json(serde_json::json!({})))
}

/// Delete one or more collections by name (Go #4688 parity).
///
/// DELETE /api/v0/collections?name=Users,Books&active-only=true
///
/// Query parameters:
/// - `name` (required): comma-separated list of collection names.
/// - `active-only` (optional, default false): if true, deletes only the active
///   head version of each named collection; if false, deletes every version.
///
/// Requires `CollectionPatch` permission when NAC is enabled.
pub async fn delete_collections_by_names(
    State(state): State<AppState>,
    identity: ExtractIdentity,
    Query(params): Query<RapidHashMap<String, String>>,
) -> Result<StatusCode, HttpError> {
    require_permission(&state, &identity, NodePermission::CollectionPatch).await?;

    let raw_names = params
        .get("name")
        .ok_or_else(|| HttpError::BadRequest("missing required 'name' query parameter".into()))?;

    let names: Vec<String> = raw_names
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    if names.is_empty() {
        return Err(HttpError::BadRequest(
            "'name' query parameter must contain at least one non-empty name".into(),
        ));
    }

    let active_only = match params.get("active-only") {
        Some(raw) => raw.parse::<bool>().map_err(|_| {
            HttpError::BadRequest(format!(
                "'active-only' must be true or false, got '{}'",
                raw
            ))
        })?,
        None => false,
    };

    let collection_mgmt = bound(state.require_collection_mgmt()?, &identity.context());
    collection_mgmt
        .delete_collections(names, active_only)
        .await
        .map_err(http_error_from_backend_message)?;

    Ok(StatusCode::OK)
}

#[cfg(test)]
#[path = "collections_tests.rs"]
mod tests;
