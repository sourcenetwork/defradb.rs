//! P2P replicator management handlers.

use axum::{extract::State, Json};
use serde::Deserialize;

use defra_p2p_adapter::peer::Replicator;

use crate::error::{peer_refused, HttpError};
use crate::identity_extractor::ExtractIdentity;
use crate::nac_guard::require_permission;
use crate::request_scope::bound;
use crate::router::{
    peer, AppState, ExplicitReplayCapabilityInput, NodePermission, ReplicationFilters,
};

/// Request to add a replicator (Go-compatible format).
#[derive(Debug, Clone, Deserialize)]
pub struct ReplicatorRequest {
    /// List of collection names to replicate.
    #[serde(rename = "Collections")]
    pub collections: Vec<String>,
    /// List of peer multiaddrs to replicate to.
    #[serde(rename = "Addresses", default)]
    pub addresses: Vec<String>,
    #[serde(rename = "ExplicitReplayCapabilities", default)]
    pub explicit_replay_capabilities: Vec<ExplicitReplayCapabilityInput>,
    /// Optional per-collection filtered replication predicates.
    #[serde(rename = "Filters", default)]
    pub filters: ReplicationFilters,
}

/// Request body for replicator removal (Go-compatible).
/// Go DefraDB uses body JSON with ID and Collections fields.
#[derive(Debug, Clone, Deserialize)]
pub struct ReplicatorDeleteRequest {
    #[serde(rename = "ID", default)]
    pub id: String,
    /// Collections to stop replicating; none removes the whole replicator.
    #[serde(rename = "Collections", default)]
    pub collections: Vec<String>,
}

/// List replicators (Go-compatible format).
///
/// GET /api/v0/p2p/replicators
///
/// Returns array of replicators with Go-compatible PascalCase field names:
/// `[{"ID": "...", "Addresses": [...], "CollectionIDs": [...], "Status": 0, "LastStatusChange": "..."}]`
///
/// Requires `P2pReplicatorList` permission when NAC is enabled.
pub async fn list_replicators(
    State(state): State<AppState>,
    identity: ExtractIdentity,
) -> Result<Json<Vec<Replicator>>, HttpError> {
    require_permission(&state, &identity, NodePermission::P2pReplicatorList).await?;
    let p2p = bound(state.require_p2p()?, &identity.context());
    Ok(Json(
        peer::list_replicators(&*p2p).await.map_err(peer_refused)?,
    ))
}

/// Go's `POST /p2p/replicators`, plus the Rust-only filtered replication and
/// explicit replay capabilities.
pub async fn add_replicator(
    State(state): State<AppState>,
    identity: ExtractIdentity,
    Json(request): Json<ReplicatorRequest>,
) -> Result<Json<()>, HttpError> {
    require_permission(&state, &identity, NodePermission::P2pReplicatorAdd).await?;
    let p2p = bound(state.require_p2p()?, &identity.context());
    let extras = peer::ReplicatorExtras {
        filters: request.filters,
        explicit_replay_capabilities: request.explicit_replay_capabilities,
        expected_authorizer_did: identity.did().map(|did| did.to_string()),
    };
    peer::add_replicator(&*p2p, &request.addresses, &request.collections, extras)
        .await
        .map_err(peer_refused)?;
    Ok(Json(()))
}

/// Go's `DELETE /p2p/replicators`: without collections the whole replicator
/// goes.
pub async fn remove_replicator(
    State(state): State<AppState>,
    identity: ExtractIdentity,
    Json(request): Json<ReplicatorDeleteRequest>,
) -> Result<(), HttpError> {
    require_permission(&state, &identity, NodePermission::P2pReplicatorDelete).await?;
    let p2p = bound(state.require_p2p()?, &identity.context());
    peer::delete_replicator(&*p2p, &request.id, &request.collections)
        .await
        .map_err(peer_refused)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replicator_request_deserialize() {
        // Go-compatible format with PascalCase field names
        let json =
            r#"{"Collections": ["Users", "Posts"], "Addresses": ["/ip4/127.0.0.1/tcp/9000"]}"#;
        let request: ReplicatorRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.collections.len(), 2);
        assert_eq!(request.addresses.len(), 1);
    }

    #[test]
    fn test_replicator_request_without_address() {
        let json = r#"{"Collections": ["Users"]}"#;
        let request: ReplicatorRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.collections.len(), 1);
        assert!(request.addresses.is_empty());
    }

    #[test]
    fn test_replicator_delete_request_deserialize() {
        let json = r#"{"ID": "replicator-123", "Collections": ["Users", "Posts"]}"#;
        let request: ReplicatorDeleteRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.id, "replicator-123");
        assert_eq!(request.collections.len(), 2);
    }

    #[test]
    fn replicator_serializes_with_go_field_names() {
        let json = serde_json::to_string(&Replicator {
            id: "replicator-123".to_string(),
            addresses: vec!["/ip4/127.0.0.1/tcp/9000".to_string()],
            collection_ids: vec!["Users".to_string()],
            status: 0,
            last_status_change: "0001-01-01T00:00:00Z".to_string(),
            filters: Default::default(),
        })
        .unwrap();
        assert_eq!(
            json,
            r#"{"ID":"replicator-123","Addresses":["/ip4/127.0.0.1/tcp/9000"],"CollectionIDs":["Users"],"Status":0,"LastStatusChange":"0001-01-01T00:00:00Z"}"#
        );
    }
}
