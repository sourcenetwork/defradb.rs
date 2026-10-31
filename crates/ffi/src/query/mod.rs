//! Query execution for FFI.
//!
//! This module exposes GraphQL query execution that matches
//! Go's cbindings/query.go behavior.

mod exec;
mod subscription;

#[cfg(test)]
mod tests;

#[cfg(test)]
use std::ffi::c_char;

use acp::nac::NodePermission;

use crate::state::NODES;
#[cfg(test)]
use crate::types::c_str_to_string;

// Re-export all public items so external imports remain unchanged.
pub(crate) use exec::run_mutation;
pub use exec::{exec_request, exec_request_async, exec_request_with_signing};
pub(crate) use subscription::{
    subscription_accepts_doc_id, subscription_doc_ids, subscription_to_scoped_query,
};

/// Determine NAC permission based on query content.
/// Delete mutations require DocumentDelete, other mutations require DocumentUpdate,
/// and queries require CollectionGet (matching Go's cbindings/query.go).
pub(crate) fn nac_permission_for_query(query_str: &str) -> NodePermission {
    let trimmed = query_str.trim_start();
    if trimmed.starts_with("mutation") {
        if let Some(brace_pos) = trimmed.find('{') {
            let after_brace = trimmed[brace_pos + 1..].trim_start();
            if after_brace.starts_with("delete_") {
                return NodePermission::DocumentDelete;
            }
        }
        NodePermission::DocumentUpdate
    } else {
        NodePermission::CollectionGet
    }
}

/// Whether the identity has the `DacBypass` NAC permission (NAC
/// admin/owner), the request's DAC bypass.
#[cfg(test)]
pub(crate) fn resolve_dac_bypass(node_ptr: usize, identity_did: *const c_char) -> bool {
    // SAFETY: `identity_did` is either null or a valid C string from the FFI caller.
    let identity = unsafe { c_str_to_string(identity_did) };
    // A check that cannot wait here leaves the bypass off.
    crate::call::wait(async { Ok(dac_bypass(node_ptr, identity.as_deref()).await) }).unwrap_or_else(
        |error| {
            tracing::warn!(%error, "could not resolve DAC bypass; leaving it off");
            false
        },
    )
}

/// [`resolve_dac_bypass`] for a caller that awaits it.
pub(crate) async fn dac_bypass(node_ptr: usize, identity: Option<&str>) -> bool {
    let did = match identity.filter(|s| !s.is_empty()) {
        Some(s) => match identity::Did::new(s) {
            Ok(d) => Some(d),
            Err(_) => return false,
        },
        None => None,
    };
    let Some(nac_manager) = NODES.get(node_ptr, |state| state.nac_manager.clone()) else {
        return false;
    };
    let status = nac_manager.status().await;
    acp::nac::should_bypass_dac(status, did.as_ref(), |d, p| async move {
        nac_manager.check_permission(d, p).await.unwrap_or(false)
    })
    .await
}

/// [`resolve_dac_bypass`] installed in the thread-local DAC bypass, as the FFI
/// entry points did before the bypass travelled in the request's context;
/// kept, test-only, for the tests written against it.
#[cfg(test)]
pub(crate) fn check_and_set_dac_bypass(node_ptr: usize, identity_did: *const c_char) {
    defra_core::dac_bypass::set_dac_bypass(resolve_dac_bypass(node_ptr, identity_did));
}
