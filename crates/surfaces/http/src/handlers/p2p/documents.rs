//! P2P document replication handlers.

use axum::{extract::State, Json};

use super::map_p2p_internal;
use crate::error::{peer_refused, HttpError};
use crate::identity_extractor::ExtractIdentity;
use crate::nac_guard::require_permission;
use crate::request_scope::bound;
use crate::router::{peer, AppState, NodePermission, SyncDocumentsRequest};

/// List P2P documents (Go-compatible).
///
/// GET /api/v0/p2p/documents
///
/// Go DefraDB returns flat array of document IDs: `["doc-id-1", "doc-id-2"]`
///
/// Requires `P2pDocumentList` permission when NAC is enabled.
pub async fn list_documents(
    State(state): State<AppState>,
    identity: ExtractIdentity,
) -> Result<Json<Vec<String>>, HttpError> {
    require_permission(&state, &identity, NodePermission::P2pDocumentList).await?;
    let p2p = bound(state.require_p2p()?, &identity.context());
    Ok(Json(
        peer::list_p2p_documents(&*p2p)
            .await
            .map_err(peer_refused)?,
    ))
}

/// Go's `POST /p2p/documents`.
pub async fn add_documents(
    State(state): State<AppState>,
    identity: ExtractIdentity,
    Json(doc_ids): Json<Vec<String>>,
) -> Result<(), HttpError> {
    require_permission(&state, &identity, NodePermission::P2pDocumentAdd).await?;
    let p2p = bound(state.require_p2p()?, &identity.context());
    peer::add_p2p_documents(&*p2p, &doc_ids)
        .await
        .map_err(peer_refused)
}

/// Go's `DELETE /p2p/documents`.
pub async fn remove_documents(
    State(state): State<AppState>,
    identity: ExtractIdentity,
    Json(doc_ids): Json<Vec<String>>,
) -> Result<(), HttpError> {
    require_permission(&state, &identity, NodePermission::P2pDocumentDelete).await?;
    let p2p = bound(state.require_p2p()?, &identity.context());
    peer::delete_p2p_documents(&*p2p, &doc_ids)
        .await
        .map_err(peer_refused)
}

/// Sync specific documents from connected peers.
///
/// POST /api/v0/p2p/documents/sync
///
/// Accepts JSON body: `{"collectionName": "...", "docIDs": ["..."]}`
///
/// Requires `P2pSyncDocuments` permission when NAC is enabled.
pub async fn sync_documents(
    State(state): State<AppState>,
    identity: ExtractIdentity,
    Json(body): Json<SyncDocumentsRequest>,
) -> Result<Json<()>, HttpError> {
    require_permission(&state, &identity, NodePermission::P2pSyncDocuments).await?;

    let p2p = bound(state.require_p2p()?, &identity.context());

    let timeout = crate::validation::parse_timeout(body.timeout.as_deref())?;

    p2p.sync_documents(&body.collection_name, body.doc_ids, timeout)
        .await
        .map_err(map_p2p_internal)?;

    Ok(Json(()))
}
