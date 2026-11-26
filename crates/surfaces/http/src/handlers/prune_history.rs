//! Pruning a collection's history to its `@retention(maxDepth: N)`.

use axum::extract::{Path, State};
use axum::Json;
use serde::Serialize;

use crate::error::{http_error_from_backend_message, HttpError};
use crate::identity_extractor::ExtractIdentity;
use crate::nac_guard::require_permission;
use crate::request_scope::bound;
use crate::router::{AppState, NodePermission};

/// How many commits a history prune removed.
#[derive(Serialize)]
pub struct PruneHistoryResponse {
    pub count: usize,
}

/// Prune each document's history in the collection to the collection's
/// `@retention(maxDepth: N)`. Checked against the node permission of a
/// collection truncate.
///
/// POST /api/v0/collections/{name}/history/prune
pub async fn prune_history(
    State(state): State<AppState>,
    identity: ExtractIdentity,
    Path(name): Path<String>,
) -> Result<Json<PruneHistoryResponse>, HttpError> {
    require_permission(&state, &identity, NodePermission::CollectionTruncate).await?;
    let collection_mgmt = bound(state.require_collection_mgmt()?, &identity.context());
    let count = collection_mgmt
        .prune_history(&name)
        .await
        .map_err(http_error_from_backend_message)?;
    Ok(Json(PruneHistoryResponse { count }))
}
