//! ACP (Access Control Policy) endpoint handlers.
//!
//! The policy handlers read the request and call the core, which checks the
//! caller's node permissions and owns every error:
//! - Add policy (Go's route; the raw policy text is the body, as Go sends it)
//! - List policies, get a policy by ID, the light client status (Rust only)

use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::error::{http_error_from_backend_message, HttpError};
use crate::identity_extractor::ExtractIdentity;
use crate::nac_guard::require_permission;
use crate::request_scope::bound;
use crate::router::{AppState, NodePermission};

/// Response for adding a policy.
#[derive(Debug, Clone, Serialize)]
pub struct AddPolicyResponse {
    #[serde(rename = "PolicyID")]
    pub policy_id: String,
}

/// A policy as the list and get routes answer it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyInfo {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<serde_json::Value>,
}

impl From<&acp::Policy> for PolicyInfo {
    fn from(policy: &acp::Policy) -> Self {
        Self {
            id: policy.id.clone(),
            name: Some(policy.name.clone()),
            description: policy.attributes.get("description").cloned(),
            resources: serde_json::to_value(&policy.resources).ok(),
        }
    }
}

/// Add a DAC policy. Go's `acpHandler.AddDACPolicy`.
///
/// POST /api/v1/acp/document/policy
pub async fn add_policy(
    State(state): State<AppState>,
    identity: ExtractIdentity,
    body: String,
) -> Result<Json<AddPolicyResponse>, HttpError> {
    let policy_id = state
        .executor
        .add_dac_policy_as(&identity.context(), &body)
        .await?;
    Ok(Json(AddPolicyResponse { policy_id }))
}

/// List every DAC policy the node holds.
///
/// GET /api/v0/acp/policy
pub async fn list_policies(
    State(state): State<AppState>,
    identity: ExtractIdentity,
) -> Result<Json<Vec<PolicyInfo>>, HttpError> {
    let policies = state
        .executor
        .list_dac_policies_as(&identity.context())
        .await?;
    Ok(Json(policies.iter().map(PolicyInfo::from).collect()))
}

/// Get a DAC policy by ID.
///
/// GET /api/v0/acp/policy/:id
pub async fn get_policy(
    State(state): State<AppState>,
    identity: ExtractIdentity,
    Path(id): Path<String>,
) -> Result<Json<PolicyInfo>, HttpError> {
    let policy = state
        .executor
        .get_dac_policy_as(&identity.context(), &id)
        .await?
        .ok_or_else(|| HttpError::NotFound(format!("Policy '{}' not found", id)))?;
    Ok(Json(PolicyInfo::from(&policy)))
}

/// Get the remote ACP's light client status.
///
/// GET /api/v0/acp/status
pub async fn get_status(
    State(state): State<AppState>,
    identity: ExtractIdentity,
) -> Result<Json<acp::LightClientStatus>, HttpError> {
    let status = state
        .executor
        .acp_light_client_status_as(&identity.context())
        .await?;
    Ok(Json(status))
}

/// Request body for document ACP relationship operations (Go-compatible).
#[derive(Debug, Clone, Deserialize)]
pub struct DocRelationshipRequest {
    #[serde(alias = "collection")]
    pub collection: String,
    #[serde(alias = "docID")]
    pub doc_id: String,
    #[serde(alias = "relation")]
    pub relation: String,
    #[serde(alias = "actor")]
    pub target_actor: String,
}

/// Response for document ACP relationship operations (Go-compatible).
#[derive(Debug, Clone, Serialize)]
pub struct DocRelationshipResponse {
    #[serde(rename = "ExistedAlready")]
    pub existed_already: bool,
}

fn relationship_operations(
    state: &AppState,
    context: &defra_core::RequestContext,
) -> Result<std::sync::Arc<dyn crate::router::DocumentAcpOperations>, HttpError> {
    let operations = state.require_doc_acp()?;
    if context.txn().is_none() {
        return Ok(bound(operations, context));
    }
    let bound = operations.bind_request(context).ok_or_else(|| {
        HttpError::NotImplemented("transaction-bound document relationships are unavailable".into())
    })?;
    if !bound.supports_transactional_relationships() {
        return Err(HttpError::NotImplemented(
            "transaction-bound document relationships are unavailable".into(),
        ));
    }
    Ok(bound)
}

/// Known documents whose ownership the authenticated caller asks about.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnDocumentsRequest {
    pub collection: String,
    pub doc_ids: Vec<String>,
}

/// A visible document's ownership relative to the authenticated caller.
#[derive(Debug, Serialize)]
pub struct OwnDocument {
    pub doc_id: String,
    pub visible: bool,
    pub is_owner: bool,
    /// Ownership presence is disclosed only for an ordinarily readable document.
    pub owner_registered: bool,
}

#[derive(Debug, Serialize)]
pub struct OwnDocumentsResponse {
    pub documents: Vec<OwnDocument>,
}

/// Observe only the caller's ownership, using the same transaction and read
/// authorization as the selected documents.
pub async fn own_documents(
    State(state): State<AppState>,
    identity: ExtractIdentity,
    headers: HeaderMap,
    Json(body): Json<OwnDocumentsRequest>,
) -> Result<Json<OwnDocumentsResponse>, HttpError> {
    let actor = identity
        .did()
        .ok_or_else(|| HttpError::Unauthorized("authentication required".into()))?;
    let context = super::txn_header::txn_context(&identity, &headers)?
        .ok_or_else(|| HttpError::BadRequest("x-defradb-tx is required".into()))?
        .with_dac_bypass(false);
    if !context.authorization_valid_now() || context.identity() != Some(actor.as_str()) {
        return Err(HttpError::Unauthorized(
            "request authorization expired".into(),
        ));
    }
    let handle =
        query::TransactionHandle::new(context.txn().expect("transaction is present").to_string());
    let rows = state
        .executor
        .observe_document_owners_in_txn(
            context,
            &handle,
            &body.collection,
            &body.doc_ids,
            Some(actor.clone()),
        )
        .await
        .map_err(|error| HttpError::QueryExecution(error.to_string()))?;
    Ok(Json(OwnDocumentsResponse {
        documents: rows
            .into_iter()
            .map(|row| OwnDocument {
                doc_id: row.doc_id,
                visible: row.visible,
                is_owner: row.visible && row.owner.as_ref() == Some(actor),
                owner_registered: row.visible && row.owner.is_some(),
            })
            .collect(),
    }))
}

fn validate_add_doc_relationship(body: &DocRelationshipRequest) -> Result<(), HttpError> {
    if body.collection.is_empty() {
        return Err(HttpError::BadRequest(
            "collection name can't be empty".into(),
        ));
    }
    if body.target_actor.is_empty() || body.doc_id.is_empty() || body.relation.is_empty() {
        return Err(HttpError::BadRequest(
            "missing a required argument needed to add doc actor relationship.".into(),
        ));
    }
    if body.relation == "owner" {
        return Err(HttpError::BadRequest(
            "OPERATION_FORBIDDEN: cannot add owner relation".into(),
        ));
    }
    Ok(())
}

async fn apply_add_doc_relationship(
    doc_acp: &dyn crate::router::DocumentAcpOperations,
    requestor: &identity::Did,
    body: &DocRelationshipRequest,
) -> Result<DocRelationshipResponse, HttpError> {
    let is_new = doc_acp
        .add_doc_relationship(
            requestor,
            &body.target_actor,
            &body.collection,
            &body.doc_id,
            &body.relation,
        )
        .await
        .map_err(http_error_from_backend_message)?;

    Ok(DocRelationshipResponse {
        existed_already: !is_new,
    })
}

/// Request body for document ACP decision preview operations.
#[derive(Debug, Clone, Deserialize)]
pub struct DocDecisionRequest {
    pub actor: String,
    pub permission: String,
    #[serde(rename = "policyID", alias = "policy_id")]
    pub policy_id: String,
    #[serde(rename = "resourceName", alias = "resource_name")]
    pub resource_name: String,
    #[serde(rename = "docID", alias = "doc_id")]
    pub doc_id: String,
}

/// Response for document ACP decision preview operations.
#[derive(Debug, Clone, Serialize)]
pub struct DocDecisionResponse {
    pub allowed: bool,
}

fn parse_doc_permission(permission: &str) -> Result<acp::DocumentPermission, HttpError> {
    match permission {
        "read" => Ok(acp::DocumentPermission::Read),
        "update" => Ok(acp::DocumentPermission::Update),
        "delete" => Ok(acp::DocumentPermission::Delete),
        _ => Err(HttpError::BadRequest(format!(
            "invalid document permission '{}'",
            permission
        ))),
    }
}

/// Check document ACP access without mutating relationships.
///
/// POST /api/v0/acp/document/decide
///
/// Requires `DacStatus` permission when NAC is enabled.
pub async fn decide_doc_access(
    State(state): State<AppState>,
    identity: ExtractIdentity,
    Json(body): Json<DocDecisionRequest>,
) -> Result<Json<DocDecisionResponse>, HttpError> {
    require_permission(&state, &identity, NodePermission::DacStatus).await?;

    if body.actor.is_empty()
        || body.permission.is_empty()
        || body.policy_id.is_empty()
        || body.resource_name.is_empty()
        || body.doc_id.is_empty()
    {
        return Err(HttpError::BadRequest(
            "missing a required argument needed to decide doc access.".into(),
        ));
    }

    let actor = body
        .actor
        .parse::<identity::Did>()
        .map_err(|e| HttpError::BadRequest(format!("invalid actor DID: {}", e)))?;
    let permission = parse_doc_permission(&body.permission)?;

    let doc_acp = bound(state.require_doc_acp()?, &identity.context());
    let allowed = doc_acp
        .check_doc_access(
            &actor,
            permission,
            &body.policy_id,
            &body.resource_name,
            &body.doc_id,
        )
        .await
        .map_err(http_error_from_backend_message)?;

    Ok(Json(DocDecisionResponse { allowed }))
}

/// Add a document ACP relationship.
///
/// POST /api/v0/acp/document/relationship
///
/// Requires `DacRelationAdd` permission when NAC is enabled.
pub async fn add_doc_relationship(
    State(state): State<AppState>,
    identity: ExtractIdentity,
    headers: HeaderMap,
    Json(body): Json<DocRelationshipRequest>,
) -> Result<Json<DocRelationshipResponse>, HttpError> {
    require_permission(&state, &identity, NodePermission::DacRelationAdd).await?;

    let requestor = identity
        .did()
        .ok_or_else(|| HttpError::BadRequest("identity required for document ACP".into()))?;

    validate_add_doc_relationship(&body)?;
    let context = super::txn_header::call_context(&identity, &headers)?;
    let doc_acp = relationship_operations(&state, &context)?;
    Ok(Json(
        apply_add_doc_relationship(doc_acp.as_ref(), requestor, &body).await?,
    ))
}

/// Add multiple document ACP relationships in one request.
///
/// POST /api/v0/acp/document/relationships
///
/// The request and response arrays have matching order. All entries are validated
/// before the first write. A backend error stops processing without rolling back
/// earlier entries. In a named transaction the caller may discard those entries
/// with the rest of the transaction.
pub async fn add_doc_relationships(
    State(state): State<AppState>,
    identity: ExtractIdentity,
    headers: HeaderMap,
    Json(bodies): Json<Vec<DocRelationshipRequest>>,
) -> Result<Json<Vec<DocRelationshipResponse>>, HttpError> {
    require_permission(&state, &identity, NodePermission::DacRelationAdd).await?;

    let requestor = identity
        .did()
        .ok_or_else(|| HttpError::BadRequest("identity required for document ACP".into()))?;

    for body in &bodies {
        validate_add_doc_relationship(body)?;
    }

    let context = super::txn_header::call_context(&identity, &headers)?;
    let doc_acp = relationship_operations(&state, &context)?;
    let mut results = Vec::with_capacity(bodies.len());
    for body in &bodies {
        results.push(apply_add_doc_relationship(doc_acp.as_ref(), requestor, body).await?);
    }
    Ok(Json(results))
}

/// Query for GET /api/v0/acp/document/relationships.
#[derive(Debug, serde::Deserialize)]
pub struct DocRelationshipsQuery {
    pub collection: String,
    #[serde(rename = "docID", alias = "doc_id")]
    pub doc_id: String,
}

/// List a document's ACP relationships.
///
/// GET /api/v0/acp/document/relationships?collection=<name>&docID=<id>
///
/// The core allows the document's owner or a holder of a relation that
/// manages one on its resource, as the embedded
/// `Node::list_dac_relationships_as`.
pub async fn list_doc_relationships(
    State(state): State<AppState>,
    identity: ExtractIdentity,
    axum::extract::Query(query): axum::extract::Query<DocRelationshipsQuery>,
) -> Result<Json<Vec<acp::DocumentRelationship>>, HttpError> {
    let requestor = identity
        .did()
        .ok_or_else(|| HttpError::Unauthorized("identity required for document ACP".into()))?;
    if query.collection.is_empty() || query.doc_id.is_empty() {
        return Err(HttpError::BadRequest(
            "collection and docID are required".into(),
        ));
    }
    let doc_acp = bound(state.require_doc_acp()?, &identity.context());
    doc_acp
        .list_doc_relationships(requestor, &query.collection, &query.doc_id)
        .await
        .map(Json)
        .map_err(http_error_from_backend_message)
}

/// Remove a document ACP relationship.
///
/// DELETE /api/v0/acp/document/relationship
///
/// Requires `DacRelationDelete` permission when NAC is enabled.
pub async fn remove_doc_relationship(
    State(state): State<AppState>,
    identity: ExtractIdentity,
    headers: HeaderMap,
    Json(body): Json<DocRelationshipRequest>,
) -> Result<Json<DocRelationshipResponse>, HttpError> {
    require_permission(&state, &identity, NodePermission::DacRelationDelete).await?;

    let requestor = identity
        .did()
        .ok_or_else(|| HttpError::BadRequest("identity required for document ACP".into()))?;

    if body.collection.is_empty() {
        return Err(HttpError::BadRequest(
            "collection name can't be empty".into(),
        ));
    }
    if body.target_actor.is_empty() || body.doc_id.is_empty() || body.relation.is_empty() {
        return Err(HttpError::BadRequest(
            "missing a required argument needed to delete doc actor relationship.".into(),
        ));
    }

    if body.relation == "owner" {
        return Err(HttpError::BadRequest(
            "OPERATION_FORBIDDEN: cannot delete owner relation".into(),
        ));
    }

    let context = super::txn_header::call_context(&identity, &headers)?;
    let doc_acp = relationship_operations(&state, &context)?;

    let was_removed = doc_acp
        .delete_doc_relationship(
            requestor,
            &body.target_actor,
            &body.collection,
            &body.doc_id,
            &body.relation,
        )
        .await
        .map_err(http_error_from_backend_message)?;

    Ok(Json(DocRelationshipResponse {
        existed_already: !was_removed,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::{MockDocumentAcpOperations, MockQueryExecutor};
    use crate::router::{AppStateBuilder, DocumentAcpOperations};
    use query::executor::QueryExecutor;
    use std::sync::Arc;

    fn doc_relationship(relation: &str, target_actor: &str) -> DocRelationshipRequest {
        DocRelationshipRequest {
            collection: "Users".into(),
            doc_id: "bae-123".into(),
            relation: relation.into(),
            target_actor: target_actor.into(),
        }
    }

    #[test]
    fn test_add_policy_response_serialize() {
        let response = AddPolicyResponse {
            policy_id: "policy-123".to_string(),
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("PolicyID"));
        assert!(json.contains("policy-123"));
    }

    #[test]
    fn test_policy_info_serialize() {
        let info = PolicyInfo {
            id: "policy-123".to_string(),
            name: Some("Test Policy".to_string()),
            description: None,
            resources: None,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("policy-123"));
        assert!(json.contains("Test Policy"));
        // Should not contain null fields
        assert!(!json.contains("description"));
    }

    #[test]
    fn test_doc_decision_request_accepts_camel_and_snake_case() {
        let camel: DocDecisionRequest = serde_json::from_str(
            r#"{
                "actor": "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK",
                "permission": "read",
                "policyID": "policy",
                "resourceName": "resource",
                "docID": "doc"
            }"#,
        )
        .unwrap();
        assert_eq!(camel.policy_id, "policy");
        assert_eq!(camel.resource_name, "resource");
        assert_eq!(camel.doc_id, "doc");

        let snake: DocDecisionRequest = serde_json::from_str(
            r#"{
                "actor": "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK",
                "permission": "delete",
                "policy_id": "policy",
                "resource_name": "resource",
                "doc_id": "doc"
            }"#,
        )
        .unwrap();
        assert_eq!(snake.policy_id, "policy");
        assert_eq!(snake.resource_name, "resource");
        assert_eq!(snake.doc_id, "doc");
    }

    #[test]
    fn test_parse_doc_permission() {
        assert_eq!(
            parse_doc_permission("read").unwrap(),
            acp::DocumentPermission::Read
        );
        assert_eq!(
            parse_doc_permission("update").unwrap(),
            acp::DocumentPermission::Update
        );
        assert_eq!(
            parse_doc_permission("delete").unwrap(),
            acp::DocumentPermission::Delete
        );
        assert!(matches!(
            parse_doc_permission("admin"),
            Err(HttpError::BadRequest(_))
        ));
    }

    #[nuclei::test]
    async fn add_doc_relationships_validates_every_item_before_writing() {
        let doc_acp = Arc::new(MockDocumentAcpOperations::new());
        let state = AppStateBuilder::new(
            Arc::new(MockQueryExecutor::new()) as Arc<dyn QueryExecutor>,
            edge::testing::edge(),
        )
        .with_doc_acp(Arc::clone(&doc_acp) as Arc<dyn DocumentAcpOperations>)
        .build();
        let requestor = identity::Did::new("did:key:requestor").unwrap();
        let identity = ExtractIdentity::from_did(Some(requestor));

        let result = add_doc_relationships(
            State(state.clone()),
            identity.clone(),
            HeaderMap::new(),
            Json(vec![
                doc_relationship("reader", "did:key:first"),
                doc_relationship("owner", "did:key:second"),
            ]),
        )
        .await;

        assert!(matches!(result, Err(HttpError::BadRequest(_))));
        assert_eq!(doc_acp.add_count(), 0);

        let Json(results) = add_doc_relationships(
            State(state),
            identity,
            HeaderMap::new(),
            Json(vec![
                doc_relationship("reader", "did:key:first"),
                doc_relationship("writer", "did:key:second"),
            ]),
        )
        .await
        .unwrap();

        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|result| !result.existed_already));
        assert_eq!(doc_acp.add_count(), 2);
    }

    #[nuclei::test]
    async fn transaction_header_never_falls_back_to_an_unbound_backend() {
        let doc_acp = Arc::new(MockDocumentAcpOperations::new());
        let state = AppStateBuilder::new(
            Arc::new(MockQueryExecutor::new()) as Arc<dyn QueryExecutor>,
            edge::testing::edge(),
        )
        .with_doc_acp(Arc::clone(&doc_acp) as Arc<dyn DocumentAcpOperations>)
        .build();
        let identity =
            ExtractIdentity::from_did(Some(identity::Did::new("did:key:caller").unwrap()));
        let mut headers = HeaderMap::new();
        headers.insert("x-defradb-tx", "1".parse().unwrap());
        let result = add_doc_relationship(
            State(state),
            identity,
            headers,
            Json(doc_relationship("reader", "did:key:reader")),
        )
        .await;
        assert!(matches!(result, Err(HttpError::NotImplemented(_))));
        assert_eq!(doc_acp.add_count(), 0);
    }
}
