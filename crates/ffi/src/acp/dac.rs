use std::ffi::c_char;

use acp::nac::NodePermission;

use crate::helpers::get_rt;
use crate::request::Request;
use crate::state::NODES;
use crate::target::go_string;
use crate::types::DefraResult;
use crate::{ffi_async, ffi_entry, try_ffi, ERR_INVALID_NODE_HANDLE};

/// Go's `acp.ErrFailedToAddDocActorRelationship`, which wraps every refusal
/// from document access control.
pub(crate) const ERR_ADD_DOC_ACTOR_RELATIONSHIP: &str =
    "failed to add document actor relationship with acp";
/// Go's `acp.ErrFailedToDeleteDocActorRelationship`.
pub(crate) const ERR_DELETE_DOC_ACTOR_RELATIONSHIP: &str =
    "failed to delete document actor relationship with acp";

async fn publish_latest_document_update(
    database: &crate::state::FfiDatabase,
    event_bus: &dyn events::Bus,
    collection_id: &str,
    doc_id: &str,
) -> Result<(), String> {
    let result = db::block::reader::read_latest_composite_block(database, doc_id).await?;
    event_bus.publish(events::Message::update(events::Update::new(
        doc_id.to_string(),
        result.cid,
        collection_id.to_string(),
        result.block,
        false,
        false,
    )));
    Ok(())
}

/// Resolve managing relations for a given relation within a policy resource.
///
/// Reads the policy from the node's DAC policies, finds the resource,
/// validates the relation exists, and returns the list of managing relations.
pub(crate) async fn resolve_managing_relations(
    database: &crate::state::FfiDatabase,
    policy_id: &str,
    resource_name: &str,
    relation: &str,
) -> Result<Vec<String>, String> {
    let Some(policies) = database.dac_policies() else {
        return Ok(Vec::new());
    };
    let Some(policy) = policies.get(policy_id).await.map_err(|e| e.to_string())? else {
        return Ok(Vec::new());
    };
    let Some(resource) = policy.get_resource(resource_name) else {
        return Ok(Vec::new());
    };
    if resource.get_relation(relation).is_none() {
        return Err(format!(
            "relation '{}' not found in policy resource '{}'",
            relation, resource_name
        ));
    }
    Ok(policy
        .get_managers_for_relation(resource_name, relation)
        .into_iter()
        .map(str::to_string)
        .collect())
}

/// Add a DAC policy.
///
/// Accepts a policy definition in YAML or JSON format.
/// Returns a JSON object with the policy ID:
/// ```json
/// { "PolicyID": "sha256_hash_of_policy" }
/// ```
///
/// # Safety
///
/// `policy` must be a valid null-terminated UTF-8 string containing
/// the policy definition in YAML or JSON format.
/// Adds the document access-control policy `policy` (YAML), created by the
/// identity `identity_ptr` names, and answers with `{"PolicyID": ...}`.
///
/// # Safety
///
/// `policy` must be null or a valid null-terminated string.
#[no_mangle]
pub unsafe extern "C" fn defra_acp_dac_policy_add(
    node_ptr: usize,
    identity_ptr: usize,
    policy: *const c_char,
) -> DefraResult {
    ffi_entry! {
        let request = try_ffi!(Request::begin(node_ptr, identity_ptr, NodePermission::DacPolicyAdd));
        try_ffi!(request.require_node("policy add"));
        let policy = go_string(policy);
        let context = request.context();
        let runner = try_ffi!(request.runner());
        // The core checks the creator and the policy, and stores it with the
        // node's document ACP, local or Vera.
        ffi_async!(caller, {
            let policy_id = runner
                .add_dac_policy_as(&context, &policy)
                .await
                .map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "PolicyID": policy_id }).to_string())
        })
    }
}

/// Add a DAC actor relationship (share document access with target).
///
/// The requestor must be the document owner. Relation can be:
/// - "reader" - read access
/// - "updater" - read + update access
/// - "deleter" - read + delete access
///
/// Answers with Go's `AddActorRelationshipResult`:
/// ```json
/// { "ExistedAlready": false }  // true when the relationship already existed
/// ```
///
/// # Safety
///
/// All string parameters must be valid null-terminated UTF-8 strings.
/// Adds the relation `relation` between `actor` and document `doc_id` of
/// collection `collection`, as the identity `identity_ptr` names, and
/// answers with the result as JSON.
///
/// # Safety
///
/// Every string must be null or a valid null-terminated string.
#[no_mangle]
pub unsafe extern "C" fn defra_acp_dac_relationship_add(
    node_ptr: usize,
    identity_ptr: usize,
    collection: *const c_char,
    doc_id: *const c_char,
    relation: *const c_char,
    actor: *const c_char,
) -> DefraResult {
    ffi_entry! {
        let rt = try_ffi!(get_rt());
        let request = try_ffi!(Request::begin(node_ptr, identity_ptr, NodePermission::DacRelationAdd));
        let node_ptr = try_ffi!(request.require_node("document relationship changes"));

        let requestor_str = request.identity.clone().unwrap_or_default();
        let target_str = go_string(actor);
        let collection_id_str = go_string(collection);
        let doc_id_str = go_string(doc_id);
        let relation_str = go_string(relation);

        // Go checks collection name first (via GetCollectionByName)
        if collection_id_str.is_empty() {
            return DefraResult::error("collection name can't be empty");
        }

        // Then validates remaining required arguments (matches Go bridge.go validation)
        if requestor_str.is_empty()
            || target_str.is_empty()
            || doc_id_str.is_empty()
            || relation_str.is_empty()
        {
            return DefraResult::error(
                "missing a required argument needed to add doc actor relationship.",
            );
        }

        // Validate node handle and get database, document_acp and event_bus
        let (database, document_acp, event_bus) = match NODES.get(node_ptr, |state| {
            (
                state.database.clone(),
                state.document_acp.clone(),
                state.event_bus.clone(),
            )
        }) {
            Some(tuple) => tuple,
            None => return DefraResult::error(ERR_INVALID_NODE_HANDLE),
        };

        // Resolve collection name -> policy resource_name and policy_id
        let (resource_name, policy_id, collection_id) =
            match database.get_collection(&collection_id_str) {
            Ok(Some(col)) => match col.schema().policy {
                Some(ref policy) => (
                    policy.resource_name.clone(),
                    policy.id.clone(),
                    col.collection_id().to_string(),
                ),
                None => {
                    return DefraResult::error("operation requires ACP, but collection has no policy");
                }
            },
            Ok(None) => {
                return DefraResult::error(format!("collection '{}' does not exist", collection_id_str));
            }
            Err(e) => {
                return DefraResult::error(format!(
                    "failed to get collection '{}': {}",
                    collection_id_str, e
                ));
            }
        };

        // Owner relation is immutable - cannot be added or removed
        if relation_str == "owner" {
            return DefraResult::error(format!(
                "{ERR_ADD_DOC_ACTOR_RELATIONSHIP}: OPERATION_FORBIDDEN: cannot add owner relation"
            ));
        }

        let result = crate::helpers::block_on(rt, async {
            let managing_relations =
                resolve_managing_relations(&database, &policy_id, &resource_name, &relation_str)
                    .await?;
            let requestor = identity::Did::new(&requestor_str)
                .map_err(|e| format!("invalid requestor DID '{}': {}", requestor_str, e))?;
            let target = if target_str == "*" {
                identity::Did::wildcard()
            } else {
                identity::Did::new(&target_str)
                    .map_err(|e| format!("invalid target DID '{}': {}", target_str, e))?
            };

            let added = document_acp
                .add_actor_relationship(
                    &requestor,
                    &target,
                    &policy_id,
                    &resource_name,
                    &doc_id_str,
                    &relation_str,
                    &managing_relations,
                )
                .await
                .map_err(|e| e.to_string())?;

            if added && doc_id_str != collection_id {
                publish_latest_document_update(
                    &database,
                    event_bus.as_ref(),
                    &collection_id,
                    &doc_id_str,
                )
                .await?;
            }

            // Local ACP relationships are node-local (matches Go): a grant is not
            // propagated to peers. Cross-node access control is Vera's role.
            let json = serde_json::json!({ "ExistedAlready": !added }).to_string();
            Ok::<String, String>(json)
        });

        match result {
            Ok(json) => DefraResult::success(json),
            Err(e) => DefraResult::error(format!("{ERR_ADD_DOC_ACTOR_RELATIONSHIP}: {e}")),
        }
    }
}

/// Delete a DAC actor relationship (revoke document access from target).
///
/// The requestor must be the document owner.
///
/// Answers with Go's `DeleteActorRelationshipResult`:
/// ```json
/// { "RecordFound": true }  // false when there was no such relationship
/// ```
///
/// # Safety
///
/// All string parameters must be valid null-terminated UTF-8 strings.
/// Deletes the relation `relation` between `actor` and document `doc_id` of
/// collection `collection`, as the identity `identity_ptr` names, and
/// answers with the result as JSON.
///
/// # Safety
///
/// Every string must be null or a valid null-terminated string.
#[no_mangle]
pub unsafe extern "C" fn defra_acp_dac_relationship_delete(
    node_ptr: usize,
    identity_ptr: usize,
    collection: *const c_char,
    doc_id: *const c_char,
    relation: *const c_char,
    actor: *const c_char,
) -> DefraResult {
    ffi_entry! {
        let rt = try_ffi!(get_rt());
        let request = try_ffi!(Request::begin(node_ptr, identity_ptr, NodePermission::DacRelationDelete));
        let node_ptr = try_ffi!(request.require_node("document relationship changes"));

        let requestor_str = request.identity.clone().unwrap_or_default();
        let target_str = go_string(actor);
        let collection_id_str = go_string(collection);
        let doc_id_str = go_string(doc_id);
        let relation_str = go_string(relation);

        // Go checks collection name first (via GetCollectionByName)
        if collection_id_str.is_empty() {
            return DefraResult::error("collection name can't be empty");
        }

        // Then validates remaining required arguments (matches Go bridge.go validation)
        if requestor_str.is_empty()
            || target_str.is_empty()
            || doc_id_str.is_empty()
            || relation_str.is_empty()
        {
            return DefraResult::error(
                "missing a required argument needed to delete doc actor relationship.",
            );
        }

        // Validate node handle and get database and document_acp
        let (database, document_acp) = match NODES.get(node_ptr, |state| {
            (state.database.clone(), state.document_acp.clone())
        }) {
            Some(tuple) => tuple,
            None => return DefraResult::error(ERR_INVALID_NODE_HANDLE),
        };

        // Resolve collection name -> policy resource_name and policy_id
        let (resource_name, policy_id) = match database.get_collection(&collection_id_str) {
            Ok(Some(col)) => match col.schema().policy {
                Some(ref policy) => (policy.resource_name.clone(), policy.id.clone()),
                None => {
                    return DefraResult::error("operation requires ACP, but collection has no policy");
                }
            },
            Ok(None) => {
                return DefraResult::error(format!("collection '{}' does not exist", collection_id_str));
            }
            Err(e) => {
                return DefraResult::error(format!(
                    "failed to get collection '{}': {}",
                    collection_id_str, e
                ));
            }
        };

        // Owner relation is immutable - cannot be added or removed
        if relation_str == "owner" {
            return DefraResult::error(format!(
                "{ERR_DELETE_DOC_ACTOR_RELATIONSHIP}: OPERATION_FORBIDDEN: cannot delete owner relation"
            ));
        }

        let result = crate::helpers::block_on(rt, async {
            let managing_relations =
                resolve_managing_relations(&database, &policy_id, &resource_name, &relation_str)
                    .await?;
            let requestor = identity::Did::new(&requestor_str)
                .map_err(|e| format!("invalid requestor DID '{}': {}", requestor_str, e))?;
            let target = if target_str == "*" {
                identity::Did::wildcard()
            } else {
                identity::Did::new(&target_str)
                    .map_err(|e| format!("invalid target DID '{}': {}", target_str, e))?
            };

            let deleted = document_acp
                .delete_actor_relationship(
                    &requestor,
                    &target,
                    &policy_id,
                    &resource_name,
                    &doc_id_str,
                    &relation_str,
                    &managing_relations,
                )
                .await
                .map_err(|e| e.to_string())?;

            // Local ACP relationships are node-local (matches Go): a revoke is not
            // propagated to peers. Cross-node access control is Vera's role.
            let json = serde_json::json!({ "RecordFound": deleted }).to_string();
            Ok::<String, String>(json)
        });

        match result {
            Ok(json) => DefraResult::success(json),
            Err(e) => DefraResult::error(format!("{ERR_DELETE_DOC_ACTOR_RELATIONSHIP}: {e}")),
        }
    }
}
