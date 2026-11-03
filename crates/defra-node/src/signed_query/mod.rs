//! Queries the node runs as its own signing identity.
//!
//! The node's identity, signing config and batch session travel in the
//! request's context. A signed query runs as a task of the network runtime
//! ([`Edge::spawn`]), so it keeps running if its caller stops waiting.

use std::sync::Arc;

use defra_core::signing::SigningConfig;
use edge::{Edge, EdgeError};
use query::{QueryExecutor, QueryRequest, QueryResponse, TransactionHandle};

#[cfg(test)]
mod tests;

pub(super) async fn execute_with_signing_context(
    edge: &Edge,
    executor: Arc<dyn QueryExecutor>,
    mut request: QueryRequest,
    txn_handle: Option<TransactionHandle>,
    signing_config: SigningConfig,
    node_did: String,
) -> QueryResponse {
    let batch_session = Some(signing_config.public_key_hex.clone());
    request.context = request
        .context
        .with_identity(Some(node_did))
        .with_signing(Some(signing_config))
        .with_batch_session(batch_session);
    let context = request.context.clone();
    edge.spawn(context.mirror(async move {
        match txn_handle {
            Some(txn_handle) => executor.execute_in_txn(request, &txn_handle).await,
            None => super::execute_autocommit_request(executor.as_ref(), request).await,
        }
    }))
    .await
    .unwrap_or_else(refused_response)
}

/// The answer to a query its task never finished: refused because the node
/// is shutting down, or failed.
pub(super) fn refused_response(error: EdgeError) -> QueryResponse {
    match error {
        EdgeError::Closed => QueryResponse::error("the node is shutting down"),
        error => QueryResponse::error(format!("query execution task failed: {error}")),
    }
}

pub(super) fn unavailable_node_signer_response(node_did: &str) -> QueryResponse {
    QueryResponse::error(format!(
        "configured node signing identity {node_did} is unavailable"
    ))
}
