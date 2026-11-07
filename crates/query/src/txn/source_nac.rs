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
    let requestor = authenticated_actor(&actor_context)?;
    source_permission_job_for(actor_context, requestor, edge, SourceCaller::Request)
}

/// Prepare a source-grant turn for an embedder-authenticated actor on a
/// node-owned transaction. An exactly present add or exactly absent delete
/// changes nothing and needs no authority; every changing edge is checked as
/// that actor. Like the host turn, it then replays the projector's
/// descendants of the edge as that actor in the same statement, so a refused
/// descendant rolls the edge back. Never expose this as an HTTP or GraphQL
/// request parameter.
pub fn embedded_source_permission_job(
    actor_context: RequestContext,
    edge: SourcePermission,
) -> Result<TxnJob<Result<bool, TransactionError>>, TransactionError> {
    let requestor = authenticated_actor(&actor_context)?;
    source_permission_job_for(actor_context, requestor, edge, SourceCaller::Embedder)
}

fn authenticated_actor(actor_context: &RequestContext) -> Result<Did, TransactionError> {
    if !actor_context.authorization_valid_now() {
        return Err(TransactionError::operation(
            "request authorization has expired",
        ));
    }
    let requestor = actor_context
        .identity()
        .ok_or_else(|| TransactionError::operation("authenticated identity required"))?;
    Did::new(requestor).map_err(|error| TransactionError::operation(error.to_string()))
}

/// Prepare a source-only host transaction turn. The caller must be the trusted
/// embedded host; the transaction owner also supplies the native authority.
pub fn host_source_permission_job(
    owner_context: RequestContext,
    edge: SourcePermission,
) -> Result<TxnJob<Result<bool, TransactionError>>, TransactionError> {
    if !owner_context.authorization_valid_now() || owner_context.identity().is_none() {
        return Err(TransactionError::operation(
            "fresh authenticated transaction owner required",
        ));
    }
    let actor = Did::new(owner_context.identity().expect("identity checked above"))
        .map_err(|error| TransactionError::operation(error.to_string()))?;
    source_permission_job_for(owner_context, actor, edge, SourceCaller::Host)
}

/// Who admitted the turn. Only a trusted embedder or host may restate an
/// unchanged edge without authority, and only they replay the projector's
/// descendants; an untrusted caller must not learn whether an edge it cannot
/// administer exists.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SourceCaller {
    Request,
    Embedder,
    Host,
}

impl SourceCaller {
    fn trusted(self) -> bool {
        !matches!(self, Self::Request)
    }
}

fn source_permission_job_for(
    actor_context: RequestContext,
    requestor: Did,
    edge: SourcePermission,
    caller: SourceCaller,
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
                let unchanged = caller.trusted()
                    && fetcher
                        .source_node_permission_matches(
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
                let changed = if unchanged {
                    false
                } else {
                    fetcher
                        .change_source_node_permission(
                            &requestor,
                            permission,
                            edge.parent_source.as_deref(),
                            &edge.subject_kind,
                            &edge.subject,
                            edge.delete,
                        )
                        .await
                        .map_err(|error| match error {
                            crate::error::QueryError::PermissionDenied(_) => {
                                TransactionError::Storage(
                                    storage::corekv::Error::ProjectionPermissionDenied,
                                )
                            }
                            other => TransactionError::operation(other.to_string()),
                        })?
                        .ok_or_else(|| {
                            TransactionError::not_supported(
                                "transaction-bound node authorization is unavailable",
                            )
                        })?
                };
                if caller.trusted() {
                    fetcher
                        .project_host_source_change(&actor_context)
                        .await
                        .map_err(|error| match error {
                            crate::error::QueryError::PermissionDenied(_) => {
                                TransactionError::Storage(
                                    storage::corekv::Error::ProjectionPermissionDenied,
                                )
                            }
                            other => TransactionError::operation(other.to_string()),
                        })?;
                }
                Ok(changed)
            }
            .await;
            turn.end_statement(result.is_ok())?;
            result
        })
    }))
}
