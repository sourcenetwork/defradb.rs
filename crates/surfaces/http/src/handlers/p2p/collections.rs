//! P2P collection management handlers.

use axum::{extract::State, Json};

use super::map_p2p_internal;
use crate::error::{peer_refused, HttpError};
use crate::identity_extractor::ExtractIdentity;
use crate::nac_guard::require_permission;
use crate::request_scope::bound;
use crate::router::{peer, AppState, NodePermission, SyncBranchableRequest, SyncVersionsRequest};

/// List P2P collections.
///
/// GET /api/v0/p2p/collections
///
/// Requires `P2pCollectionList` permission when NAC is enabled.
pub async fn list_collections(
    State(state): State<AppState>,
    identity: ExtractIdentity,
) -> Result<Json<Vec<String>>, HttpError> {
    require_permission(&state, &identity, NodePermission::P2pCollectionList).await?;
    let p2p = bound(state.require_p2p()?, &identity.context());
    Ok(Json(
        peer::list_p2p_collections(&*p2p)
            .await
            .map_err(peer_refused)?,
    ))
}

/// Go's `POST /p2p/collections`.
pub async fn add_collections(
    State(state): State<AppState>,
    identity: ExtractIdentity,
    Json(collections): Json<Vec<String>>,
) -> Result<(), HttpError> {
    require_permission(&state, &identity, NodePermission::P2pCollectionAdd).await?;
    let p2p = bound(state.require_p2p()?, &identity.context());
    peer::add_p2p_collections(&*p2p, &collections)
        .await
        .map_err(peer_refused)
}

/// Go's `DELETE /p2p/collections`.
pub async fn remove_collections(
    State(state): State<AppState>,
    identity: ExtractIdentity,
    Json(collections): Json<Vec<String>>,
) -> Result<(), HttpError> {
    require_permission(&state, &identity, NodePermission::P2pCollectionDelete).await?;
    let p2p = bound(state.require_p2p()?, &identity.context());
    peer::delete_p2p_collections(&*p2p, &collections)
        .await
        .map_err(peer_refused)
}

/// Sync a branchable collection from connected peers.
///
/// POST /api/v0/p2p/collections/sync-branchable
///
/// Accepts JSON body: `{"collectionID": "..."}`
///
/// Requires `P2pSyncBranchableCollection` permission when NAC is enabled.
pub async fn sync_branchable(
    State(state): State<AppState>,
    identity: ExtractIdentity,
    Json(body): Json<SyncBranchableRequest>,
) -> Result<Json<()>, HttpError> {
    require_permission(
        &state,
        &identity,
        NodePermission::P2pSyncBranchableCollection,
    )
    .await?;

    let p2p = bound(state.require_p2p()?, &identity.context());

    p2p.sync_branchable_collection(&body.collection_id)
        .await
        .map_err(map_p2p_internal)?;

    Ok(Json(()))
}

/// Sync collection versions (schema definitions) from connected peers via Bitswap.
///
/// POST /api/v0/p2p/collections/sync-versions
///
/// Accepts JSON body: `{"versionIDs": ["bafyrei...", ...]}`
///
/// Requires `P2pSyncCollectionVersions` permission when NAC is enabled.
pub async fn sync_versions(
    State(state): State<AppState>,
    identity: ExtractIdentity,
    Json(body): Json<SyncVersionsRequest>,
) -> Result<Json<()>, HttpError> {
    require_permission(&state, &identity, NodePermission::P2pSyncCollectionVersions).await?;

    let p2p = bound(state.require_p2p()?, &identity.context());

    p2p.sync_collection_versions(body.version_ids)
        .await
        .map_err(map_p2p_internal)?;

    Ok(Json(()))
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_collections_array_deserialize() {
        // Go-compatible format: raw array
        let json = r#"["Users", "Posts"]"#;
        let collections: Vec<String> = serde_json::from_str(json).unwrap();
        assert_eq!(collections.len(), 2);
    }
}
