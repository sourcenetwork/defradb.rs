//! NAC (Node Access Control) permission checking for FFI functions.
//!
//! Provides a reusable helper that FFI functions call before executing
//! their operations. When NAC is not configured or disabled, all
//! operations pass through. When enabled, the caller's identity is
//! checked against the required permission.

use std::ffi::c_char;
use std::sync::Arc;

use acp::nac::{NacStatus, NodePermission};

use crate::state::{FfiNacManager, NODES};
use crate::types::{c_str_to_string, DefraResult};
use crate::ERR_INVALID_NODE_HANDLE;

/// Check NAC permission for an FFI operation.
///
/// Returns `Ok(())` if authorized, `Err(DefraResult)` if denied.
///
/// - NAC not configured or disabled -> always Ok
/// - NAC enabled + no identity -> Err (not authorized)
/// - NAC enabled + identity lacks permission -> Err (not authorized)
/// - NAC enabled + identity has permission -> Ok
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub fn check_nac_permission(
    nac_manager: &Arc<FfiNacManager>,
    identity_did: *const c_char,
    permission: NodePermission,
) -> Result<(), DefraResult> {
    // SAFETY: `identity_did` is either null or a valid C string from the FFI caller.
    let identity = unsafe { c_str_to_string(identity_did) };
    wait(admit(nac_manager.as_ref(), identity.as_deref(), permission))
}

/// Convenience: extract NAC manager from node and check permission.
///
/// Returns `Ok(())` if authorized, `Err(DefraResult)` if the node handle
/// is invalid or the caller is not authorized.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub fn check_nac_for_node(
    node_ptr: usize,
    identity_did: *const c_char,
    permission: NodePermission,
) -> Result<(), DefraResult> {
    // SAFETY: `identity_did` is either null or a valid C string from the FFI caller.
    let identity = unsafe { c_str_to_string(identity_did) };
    wait(admit_on_node(node_ptr, identity.as_deref(), permission))
}

/// Whether `identity` may perform `permission` on node `node_ptr`, waited
/// for on the caller's thread.
pub(crate) fn check(
    node_ptr: usize,
    identity: Option<&str>,
    permission: NodePermission,
) -> Result<(), DefraResult> {
    wait(admit_on_node(node_ptr, identity, permission))
}

/// [`check_nac_for_node`] for a caller that awaits it.
pub(crate) async fn admit_on_node(
    node_ptr: usize,
    identity: Option<&str>,
    permission: NodePermission,
) -> Result<(), DefraResult> {
    let nac_manager = NODES
        .get(node_ptr, |state| state.nac_manager.clone())
        .ok_or_else(|| DefraResult::error(ERR_INVALID_NODE_HANDLE))?;
    admit(nac_manager.as_ref(), identity, permission).await
}

/// Whether `identity` may perform `permission`. Check runtime NAC status
/// directly rather than is_enabled(), because is_enabled() also requires
/// config.enabled which is a startup flag. FFI nodes start with
/// config.enabled=false and enable NAC at runtime via enable_nac(). An empty
/// identity is checked as the wildcard; a malformed one is denied.
async fn admit(
    nac_manager: &FfiNacManager,
    identity: Option<&str>,
    permission: NodePermission,
) -> Result<(), DefraResult> {
    if nac_manager.status().await != NacStatus::Enabled {
        return Ok(());
    }
    let did = match identity.filter(|s| !s.is_empty()) {
        Some(s) => identity::Did::new(s).ok(),
        None => Some(identity::Did::wildcard()),
    };
    let allowed = match did {
        Some(did) => nac_manager
            .check_permission(&did, permission)
            .await
            .unwrap_or(false),
        None => false,
    };
    if allowed {
        Ok(())
    } else {
        Err(DefraResult::error(format!(
            "not authorized to perform operation. Permission: {}",
            permission.as_str()
        )))
    }
}

/// The check, waited for on the caller's thread; a check that cannot wait
/// there fails with its reason.
fn wait<F>(check: F) -> Result<(), DefraResult>
where
    F: std::future::Future<Output = Result<(), DefraResult>>,
{
    crate::call::wait(async { Ok(check.await) }).map_err(DefraResult::error)?
}
