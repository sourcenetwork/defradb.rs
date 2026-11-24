//! P2P: peers, replicators, and the collections and documents a node
//! replicates.
//!
//! Every call works on a node (not a transaction) and runs its P2P
//! operation on the network runtime, as the identity it names.

mod collection;
mod document;
mod mobile;
mod peer;
mod replicator;

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use acp::nac::NodePermission;
use defra_p2p_adapter::P2POperations;

use crate::request::Request;
use crate::state::NODES;
use crate::target::{go_string, ERR_INVALID_HANDLE};
use crate::types::DefraResult;

/// What a P2P call on a node without P2P reports.
const ERR_NO_P2P: &str = "no p2p system configured";

/// What a call whose timeout passes reports, as Go's context does.
const ERR_DEADLINE: &str = "context deadline exceeded";

/// Runs the P2P operation `operation` builds for node `node_ptr`, as the
/// identity `identity_ptr` names, admitted for `permission`; with `timeout`,
/// it fails once that passes.
fn run<M, Fut, T, E>(
    node_ptr: usize,
    identity_ptr: usize,
    permission: NodePermission,
    timeout: Option<Duration>,
    operation: M,
) -> Result<T, DefraResult>
where
    M: FnOnce(Arc<dyn P2POperations>) -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let ops =
        admit(node_ptr, identity_ptr, permission)?.ok_or_else(|| DefraResult::error(ERR_NO_P2P))?;
    drive(ops, timeout, operation)
}

/// The P2P operations of node `node_ptr` for the identity `identity_ptr`
/// names, once admitted for `permission`; none when the node runs without
/// P2P.
fn admit(
    node_ptr: usize,
    identity_ptr: usize,
    permission: NodePermission,
) -> Result<Option<Arc<dyn P2POperations>>, DefraResult> {
    let request = Request::begin(node_ptr, identity_ptr, permission)?;
    // Go's P2P calls take a node, never a transaction.
    let node = request.require_node("P2P")?;
    let context = request.context();
    NODES
        .get(node, |state| {
            state.p2p.as_ref().map(|p2p| p2p.system.ops_for(&context))
        })
        .ok_or_else(|| DefraResult::error(ERR_INVALID_HANDLE))
}

/// Runs the operation `operation` builds on `ops`; with `timeout`, it fails
/// once that passes.
fn drive<M, Fut, T, E>(
    ops: Arc<dyn P2POperations>,
    timeout: Option<Duration>,
    operation: M,
) -> Result<T, DefraResult>
where
    M: FnOnce(Arc<dyn P2POperations>) -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let rt = crate::helpers::get_rt()?;
    let result = crate::helpers::block_on(rt, async {
        let call = operation(ops);
        match timeout {
            None => Ok(call.await),
            Some(limit) => rt.timeout(limit, call).await,
        }
    });
    match result {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => Err(DefraResult::error(error.to_string())),
        Err(edge::EdgeError::Deadline(_)) => Err(DefraResult::error(ERR_DEADLINE)),
        Err(error) => Err(DefraResult::error(error.to_string())),
    }
}

/// A P2P operation's success with no value, or its error.
fn ok(result: Result<(), DefraResult>) -> DefraResult {
    result
        .map(|()| DefraResult::ok())
        .unwrap_or_else(|error| error)
}

/// A P2P operation's value as JSON, or its error.
fn json<T>(result: Result<T, DefraResult>) -> DefraResult
where
    T: serde::Serialize,
{
    match result {
        Ok(value) => DefraResult::from(serde_json::to_string(&value).map_err(|e| e.to_string())),
        Err(error) => error,
    }
}

/// A Go duration string, null or empty for none.
///
/// # Safety
///
/// `timeout` must be null or a valid null-terminated string.
unsafe fn timeout(timeout: *const std::ffi::c_char) -> Result<Option<Duration>, DefraResult> {
    let timeout = go_string(timeout);
    if timeout.is_empty() {
        return Ok(None);
    }
    let nanos = crate::duration::parse_go_duration(&timeout).map_err(DefraResult::error)?;
    Ok((nanos > 0).then(|| Duration::from_nanos(nanos as u64)))
}

pub use collection::{
    defra_p2p_collection_add, defra_p2p_collection_delete, defra_p2p_collection_list,
    defra_p2p_collection_sync_branchable, defra_p2p_collection_sync_versions,
};
pub use document::{
    defra_p2p_document_add, defra_p2p_document_delete, defra_p2p_document_list,
    defra_p2p_document_sync,
};
pub use mobile::{
    defra_mobile_notify_network_change, defra_mobile_peer_details, defra_mobile_shareable_address,
    defra_mobile_sync_status,
};
pub use peer::{defra_p2p_connect, defra_p2p_disconnect, defra_p2p_info, defra_p2p_peer_list};
pub use replicator::{
    defra_p2p_replicator_add, defra_p2p_replicator_add_with_filter, defra_p2p_replicator_delete,
    defra_p2p_replicator_list, defra_p2p_replicator_retry,
};

#[cfg(test)]
mod tests;
