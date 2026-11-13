use std::ffi::c_char;

use crate::request::Request;
use crate::state::NODES;
use crate::types::{c_str_to_string, DefraResult};
use crate::{ffi_async, ffi_async_ok, ffi_entry, try_ffi, ERR_INVALID_NODE_HANDLE};

/// Answers with the node access-control status as JSON, Go's
/// `GetNACStatus`; the core checks `NacStatus` for the identity
/// `identity_ptr` names.
///
/// ```json
/// {
///   "status": "enabled" | "disabled temporarily" | "not configured",
///   "configured_enabled": true | false,
///   "dev_mode": true | false,
///   "owner": "did:key:..." | null
/// }
/// ```
#[no_mangle]
pub extern "C" fn defra_acp_nac_status(node_ptr: usize, identity_ptr: usize) -> DefraResult {
    ffi_entry! {
        let request = try_ffi!(Request::resolve(node_ptr, identity_ptr));
        let node_ptr = request.node();
        let context = request.context();
        let runner = try_ffi!(request.runner());
        let nac_manager = match NODES.get(node_ptr, |state| state.nac_manager.clone()) {
            Some(m) => m,
            None => return DefraResult::error(ERR_INVALID_NODE_HANDLE),
        };

        ffi_async!(node_ptr, {
            runner.nac_status_as(&context).await.map_err(|e| e.to_string())?;
            let info = nac_manager.info().await;
            serde_json::to_string(&info).map_err(|e| format!("failed to serialize NAC info: {}", e))
        })
    }
}

/// Disables node access control temporarily, Go's `DisableNAC`, as the
/// identity `identity_ptr` names; the core checks the caller.
#[no_mangle]
pub extern "C" fn defra_acp_nac_disable(node_ptr: usize, identity_ptr: usize) -> DefraResult {
    ffi_entry! {
        let request = try_ffi!(Request::resolve(node_ptr, identity_ptr));
        let node_ptr = try_ffi!(request.require_node("node access control changes"));
        let context = request.context();
        let runner = try_ffi!(request.runner());
        ffi_async_ok!(node_ptr, {
            runner.disable_nac_as(&context).await.map_err(|e| e.to_string())
        })
    }
}

/// Re-enables a disabled node access control, Go's `ReEnableNAC`, as the
/// identity `identity_ptr` names; the core checks the caller against the
/// stored relationships.
#[no_mangle]
pub extern "C" fn defra_acp_nac_re_enable(node_ptr: usize, identity_ptr: usize) -> DefraResult {
    ffi_entry! {
        let request = try_ffi!(Request::resolve(node_ptr, identity_ptr));
        let node_ptr = try_ffi!(request.require_node("node access control changes"));
        let context = request.context();
        let runner = try_ffi!(request.runner());
        ffi_async_ok!(node_ptr, {
            runner.re_enable_nac_as(&context).await.map_err(|e| e.to_string())
        })
    }
}

/// The relation a relationship call names; `admin` when it names none.
///
/// # Safety
///
/// `relation` must be null or a valid null-terminated string.
unsafe fn relation_or_admin(relation: *const c_char) -> String {
    match c_str_to_string(relation) {
        Some(relation) if !relation.is_empty() => relation,
        _ => "admin".to_string(),
    }
}

/// Adds the node access-control relation `relation` ("admin" for null or
/// empty) of `actor`, Go's `AddNACActorRelationship`, as the identity
/// `identity_ptr` names; the core checks the caller and the relation.
///
/// Answers with Go's `AddActorRelationshipResult`:
/// ```json
/// { "ExistedAlready": false }  // true when the relationship already existed
/// ```
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
        let context = request.context();
        let runner = try_ffi!(request.runner());
        let relation = relation_or_admin(relation);
        let actor = c_str_to_string(actor).unwrap_or_default();
        ffi_async!(node_ptr, {
            let existed = runner
                .add_nac_actor_relationship_as(&context, &relation, &actor)
                .await
                .map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "ExistedAlready": existed }).to_string())
        })
    }
}

/// Deletes the node access-control relation `relation` ("admin" for null or
/// empty) of `actor`, Go's `DeleteNACActorRelationship`, as the identity
/// `identity_ptr` names; the core checks the caller and the relation.
///
/// Answers with Go's `DeleteActorRelationshipResult`:
/// ```json
/// { "RecordFound": true }  // false when there was no such relationship
/// ```
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
        let context = request.context();
        let runner = try_ffi!(request.runner());
        let relation = relation_or_admin(relation);
        let actor = c_str_to_string(actor).unwrap_or_default();
        ffi_async!(node_ptr, {
            let deleted = runner
                .delete_nac_actor_relationship_as(&context, &relation, &actor)
                .await
                .map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "RecordFound": deleted }).to_string())
        })
    }
}

/// Purges every node access-control state, Go's `PurgeNACState`, as the
/// identity `identity_ptr` names; the core allows it in development mode
/// only, for an admin. An embedded node runs node access control in
/// development mode, so an admin may.
#[no_mangle]
pub extern "C" fn defra_acp_nac_purge(node_ptr: usize, identity_ptr: usize) -> DefraResult {
    ffi_entry! {
        let request = try_ffi!(Request::resolve(node_ptr, identity_ptr));
        let node_ptr = try_ffi!(request.require_node("node access control changes"));
        let Some(requestor) = try_ffi!(request.did()) else {
            return DefraResult::error("purging node access control requires an identity");
        };
        let nac_manager = match NODES.get(node_ptr, |state| state.nac_manager.clone()) {
            Some(m) => m,
            None => return DefraResult::error(ERR_INVALID_NODE_HANDLE),
        };
        ffi_async_ok!(node_ptr, {
            nac_manager.purge(&requestor).await.map_err(|e| e.to_string())
        })
    }
}
