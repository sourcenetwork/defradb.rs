//! NAC (Node Access Control) handlers.
//!
//! Go's routes read the request and call the core, which checks the caller's
//! node permissions and owns every error: status, disable, re-enable, and the
//! actor relationships. `enable` and the batch relationship route are Rust
//! only.

use axum::{extract::State, response::IntoResponse, Json};

use crate::error::{http_error_from_backend_message, HttpError};
use crate::identity_extractor::ExtractIdentity;
use crate::router::AppState;

/// Request body for Go-compatible NAC relationship operations.
/// Go DefraDB uses: `{"Relation": "admin", "TargetActor": "did:key:..."}`
#[derive(Debug, serde::Deserialize)]
pub struct GoNacRelationshipRequest {
    /// Relation type (e.g., "admin").
    #[serde(rename = "Relation")]
    pub relation: String,
    /// Target actor DID.
    #[serde(rename = "TargetActor")]
    pub target_actor: String,
}

/// Go's `client.AddActorRelationshipResult`.
#[derive(Debug, serde::Serialize)]
pub struct AddActorRelationshipResult {
    #[serde(rename = "ExistedAlready")]
    pub existed_already: bool,
}

/// Go's `client.DeleteActorRelationshipResult`.
#[derive(Debug, serde::Serialize)]
pub struct DeleteActorRelationshipResult {
    #[serde(rename = "RecordFound")]
    pub record_found: bool,
}

/// Go's `client.NACStatusResult`.
#[derive(Debug, serde::Serialize)]
pub struct NacStatusResult {
    #[serde(rename = "Status")]
    pub status: String,
}

/// Request body for enabling NAC (Go-compatible format).
#[derive(Debug, serde::Deserialize)]
pub struct EnableNacRequest {
    /// The owner DID to initialize NAC with.
    #[serde(rename = "OwnerDID")]
    pub owner_did: String,
}

/// POST /api/v0/acp/node/enable
///
/// Enable NAC with the given owner identity.
/// Go DefraDB format: `{"OwnerDID": "did:key:..."}`
///
/// The caller must authenticate as the owner DID being registered.
/// This prevents unauthorized parties from hijacking NAC initialization.
pub async fn enable(
    State(state): State<AppState>,
    identity: ExtractIdentity,
    Json(body): Json<EnableNacRequest>,
) -> Result<impl IntoResponse, HttpError> {
    let nac = state.require_nac()?;

    let owner = identity::Did::new(&body.owner_did)
        .map_err(|e| HttpError::BadRequest(format!("invalid OwnerDID: {}", e)))?;

    let caller = identity
        .did()
        .cloned()
        .ok_or_else(|| HttpError::Forbidden("authentication required to enable NAC".into()))?;

    if caller != owner {
        tracing::warn!(
            caller = %caller,
            owner = %owner,
            "NAC enable rejected: caller identity does not match OwnerDID"
        );
        return Err(HttpError::Forbidden(
            "caller identity must match OwnerDID".into(),
        ));
    }

    nac.enable(&owner).await.map_err(|e| {
        tracing::warn!(error = %e, "NAC enable operation failed");
        http_error_from_backend_message(e)
    })?;

    Ok(axum::http::StatusCode::OK.into_response())
}
/// GET /api/v0/acp/node/status. Go's `acpHandler.GetNACStatus`.
pub async fn get_status(
    State(state): State<AppState>,
    identity: ExtractIdentity,
) -> Result<Json<NacStatusResult>, HttpError> {
    let status = state.executor.nac_status_as(&identity.context()).await?;
    Ok(Json(NacStatusResult {
        status: status.to_string(),
    }))
}

/// POST /api/v0/acp/node/relationship. Go's
/// `acpHandler.AddNACActorRelationship`.
pub async fn go_add_relationship(
    State(state): State<AppState>,
    identity: ExtractIdentity,
    Json(body): Json<GoNacRelationshipRequest>,
) -> Result<Json<AddActorRelationshipResult>, HttpError> {
    Ok(Json(add_relationship(&state, &identity, &body).await?))
}

/// POST /api/v0/acp/node/relationships
///
/// Add several NAC relationships in one request, in order. Every entry is
/// validated before any write runs; a later backend error stops the request
/// without undoing the entries before it.
pub async fn go_add_relationships(
    State(state): State<AppState>,
    identity: ExtractIdentity,
    Json(bodies): Json<Vec<GoNacRelationshipRequest>>,
) -> Result<Json<Vec<AddActorRelationshipResult>>, HttpError> {
    let targets = bodies
        .iter()
        .map(validate_add_relationship)
        .collect::<Result<Vec<_>, _>>()?;
    let mut results = Vec::with_capacity(bodies.len());
    for (body, _target) in bodies.iter().zip(&targets) {
        results.push(add_relationship(&state, &identity, body).await?);
    }
    Ok(Json(results))
}

/// Validate one add-relationship entry: the relation must be a NAC relation
/// other than "owner", and the target must parse as a DID.
fn validate_add_relationship(body: &GoNacRelationshipRequest) -> Result<identity::Did, HttpError> {
    if !acp::is_valid_nac_relation(&body.relation) || body.relation == "owner" {
        return Err(HttpError::BadRequest(
            "relation not in resource".to_string(),
        ));
    }
    if body.target_actor.is_empty() {
        return Err(HttpError::BadRequest("actor must be a valid did".into()));
    }
    identity::Did::new(&body.target_actor)
        .map_err(|e| HttpError::BadRequest(format!("invalid TargetActor DID: {}", e)))
}

async fn add_relationship(
    state: &AppState,
    identity: &ExtractIdentity,
    body: &GoNacRelationshipRequest,
) -> Result<AddActorRelationshipResult, HttpError> {
    let existed_already = state
        .executor
        .add_nac_actor_relationship_as(&identity.context(), &body.relation, &body.target_actor)
        .await?;
    Ok(AddActorRelationshipResult { existed_already })
}

/// DELETE /api/v0/acp/node/relationship. Go's
/// `acpHandler.DeleteNACActorRelationship`.
pub async fn go_remove_relationship(
    State(state): State<AppState>,
    identity: ExtractIdentity,
    Json(body): Json<GoNacRelationshipRequest>,
) -> Result<Json<DeleteActorRelationshipResult>, HttpError> {
    let record_found = state
        .executor
        .delete_nac_actor_relationship_as(&identity.context(), &body.relation, &body.target_actor)
        .await?;
    Ok(Json(DeleteActorRelationshipResult { record_found }))
}

/// POST /api/v0/acp/node/disable. Go's `acpHandler.DisableNAC`.
pub async fn disable(
    State(state): State<AppState>,
    identity: ExtractIdentity,
) -> Result<Json<()>, HttpError> {
    state.executor.disable_nac_as(&identity.context()).await?;
    Ok(Json(()))
}

/// POST /api/v0/acp/node/re-enable. Go's `acpHandler.ReEnableNAC`.
pub async fn re_enable(
    State(state): State<AppState>,
    identity: ExtractIdentity,
) -> Result<Json<()>, HttpError> {
    state.executor.re_enable_nac_as(&identity.context()).await?;
    Ok(Json(()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockQueryExecutor;
    use crate::router::AppStateBuilder;
    use query::executor::QueryExecutor;
    use std::sync::Arc;

    fn relationship(relation: &str, target_actor: &str) -> GoNacRelationshipRequest {
        GoNacRelationshipRequest {
            relation: relation.into(),
            target_actor: target_actor.into(),
        }
    }

    #[nuclei::test]
    async fn add_relationships_prevalidates_and_returns_ordered_results() {
        let requestor = identity::Did::new("did:key:requestor").unwrap();
        let target = identity::Did::new("did:key:target").unwrap();
        let executor = Arc::new(MockQueryExecutor::new());
        let state = AppStateBuilder::new(
            Arc::clone(&executor) as Arc<dyn QueryExecutor>,
            edge::testing::edge(),
        )
        .build();
        let identity = ExtractIdentity::from_did(Some(requestor));

        let result = go_add_relationships(
            State(state.clone()),
            identity.clone(),
            Json(vec![
                relationship("admin", target.as_str()),
                relationship("owner", "did:key:other"),
            ]),
        )
        .await;

        assert!(matches!(result, Err(HttpError::BadRequest(_))));
        assert!(executor.nac_relationships().is_empty());

        let Json(results) = go_add_relationships(
            State(state),
            identity,
            Json(vec![
                relationship("admin", target.as_str()),
                relationship("admin", target.as_str()),
            ]),
        )
        .await
        .unwrap();

        assert_eq!(
            results
                .iter()
                .map(|result| result.existed_already)
                .collect::<Vec<_>>(),
            vec![false, true]
        );
    }
}
