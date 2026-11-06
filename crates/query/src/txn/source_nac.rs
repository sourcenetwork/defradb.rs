use defra_core::RequestContext;
use identity::Did;
use serde::{Deserialize, Serialize};

use super::{txn_job, TxnJob};
use crate::error::TransactionError;

/// One independently removable edge in a node permission's source graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourcePermission {
    pub permission: String,
    pub parent_source: Option<String>,
    pub subject_kind: String,
    pub subject: String,
    pub delete: bool,
}

/// Prepare one source-grant transaction turn for its authenticated actor.
pub fn source_permission_job(
    actor_context: RequestContext,
    edge: SourcePermission,
) -> Result<TxnJob<Result<bool, TransactionError>>, TransactionError> {
    if !actor_context.authorization_valid_now() {
        return Err(TransactionError::operation(
            "request authorization has expired",
        ));
    }
    let requestor = actor_context
        .identity()
        .ok_or_else(|| TransactionError::operation("authenticated identity required"))?;
    let requestor =
        Did::new(requestor).map_err(|error| TransactionError::operation(error.to_string()))?;
    source_permission_job_for(actor_context, requestor, edge, false)
}

/// Prepare a source-only host transaction turn. The caller must be the trusted
/// embedded host; the owner context still controls transaction admission.
pub fn host_source_permission_job(
    owner_context: RequestContext,
    node: Did,
    edge: SourcePermission,
) -> Result<TxnJob<Result<bool, TransactionError>>, TransactionError> {
    if !owner_context.authorization_valid_now() || owner_context.identity().is_none() {
        return Err(TransactionError::operation("fresh authenticated transaction owner required"));
    }
    source_permission_job_for(owner_context, node, edge, true)
}

fn source_permission_job_for(
    actor_context: RequestContext,
    requestor: Did,
    edge: SourcePermission,
    host: bool,
) -> Result<TxnJob<Result<bool, TransactionError>>, TransactionError> {
    let permission = acp::NodePermission::parse(&edge.permission)
        .ok_or_else(|| TransactionError::operation("invalid node permission"))?;
    Ok(txn_job(move |turn| {
        Box::pin(async move {
            if !actor_context.authorization_valid_now() || turn.is_readonly() {
                return Err(TransactionError::operation(
                    "a fresh writable transaction is required",
                ));
            }
            turn.begin_statement()?;
            let result = async {
                let ctx = turn.context()?;
                let fetcher = ctx.doc_fetcher();
                let changed = fetcher
                    .change_source_node_permission(
                        &requestor,
                        permission,
                        edge.parent_source.as_deref(),
                        &edge.subject_kind,
                        &edge.subject,
                        edge.delete,
                    )
                    .await
                    .map_err(|error| TransactionError::operation(error.to_string()))?
                    .ok_or_else(|| {
                        TransactionError::not_supported(
                            "transaction-bound node authorization is unavailable",
                        )
                    })?;
                if host && changed {
                    fetcher.project_host_source_change().await
                        .map_err(|error| TransactionError::operation(error.to_string()))?;
                }
                Ok(changed)
            }
            .await;
            turn.end_statement(result.is_ok())?;
            result
        })
    }))
}
