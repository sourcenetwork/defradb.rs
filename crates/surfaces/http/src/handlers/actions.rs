use axum::{extract::State, Json};
use defra_core::ActionExecution;

use crate::error::HttpError;
use crate::identity_extractor::ExtractIdentity;
use crate::nac_guard::require_permission;
use crate::request_scope::bound;
use crate::router::{AppState, NodePermission};

/// List actions that are still running or ended with an error.
pub async fn list_actions(
    State(state): State<AppState>,
    identity: ExtractIdentity,
) -> Result<Json<Vec<ActionExecution>>, HttpError> {
    require_permission(&state, &identity, NodePermission::ActionList).await?;
    let actions = bound(state.require_collection_mgmt()?, &identity.context())
        .list_actions()
        .await
        .map_err(HttpError::Internal)?;
    Ok(Json(actions))
}
