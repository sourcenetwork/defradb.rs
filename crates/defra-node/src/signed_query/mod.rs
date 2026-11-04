//! Queries a signing node runs with a resolved signer.
//!
//! The acting identity, signing config and batch session travel in the
//! request's context. A signed query runs as a task of the network runtime
//! ([`Edge::spawn`]), so it keeps running if its caller stops waiting.

use std::sync::Arc;

use defra_core::signing::SigningConfig;
use edge::{Edge, EdgeError};
use query::{QueryExecutor, QueryRequest, QueryResponse, TransactionHandle};

#[cfg(test)]
mod tests;

/// `request` run as the node identity `node_did`, signing with
/// `signing_config`.
pub(super) async fn execute_with_signing_context(
    edge: &Edge,
    executor: Arc<dyn QueryExecutor>,
    mut request: QueryRequest,
    txn_handle: Option<TransactionHandle>,
    signing_config: SigningConfig,
    node_did: String,
) -> QueryResponse {
    request.context = request.context.with_identity(Some(node_did));
    execute_signed(edge, executor, request, txn_handle, signing_config).await
}

/// `request` run as the identity its context acts as, signing with
/// `signing_config`. Its signatures are prepared on this network-runtime
/// task, outside database workers.
pub(super) async fn execute_signed(
    edge: &Edge,
    executor: Arc<dyn QueryExecutor>,
    mut request: QueryRequest,
    txn_handle: Option<TransactionHandle>,
    signing_config: SigningConfig,
) -> QueryResponse {
    request.context = request.context.with_signing_session(Some(signing_config));
    let context = request.context.clone();
    edge.spawn(context.mirror(async move {
        match txn_handle {
            Some(txn_handle) => executor.execute_in_txn(request, &txn_handle).await,
            None => super::execute_autocommit_request(executor.as_ref(), request).await,
        }
    }))
    .await
    .unwrap_or_else(|error| match error {
        EdgeError::Closed => QueryResponse::error("the node is shutting down"),
        error => QueryResponse::error(format!("query execution task failed: {error}")),
    })
}

pub(super) fn unavailable_node_signer_response(node_did: &str) -> QueryResponse {
    QueryResponse::error(format!(
        "configured node signing identity {node_did} is unavailable"
    ))
}
