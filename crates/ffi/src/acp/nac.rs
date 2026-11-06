use std::ffi::c_char;

use crate::helpers::require_c_str;
use crate::request::Request;
use crate::state::NODES;
use crate::types::{c_str_to_string, DefraResult};
use crate::{ffi_async, ffi_async_ok, ffi_entry, try_ffi, ERR_INVALID_NODE_HANDLE};
use acp::nac::{is_valid_nac_relation, NodePermission};
use acp::normalize_auth_error;

/// Get the current NAC status.
///
/// Returns a JSON object with NAC status information:
/// ```json
/// {
///   "status": "enabled" | "disabled temporarily" | "not configured",
///   "configured_enabled": true | false,
///   "dev_mode": true | false,
///   "owner": "did:key:..." | null
/// }
/// ```
///
/// This function is NAC-gated with the `NacStatus` permission.
///
/// # Safety
///
/// Caller must ensure all pointer arguments are valid, non-null, and point to valid C strings.
/// Answers with the node access-control status as JSON.
#[no_mangle]
pub extern "C" fn defra_acp_nac_status(node_ptr: usize, identity_ptr: usize) -> DefraResult {
    ffi_entry! {
        let request = try_ffi!(Request::begin(node_ptr, identity_ptr, NodePermission::NacStatus));
        let node_ptr = request.node();

        let nac_manager = match NODES.get(node_ptr, |state| state.nac_manager.clone()) {
            Some(m) => m,
            None => return DefraResult::error(ERR_INVALID_NODE_HANDLE),
        };

        ffi_async!(node_ptr, {
            let info = nac_manager.info().await;
            let json = serde_json::to_string(&info)
                .map_err(|e| format!("failed to serialize NAC info: {}", e))?;

            Ok(json)
        })
    }
}

/// Temporarily disable NAC.
///
/// The requestor_did must be an admin. Returns success on completion.
///
/// # Safety
///
/// `requestor_did` must be a valid null-terminated UTF-8 string.
/// Disables node access control, as the identity `identity_ptr` names.
#[no_mangle]
pub extern "C" fn defra_acp_nac_disable(node_ptr: usize, identity_ptr: usize) -> DefraResult {
    ffi_entry! {
        let request = try_ffi!(Request::resolve(node_ptr, identity_ptr));
        let node_ptr = try_ffi!(request.require_node("node access control changes"));
        let requestor_str = request.identity.clone().unwrap_or_default();

        let nac_manager = match NODES.get(node_ptr, |state| state.nac_manager.clone()) {
            Some(m) => m,
            None => return DefraResult::error(ERR_INVALID_NODE_HANDLE),
        };

        ffi_async_ok!(node_ptr, {
            // Check NAC state before validating DID (Go checks state first)
            let status = nac_manager.status().await;
            if status == acp::nac::NacStatus::NotConfigured {
                return Err("node acp is not configured".to_string());
            }
            if status == acp::nac::NacStatus::DisabledTemporarily {
                return Err("node acp is already disabled".to_string());
            }

            // Empty DID means no identity - not authorized
            if requestor_str.is_empty() {
                return Err(format!("not authorized to perform operation. Permission: {}", NodePermission::NacDisable));
            }

            let requestor = identity::Did::new(&requestor_str)
                .map_err(|e| format!("invalid DID '{}': {}", requestor_str, e))?;

            nac_manager
                .disable(&requestor)
                .await
                .map_err(|e| normalize_auth_error(e.to_string(), "disable-nac"))?;

            Ok(())
        })
    }
}

/// Re-enable NAC after temporary disable.
///
/// The requestor_did must be an admin. Returns success on completion.
///
/// # Safety
///
/// `requestor_did` must be a valid null-terminated UTF-8 string.
/// Re-enables node access control, as the identity `identity_ptr` names.
#[no_mangle]
pub extern "C" fn defra_acp_nac_re_enable(node_ptr: usize, identity_ptr: usize) -> DefraResult {
    ffi_entry! {
        let request = try_ffi!(Request::resolve(node_ptr, identity_ptr));
        let node_ptr = try_ffi!(request.require_node("node access control changes"));
        let requestor_str = request.identity.clone().unwrap_or_default();

        let nac_manager = match NODES.get(node_ptr, |state| state.nac_manager.clone()) {
            Some(m) => m,
            None => return DefraResult::error(ERR_INVALID_NODE_HANDLE),
        };

        ffi_async_ok!(node_ptr, {
            // Check NAC state before validating DID (Go checks state first)
            let status = nac_manager.status().await;
            if status == acp::nac::NacStatus::NotConfigured {
                return Err("node acp is not configured".to_string());
            }

            // Empty DID means no identity - not authorized
            if requestor_str.is_empty() {
                return Err(
                    format!("not authorized to perform operation. Permission: {}", NodePermission::NacReEnable),
                );
            }

            let requestor = identity::Did::new(&requestor_str)
                .map_err(|e| format!("invalid DID '{}': {}", requestor_str, e))?;

            // Check admin using persisted relationships (is_admin returns true
            // for everyone when disabled, but re-enable needs real admin check)
            let is_admin = nac_manager
                .is_admin_persisted(&requestor)
                .await
                .unwrap_or(false);
            if !is_admin {
                return Err(
                    format!("not authorized to perform operation. Permission: {}", NodePermission::NacReEnable),
                );
            }
            if status == acp::nac::NacStatus::Enabled {
                return Err("node acp is already enabled".to_string());
            }

            nac_manager
                .re_enable(&requestor)
                .await
                .map_err(|e| normalize_auth_error(e.to_string(), "re-enable-nac"))?;

            Ok(())
        })
    }
}

/// Add a NAC actor relationship.
///
/// The requestor must be an admin. The relation can be "admin" or a
/// specific permission name (e.g., "read-document").
///
/// Answers with Go's `AddActorRelationshipResult`:
/// ```json
/// { "ExistedAlready": false }  // true when the relationship already existed
/// ```
///
/// # Safety
///
/// All string parameters must be valid null-terminated UTF-8 strings.
/// Adds the node access-control relation `relation` ("admin" for null or
/// empty) of `actor`, as the identity `identity_ptr` names.
///
/// # Safety
///
/// `relation` and `actor` must be null or valid null-terminated strings.
#[no_mangle]
pub unsafe extern "C" fn defra_acp_nac_relationship_add(
    node_ptr: usize,
    identity_ptr: usize,
    relation: *const c_char,
    actor: *const c_char,
) -> DefraResult {
    ffi_entry! {
        let request = try_ffi!(Request::resolve(node_ptr, identity_ptr));
        let node_ptr = try_ffi!(request.require_node("node access control changes"));
        let requestor_str = request.identity.clone().unwrap_or_default();
        let target_did = actor;

        let relation_str = match c_str_to_string(relation) {
            Some(s) if !s.is_empty() => s,
            _ => "admin".to_string(),
        };

        let target_str = try_ffi!(require_c_str(target_did, "target_did"));

        let nac_manager = match NODES.get(node_ptr, |state| state.nac_manager.clone()) {
            Some(m) => m,
            None => return DefraResult::error(ERR_INVALID_NODE_HANDLE),
        };

        ffi_async!(node_ptr, {
            // Check NAC state before validating DID (Go checks state first)
            let status = nac_manager.status().await;
            if status == acp::nac::NacStatus::NotConfigured {
                return Err("node acp is not configured".to_string());
            }
            if status == acp::nac::NacStatus::DisabledTemporarily {
                return Err("operation requires ACP, but ACP not available".to_string());
            }

            // Empty or wildcard requestor: check if wildcard admin exists to decide error
            if requestor_str.is_empty() || requestor_str == "*" {
                let wildcard = identity::Did::wildcard();
                let wildcard_is_admin = nac_manager.is_admin(&wildcard).await.unwrap_or(false);
                if wildcard_is_admin {
                    return Err("node acp relationship operation requires identity".to_string());
                } else {
                    return Err(
                        format!("not authorized to perform operation. Permission: {}", NodePermission::NacRelationAdd),
                    );
                }
            }

            let requestor = identity::Did::new(&requestor_str).map_err(|_| {
                format!("not authorized to perform operation. Permission: {}", NodePermission::NacRelationAdd)
            })?;

            // Check admin authorization BEFORE validating target DID
            if !nac_manager.is_admin(&requestor).await.unwrap_or(false) {
                return Err(
                    format!("not authorized to perform operation. Permission: {}", NodePermission::NacRelationAdd),
                );
            }

            // Validate relation name against NAC policy
            if !is_valid_nac_relation(&relation_str) {
                return Err("relation not in resource".to_string());
            }

            // Owner relation cannot be modified
            if relation_str == "owner" {
                return Err("relation not in resource".to_string());
            }

            // Validate target DID
            if target_str.is_empty() {
                return Err("actor must be a valid did".to_string());
            }

            let target = if target_str == "*" {
                identity::Did::wildcard()
            } else {
                identity::Did::new(&target_str)
                    .map_err(|e| format!("invalid target DID '{}': {}", target_str, e))?
            };

            // Route to appropriate operation based on relation
            let added = if relation_str == "admin" {
                nac_manager
                    .add_admin(&requestor, &target)
                    .await
                    .map_err(|e| normalize_auth_error(e.to_string(), "add-nac-relation"))?
            } else if let Some(perm) = NodePermission::parse(&relation_str) {
                // Grant specific permission
                nac_manager
                    .add_permission_grant(&requestor, &target, perm)
                    .await
                    .map_err(|e| normalize_auth_error(e.to_string(), "add-nac-relation"))?
            } else {
                return Err("relation not in resource".to_string());
            };

            Ok(serde_json::json!({ "ExistedAlready": !added }).to_string())
        })
    }
}

/// Delete a NAC actor relationship.
///
/// The requestor must be an admin. The owner cannot be removed.
///
/// Answers with Go's `DeleteActorRelationshipResult`:
/// ```json
/// { "RecordFound": true }  // false when there was no such relationship
/// ```
///
/// # Safety
///
/// All string parameters must be valid null-terminated UTF-8 strings.
/// Deletes the node access-control relation `relation` ("admin" for null or
/// empty) of `actor`, as the identity `identity_ptr` names.
///
/// # Safety
///
/// `relation` and `actor` must be null or valid null-terminated strings.
#[no_mangle]
pub unsafe extern "C" fn defra_acp_nac_relationship_delete(
    node_ptr: usize,
    identity_ptr: usize,
    relation: *const c_char,
    actor: *const c_char,
) -> DefraResult {
    ffi_entry! {
        let request = try_ffi!(Request::resolve(node_ptr, identity_ptr));
        let node_ptr = try_ffi!(request.require_node("node access control changes"));
        let requestor_str = request.identity.clone().unwrap_or_default();
        let target_did = actor;

        let relation_str = match c_str_to_string(relation) {
            Some(s) if !s.is_empty() => s,
            _ => "admin".to_string(),
        };

        let target_str = try_ffi!(require_c_str(target_did, "target_did"));

        let nac_manager = match NODES.get(node_ptr, |state| state.nac_manager.clone()) {
            Some(m) => m,
            None => return DefraResult::error(ERR_INVALID_NODE_HANDLE),
        };

        ffi_async!(node_ptr, {
            // Check NAC state before validating DID (Go checks state first)
            let status = nac_manager.status().await;
            if status == acp::nac::NacStatus::NotConfigured {
                return Err("node acp is not configured".to_string());
            }
            if status == acp::nac::NacStatus::DisabledTemporarily {
                return Err("operation requires ACP, but ACP not available".to_string());
            }

            // Empty or wildcard requestor: check if wildcard admin exists to decide error
            if requestor_str.is_empty() || requestor_str == "*" {
                let wildcard = identity::Did::wildcard();
                let wildcard_is_admin = nac_manager.is_admin(&wildcard).await.unwrap_or(false);
                if wildcard_is_admin {
                    return Err("node acp relationship operation requires identity".to_string());
                } else {
                    return Err(
                        format!("not authorized to perform operation. Permission: {}", NodePermission::NacRelationDelete),
                    );
                }
            }

            let requestor = identity::Did::new(&requestor_str).map_err(|_| {
                format!("not authorized to perform operation. Permission: {}", NodePermission::NacRelationDelete)
            })?;

            // Check admin authorization BEFORE validating target DID
            if !nac_manager.is_admin(&requestor).await.unwrap_or(false) {
                return Err(
                    format!("not authorized to perform operation. Permission: {}", NodePermission::NacRelationDelete),
                );
            }

            // Validate relation name against NAC policy
            if !is_valid_nac_relation(&relation_str) {
                return Err("relation not in resource".to_string());
            }

            // Owner relation cannot be modified
            if relation_str == "owner" {
                return Err("relation not in resource".to_string());
            }

            // Empty target with authorized requestor: Go returns {RecordFound: false}
            if target_str.is_empty() {
                return Ok(serde_json::json!({ "RecordFound": false }).to_string());
            }

            let target = if target_str == "*" {
                identity::Did::wildcard()
            } else {
                identity::Did::new(&target_str)
                    .map_err(|e| format!("invalid target DID '{}': {}", target_str, e))?
            };

            let deleted = if relation_str == "admin" {
                nac_manager
                    .remove_admin(&requestor, &target)
                    .await
                    .map_err(|e| normalize_auth_error(e.to_string(), "delete-nac-relation"))?
            } else if let Some(perm) = NodePermission::parse(&relation_str) {
                nac_manager
                    .remove_permission_grant(&requestor, &target, perm)
                    .await
                    .map_err(|e| normalize_auth_error(e.to_string(), "delete-nac-relation"))?
            } else {
                return Err("relation not in resource".to_string());
            };

            Ok(serde_json::json!({ "RecordFound": deleted }).to_string())
        })
    }
}
