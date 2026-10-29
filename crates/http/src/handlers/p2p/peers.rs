//! P2P peer management handlers.

use axum::{
    extract::{Path, State},
    Json,
};
use serde::Serialize;

use super::{map_p2p_bad_request, map_p2p_internal};
use crate::error::peer_refused;
use crate::error::HttpError;
use crate::identity_extractor::ExtractIdentity;
use crate::nac_guard::require_permission;
use crate::request_scope::bound;
use crate::router::{peer, AppState, NodePermission};

/// Response for P2P node info.
///
/// For libp2p this returns multiaddrs with the peer id embedded.
/// For iroh this returns already-connectable peer address strings such as
/// endpoint tickets, raw endpoint ids, or `<endpoint-id>@<host>:<port>`.
pub type P2pInfoResponse = Vec<String>;

/// Response for the single best shareable P2P address.
#[derive(Debug, Clone, Serialize)]
pub struct ShareableAddressResponse {
    pub address: Option<String>,
}

/// Get P2P node information (Go-compatible format).
///
/// GET /api/v0/p2p/info
///
/// Returns connectable peer addresses for the active transport.
///
/// Requires `P2pPeerInfo` permission when NAC is enabled.
pub async fn get_info(
    State(state): State<AppState>,
    identity: ExtractIdentity,
) -> Result<Json<P2pInfoResponse>, HttpError> {
    require_permission(&state, &identity, NodePermission::P2pPeerInfo).await?;
    let Some(p2p) = &state.p2p else {
        return Ok(Json(Vec::new()));
    };
    let p2p = bound(p2p, &identity.context());
    Ok(Json(peer::peer_info(&*p2p).await.map_err(peer_refused)?))
}

/// Live sync resource diagnostics (#1099).
///
/// GET /api/v0/p2p/sync/status
///
/// Returns the `p2p::sync::SyncStatus` snapshot: push-backlog occupancy
/// (queued items/bytes, active workers, per-peer backlog and cooldowns,
/// overload counters), pending-DAG depth, and retained task handles.
///
/// Requires `P2pPeerInfo` permission when NAC is enabled.
pub async fn sync_status(
    State(state): State<AppState>,
    identity: ExtractIdentity,
) -> Result<Json<serde_json::Value>, HttpError> {
    require_permission(&state, &identity, NodePermission::P2pPeerInfo).await?;

    let p2p = bound(state.require_p2p()?, &identity.context());
    let status = p2p.sync_status().await.map_err(map_p2p_internal)?;
    Ok(Json(status))
}

/// Everything this node knows about one peer, for debugging.
///
/// GET /api/v0/p2p/peers/{peer_id}
///
/// Returns the `p2p::sync::PeerDetails` JSON plus `did` and `push_retries`;
/// the compact per-peer overview is `peers` in `/p2p/sync/status`.
///
/// Requires `P2pPeerInfo` permission when NAC is enabled.
pub async fn peer_details(
    State(state): State<AppState>,
    identity: ExtractIdentity,
    Path(peer_id): Path<String>,
) -> Result<Json<serde_json::Value>, HttpError> {
    require_permission(&state, &identity, NodePermission::P2pPeerInfo).await?;

    let peer_id = crate::router::TransportPeerId::new(peer_id).map_err(map_p2p_bad_request)?;
    let p2p = bound(state.require_p2p()?, &identity.context());
    let details = p2p.peer_details(&peer_id).await.map_err(map_p2p_internal)?;
    Ok(Json(details))
}

/// Get the single best shareable P2P address, if available.
///
/// GET /api/v0/p2p/shareable-address
///
/// Requires `P2pPeerInfo` permission when NAC is enabled.
pub async fn get_shareable_address(
    State(state): State<AppState>,
    identity: ExtractIdentity,
) -> Result<Json<ShareableAddressResponse>, HttpError> {
    require_permission(&state, &identity, NodePermission::P2pPeerInfo).await?;

    let p2p = bound(state.require_p2p()?, &identity.context());
    let address = p2p.shareable_address().await.map_err(map_p2p_internal)?;

    Ok(Json(ShareableAddressResponse { address }))
}

/// List active peers (Go-compatible).
///
/// GET /api/v0/p2p/active-peers
///
/// Returns array of connected peer IDs as strings.
///
/// Requires `P2pPeerActive` permission when NAC is enabled.
pub async fn active_peers(
    State(state): State<AppState>,
    identity: ExtractIdentity,
) -> Result<Json<Vec<String>>, HttpError> {
    require_permission(&state, &identity, NodePermission::P2pPeerActive).await?;
    let p2p = bound(state.require_p2p()?, &identity.context());
    Ok(Json(peer::active_peers(&*p2p).await.map_err(peer_refused)?))
}

/// Go's `POST /p2p/connect`.
pub async fn connect(
    State(state): State<AppState>,
    identity: ExtractIdentity,
    Json(addresses): Json<Vec<String>>,
) -> Result<(), HttpError> {
    require_permission(&state, &identity, NodePermission::P2pPeerConnect).await?;
    let p2p = bound(state.require_p2p()?, &identity.context());
    peer::connect(&*p2p, &addresses).await.map_err(peer_refused)
}

/// Go's `POST /p2p/disconnect`.
pub async fn disconnect(
    State(state): State<AppState>,
    identity: ExtractIdentity,
    Json(addresses): Json<Vec<String>>,
) -> Result<(), HttpError> {
    require_permission(&state, &identity, NodePermission::P2pPeerDisconnect).await?;
    let p2p = bound(state.require_p2p()?, &identity.context());
    peer::disconnect(&*p2p, &addresses)
        .await
        .map_err(peer_refused)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_p2p_info_response_serialize() {
        // Response is now array of full multiaddrs (Go-compatible format)
        let response: P2pInfoResponse =
            vec!["/ip4/127.0.0.1/tcp/9000/p2p/12D3KooWtest".to_string()];
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("12D3KooWtest"));
        assert!(json.contains("/ip4/127.0.0.1/tcp/9000"));
        // Verify it's an array, not an object
        assert!(json.starts_with('['));
    }

    #[test]
    fn test_shareable_address_response_serialize() {
        let response = ShareableAddressResponse {
            address: Some("endpoint-ticket".to_string()),
        };
        let json = serde_json::to_string(&response).unwrap();
        assert_eq!(json, r#"{"address":"endpoint-ticket"}"#);
    }
}
