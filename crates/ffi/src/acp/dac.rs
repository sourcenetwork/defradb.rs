use std::ffi::c_char;

use acp::nac::NodePermission;

use crate::helpers::{get_rt, require_c_str};
use crate::nac_check::check_nac_for_node;
use crate::state::NODES;
use crate::types::FfiResult;
use crate::{ffi_async, ffi_entry, try_ffi, ERR_INVALID_NODE_HANDLE};

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

/// Add a DAC policy, Go's `AddDACPolicy`, with `identity_did` as its
/// creator. The core checks node access and the policy.
///
/// Returns a JSON object with the policy ID:
/// ```json
/// { "PolicyID": "sha256_hash_of_policy" }
/// ```
///
/// # Safety
///
/// `identity_did` must be null or a valid null-terminated UTF-8 string;
/// `policy` must be a valid null-terminated UTF-8 string containing the
/// policy definition in YAML or JSON format.
#[no_mangle]
pub unsafe extern "C" fn add_dac_policy(
    node_ptr: usize,
    identity_did: *const c_char,
    policy: *const c_char,
) -> FfiResult {
    ffi_entry! {
        let context = try_ffi!(crate::lens::call_context(identity_did, std::ptr::null()));
        let policy = try_ffi!(require_c_str(policy, "policy"));
        let runner = try_ffi!(crate::helpers::get_node_runner(node_ptr));
        ffi_async!(node_ptr, {
            let policy_id = runner
                .add_dac_policy_as(&context, &policy)
                .await
                .map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "PolicyID": policy_id }).to_string())
        })
    }
}

/// Get a DAC policy by ID.
///
/// Returns a JSON object with the policy's ID, name, description and
/// resources, or null if no ACP has it.
///
/// # Safety
///
/// `policy_id` must be a valid null-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn get_dac_policy(node_ptr: usize, policy_id: *const c_char) -> FfiResult {
    ffi_entry! {
        let policy_id = try_ffi!(require_c_str(policy_id, "policy_id"));
        let context = try_ffi!(node_context(node_ptr));
        let runner = try_ffi!(crate::helpers::get_node_runner(node_ptr));
        ffi_async!(node_ptr, {
            let policy = runner
                .get_dac_policy_as(&context, &policy_id)
                .await
                .map_err(|e| e.to_string())?;
            Ok(policy.as_ref().map(policy_json).unwrap_or_default().to_string())
        })
    }
}

/// List all DAC policy IDs.
///
/// Returns a JSON array of policy IDs.
#[no_mangle]
pub extern "C" fn list_dac_policies(node_ptr: usize) -> FfiResult {
    ffi_entry! {
        let context = try_ffi!(node_context(node_ptr));
        let runner = try_ffi!(crate::helpers::get_node_runner(node_ptr));
        ffi_async!(node_ptr, {
            let policies = runner
                .list_dac_policies_as(&context)
                .await
                .map_err(|e| e.to_string())?;
            let ids: Vec<&str> = policies.iter().map(|policy| policy.id.as_str()).collect();
            Ok(serde_json::json!(ids).to_string())
        })
    }
}

/// The context the node's own reads run in: its default identity acts.
fn node_context(node_ptr: usize) -> Result<defra_core::RequestContext, FfiResult> {
    NODES
        .get(node_ptr, |state| state.identity_did())
        .map(|did| defra_core::RequestContext::default().with_identity(did))
        .ok_or_else(|| FfiResult::error(ERR_INVALID_NODE_HANDLE))
}

/// A policy as the FFI answers it.
fn policy_json(policy: &acp::Policy) -> serde_json::Value {
    serde_json::json!({
        "id": policy.id,
        "name": policy.name,
        "description": policy.attributes.get("description"),
        "resources": policy.resources,
    })
}

/// Add a DAC actor relationship (share document access with target).
///
/// The requestor must be the document owner. Relation can be:
/// - "reader" - read access
/// - "updater" - read + update access
/// - "deleter" - read + delete access
///
/// Returns JSON with success status:
/// ```json
/// { "added": true }  // or false if already exists
/// ```
///
/// # Safety
///
/// All string parameters must be valid null-terminated UTF-8 strings.
#[no_mangle]
pub unsafe extern "C" fn add_dac_actor_relationship(
    node_ptr: usize,
    requestor_did: *const c_char,
    target_did: *const c_char,
    collection_id: *const c_char,
    doc_id: *const c_char,
    relation: *const c_char,
) -> FfiResult {
    ffi_entry! {
        let rt = try_ffi!(get_rt());
        try_ffi!(check_nac_for_node(
            node_ptr,
            requestor_did,
            NodePermission::DacRelationAdd
        ));

        let requestor_str = try_ffi!(require_c_str(requestor_did, "requestor_did"));
        let target_str = try_ffi!(require_c_str(target_did, "target_did"));
        let collection_id_str = try_ffi!(require_c_str(collection_id, "collection_id"));
        let doc_id_str = try_ffi!(require_c_str(doc_id, "doc_id"));
        let relation_str = try_ffi!(require_c_str(relation, "relation"));

        // Go checks collection name first (via GetCollectionByName)
        if collection_id_str.is_empty() {
            return FfiResult::error("collection name can't be empty");
        }

        // Then validates remaining required arguments (matches Go bridge.go validation)
        if requestor_str.is_empty()
            || target_str.is_empty()
            || doc_id_str.is_empty()
            || relation_str.is_empty()
        {
            return FfiResult::error(
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
            None => return FfiResult::error(ERR_INVALID_NODE_HANDLE),
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
                    return FfiResult::error("operation requires ACP, but collection has no policy");
                }
            },
            Ok(None) => {
                return FfiResult::error(format!("collection '{}' does not exist", collection_id_str));
            }
            Err(e) => {
                return FfiResult::error(format!(
                    "failed to get collection '{}': {}",
                    collection_id_str, e
                ));
            }
        };

        // Owner relation is immutable - cannot be added or removed
        if relation_str == "owner" {
            return FfiResult::error("OPERATION_FORBIDDEN: cannot add owner relation");
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
            let json = serde_json::json!({ "added": added }).to_string();
            Ok::<String, String>(json)
        });

        match result {
            Ok(json) => FfiResult::success(json),
            Err(e) => FfiResult::error(e),
        }
    }
}

/// Delete a DAC actor relationship (revoke document access from target).
///
/// The requestor must be the document owner.
///
/// Returns JSON with success status:
/// ```json
/// { "deleted": true }  // or false if didn't exist
/// ```
///
/// # Safety
///
/// All string parameters must be valid null-terminated UTF-8 strings.
#[no_mangle]
pub unsafe extern "C" fn delete_dac_actor_relationship(
    node_ptr: usize,
    requestor_did: *const c_char,
    target_did: *const c_char,
    collection_id: *const c_char,
    doc_id: *const c_char,
    relation: *const c_char,
) -> FfiResult {
    ffi_entry! {
        let rt = try_ffi!(get_rt());
        try_ffi!(check_nac_for_node(
            node_ptr,
            requestor_did,
            NodePermission::DacRelationDelete
        ));

        let requestor_str = try_ffi!(require_c_str(requestor_did, "requestor_did"));
        let target_str = try_ffi!(require_c_str(target_did, "target_did"));
        let collection_id_str = try_ffi!(require_c_str(collection_id, "collection_id"));
        let doc_id_str = try_ffi!(require_c_str(doc_id, "doc_id"));
        let relation_str = try_ffi!(require_c_str(relation, "relation"));

        // Go checks collection name first (via GetCollectionByName)
        if collection_id_str.is_empty() {
            return FfiResult::error("collection name can't be empty");
        }

        // Then validates remaining required arguments (matches Go bridge.go validation)
        if requestor_str.is_empty()
            || target_str.is_empty()
            || doc_id_str.is_empty()
            || relation_str.is_empty()
        {
            return FfiResult::error(
                "missing a required argument needed to delete doc actor relationship.",
            );
        }

        // Validate node handle and get database and document_acp
        let (database, document_acp) = match NODES.get(node_ptr, |state| {
            (
                state.database.clone(),
                state.document_acp.clone(),
            )
        }) {
            Some(tuple) => tuple,
            None => return FfiResult::error(ERR_INVALID_NODE_HANDLE),
        };

        // Resolve collection name -> policy resource_name and policy_id
        let (resource_name, policy_id) = match database.get_collection(&collection_id_str) {
            Ok(Some(col)) => match col.schema().policy {
                Some(ref policy) => (policy.resource_name.clone(), policy.id.clone()),
                None => {
                    return FfiResult::error("operation requires ACP, but collection has no policy");
                }
            },
            Ok(None) => {
                return FfiResult::error(format!("collection '{}' does not exist", collection_id_str));
            }
            Err(e) => {
                return FfiResult::error(format!(
                    "failed to get collection '{}': {}",
                    collection_id_str, e
                ));
            }
        };

        // Owner relation is immutable - cannot be added or removed
        if relation_str == "owner" {
            return FfiResult::error("OPERATION_FORBIDDEN: cannot delete owner relation");
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
            let json = serde_json::json!({ "deleted": deleted }).to_string();
            Ok::<String, String>(json)
        });

        match result {
            Ok(json) => FfiResult::success(json),
            Err(e) => FfiResult::error(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::{new_node, node_close};
    use crate::types::NodeInitOptions;
    use crate::{add_schema, exec_request};
    use std::ffi::{CStr, CString};

    fn ffi_value(result: &crate::types::FfiResult) -> String {
        assert_eq!(result.status, 0, "expected success status");
        unsafe { CStr::from_ptr(result.value).to_string_lossy().into_owned() }
    }

    fn extract_doc_id(payload: &str, field: &str) -> String {
        let json: serde_json::Value = serde_json::from_str(payload).unwrap();
        let data = &json["data"][field];
        match data {
            serde_json::Value::Array(items) => items[0]["_docID"].as_str().unwrap().to_string(),
            serde_json::Value::Object(_) => data["_docID"].as_str().unwrap().to_string(),
            other => panic!("unexpected GraphQL payload for {field}: {other}"),
        }
    }

    fn test_did() -> &'static str {
        "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK"
    }

    fn test_did2() -> &'static str {
        "did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH"
    }

    #[test]
    fn test_dac_actor_relationship_missing_collection_returns_error() {
        assert!(crate::runtime::init_runtime());

        let result = new_node(NodeInitOptions::default());
        assert_eq!(result.status, 0, "new_node should succeed");
        let node = result.node_ptr;

        let requestor_did = CString::new(test_did()).unwrap();
        let target_did = CString::new(test_did2()).unwrap();
        let collection_id = CString::new("test-collection").unwrap();
        let doc_id = CString::new("test-doc").unwrap();
        let relation = CString::new("reader").unwrap();

        let result = unsafe {
            add_dac_actor_relationship(
                node,
                requestor_did.as_ptr(),
                target_did.as_ptr(),
                collection_id.as_ptr(),
                doc_id.as_ptr(),
                relation.as_ptr(),
            )
        };
        assert_eq!(result.status, 1, "missing collection should be an error");
        assert!(
            result.value.is_null(),
            "error result should not contain a value"
        );
        assert!(
            !result.error.is_null(),
            "error result should contain a message"
        );
        let error = unsafe { CStr::from_ptr(result.error).to_string_lossy() };
        assert_eq!(error, "collection 'test-collection' does not exist");
        unsafe { crate::types::defra_free_string(result.error) };

        let close_result = node_close(node);
        assert_eq!(close_result.status, 0, "node_close should succeed");
    }

    #[test]
    fn test_local_dac_policy_is_enforced_via_ffi() {
        assert!(crate::runtime::init_runtime());

        let options = NodeInitOptions::default();
        let result = new_node(options);
        assert_eq!(result.status, 0, "new_node should succeed");
        let node = result.node_ptr;

        let owner_did = CString::new(test_did()).unwrap();
        let viewer_did = CString::new(test_did2()).unwrap();
        let policy_yaml = CString::new(
            r#"
name: Viewer Policy
resources:
  - name: users
    relations:
      - name: viewer
      - name: editor
      - name: remover
    permissions:
      - name: read
        expr: viewer
      - name: update
        expr: editor
      - name: delete
        expr: remover
"#,
        )
        .unwrap();

        let add_policy_result =
            unsafe { add_dac_policy(node, owner_did.as_ptr(), policy_yaml.as_ptr()) };
        let add_policy_json = ffi_value(&add_policy_result);
        let policy_id = serde_json::from_str::<serde_json::Value>(&add_policy_json).unwrap()
            ["PolicyID"]
            .as_str()
            .unwrap()
            .to_string();
        unsafe { crate::types::defra_free_string(add_policy_result.value) };

        let policy_id_c = CString::new(policy_id.clone()).unwrap();
        let get_policy_result = unsafe { get_dac_policy(node, policy_id_c.as_ptr()) };
        let get_policy_json = ffi_value(&get_policy_result);
        assert!(get_policy_json.contains("Viewer Policy"));
        unsafe { crate::types::defra_free_string(get_policy_result.value) };

        let list_policy_result = list_dac_policies(node);
        let list_policy_json = ffi_value(&list_policy_result);
        assert!(list_policy_json.contains(&policy_id));
        unsafe { crate::types::defra_free_string(list_policy_result.value) };

        let sdl = CString::new(format!(
            r#"type User @policy(id: "{policy_id}", resource: "users") {{ name: String }}"#
        ))
        .unwrap();
        let schema_result = unsafe { add_schema(node, owner_did.as_ptr(), sdl.as_ptr()) };
        assert_eq!(schema_result.status, 0, "add_schema should succeed");
        if !schema_result.value.is_null() {
            unsafe { crate::types::defra_free_string(schema_result.value) };
        }

        let add_doc =
            CString::new(r#"mutation { add_User(input: {name: "Alice"}) { _docID name } }"#)
                .unwrap();
        let add_doc_result = unsafe {
            exec_request(
                node,
                std::ptr::null(),
                owner_did.as_ptr(),
                add_doc.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        let add_doc_json = ffi_value(&add_doc_result);
        let doc_id = extract_doc_id(&add_doc_json, "add_User");
        unsafe { crate::types::defra_free_string(add_doc_result.value) };

        let read_query = CString::new(r#"{ User { _docID name } }"#).unwrap();
        let denied_read_result = unsafe {
            exec_request(
                node,
                std::ptr::null(),
                viewer_did.as_ptr(),
                read_query.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        let denied_read_json = ffi_value(&denied_read_result);
        assert!(
            !denied_read_json.contains(&doc_id),
            "viewer should not see protected documents before a relationship is granted"
        );
        unsafe { crate::types::defra_free_string(denied_read_result.value) };

        let collection_id = CString::new("User").unwrap();
        let doc_id_c = CString::new(doc_id.clone()).unwrap();
        let relation = CString::new("viewer").unwrap();
        let mut relationship_events = NODES
            .get(node, |state| {
                state.event_bus.subscribe(&[events::EventName::Update])
            })
            .unwrap();
        let add_rel_result = unsafe {
            add_dac_actor_relationship(
                node,
                owner_did.as_ptr(),
                viewer_did.as_ptr(),
                collection_id.as_ptr(),
                doc_id_c.as_ptr(),
                relation.as_ptr(),
            )
        };
        let add_rel_json = ffi_value(&add_rel_result);
        assert!(add_rel_json.contains(r#""added":true"#));
        unsafe { crate::types::defra_free_string(add_rel_result.value) };

        let message = relationship_events
            .try_recv()
            .expect("relationship update event");
        let update = message.as_update().expect("document update");
        assert_eq!(update.doc_id, doc_id);
        assert!(!update.block.is_empty());
        assert_eq!(
            defra_core::block::generate_cid_from_bytes(&update.block).unwrap(),
            update.cid
        );

        let allowed_read_result = unsafe {
            exec_request(
                node,
                std::ptr::null(),
                viewer_did.as_ptr(),
                read_query.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        let allowed_read_json = ffi_value(&allowed_read_result);
        assert!(allowed_read_json.contains(&doc_id));
        assert!(allowed_read_json.contains("Alice"));
        unsafe { crate::types::defra_free_string(allowed_read_result.value) };

        let update_mutation = CString::new(format!(
            r#"mutation {{ update_User(docIDs: ["{doc_id}"], input: {{name: "Mallory"}}) {{ _docID name }} }}"#
        ))
        .unwrap();
        let denied_update_result = unsafe {
            exec_request(
                node,
                std::ptr::null(),
                viewer_did.as_ptr(),
                update_mutation.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        let denied_update_json = ffi_value(&denied_update_result);
        assert!(
            denied_update_json.contains("permission denied")
                || denied_update_json.contains("UNAUTHORIZED")
                || denied_update_json.contains("errors"),
            "viewer should not gain update access from a read-only relation: {denied_update_json}"
        );
        unsafe { crate::types::defra_free_string(denied_update_result.value) };

        let close_result = node_close(node);
        assert_eq!(close_result.status, 0, "node_close should succeed");
    }
}
