//! A relationship change inside a transaction: the caller's explicit one, or
//! one begun and committed for an autocommit change when document ACP joins
//! transactions. Every policy read and relationship write runs in the
//! transaction it authorizes.

use std::sync::Arc;

use defra_core::RequestContext;
use query::txn::{txn_job, TransactionHandle, TransactionRegistry};
use storage::corekv::{OnCommit, Store};

use super::settle::{publish_document_update, settle_owed_changes};
use super::{ChangeKind, DocRelationship, DocumentRelationships, RelationshipChange, Result};

/// The transaction begun for one autocommit relationship change. Unless it is
/// committed, dropping it abandons the transaction, including when the request
/// future is cancelled between its begin and commit.
struct AutocommitTxn<'a, S>
where
    S: Store + 'static,
{
    registry: &'a crate::DbTransactionRegistry<S>,
    handle: Option<TransactionHandle>,
}

impl<'a, S> AutocommitTxn<'a, S>
where
    S: Store + 'static,
{
    fn new(registry: &'a crate::DbTransactionRegistry<S>, handle: TransactionHandle) -> Self {
        Self {
            registry,
            handle: Some(handle),
        }
    }

    fn handle(&self) -> &TransactionHandle {
        self.handle.as_ref().expect("the transaction is not final")
    }

    fn committed(mut self) {
        self.handle = None;
    }
}

impl<S> Drop for AutocommitTxn<'_, S>
where
    S: Store + 'static,
{
    fn drop(&mut self) {
        if let Some(handle) = &self.handle {
            self.registry.abandon(handle);
        }
    }
}

/// Whether a native node administrator may change a relationship it does not
/// manage. Go's public routes refuse one; only a trusted embedder's route
/// admits it.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum NodeAdmin {
    Refused,
    Admitted,
}

impl<S> DocumentRelationships<S>
where
    S: Store + 'static,
{
    /// An autocommit change in a transaction begun for it, committed once
    /// the change is among its writes.
    pub(super) async fn autocommit(
        &self,
        context: &RequestContext,
        identity: &str,
        relationship: DocRelationship<'_>,
        delete: bool,
    ) -> Result<bool> {
        let handle = self.registry.begin_as(context, false).await?;
        let transaction = AutocommitTxn::new(self.registry.as_ref(), handle);
        let id: defra_core::TxnId = transaction
            .handle()
            .as_str()
            .parse()
            .map_err(|_| query::QueryError::execution("invalid transaction ID"))?;
        let txn_context = context.clone().with_txn(Some(id));
        let changed = self
            .in_txn(
                &txn_context,
                context,
                identity,
                relationship,
                delete,
                NodeAdmin::Refused,
            )
            .await;
        let changed = changed?.reported(delete);
        self.registry
            .commit_detached_as(context, transaction.handle())
            .await?;
        transaction.committed();
        Ok(changed)
    }

    pub(super) async fn in_txn(
        &self,
        transaction_context: &RequestContext,
        actor_context: &RequestContext,
        identity: &str,
        relationship: DocRelationship<'_>,
        delete: bool,
        node_admin: NodeAdmin,
    ) -> Result<RelationshipChange> {
        let DocRelationship {
            collection,
            doc_id,
            relation,
            target,
        } = relationship;
        if !transaction_context.authorization_valid_now()
            || !actor_context.authorization_valid_now()
            || match identity.is_empty() {
                true => actor_context.identity().is_some(),
                false => actor_context.identity() != Some(identity),
            }
            || actor_context
                .txn()
                .is_some_and(|id| Some(id) != transaction_context.txn())
        {
            return Err(query::QueryError::permission_denied(
                "request identity or authorization is invalid",
            )
            .into());
        }
        let txn = transaction_context
            .txn()
            .ok_or_else(|| query::QueryError::execution("an explicit transaction is required"))?;
        let handle = TransactionHandle::new(txn.to_string());
        let collection = collection.to_owned();
        let doc_id = doc_id.to_owned();
        let relation = relation.to_owned();
        let target = target.to_owned();
        let identity = identity.to_owned();
        let provider = acp::bound_to_request(&self.provider, actor_context);
        let unbound = Arc::clone(&self.provider);
        let database = Arc::clone(self.registry.db());
        let relationships = self.clone();
        let authorization = actor_context.clone();
        let transaction_authorization = transaction_context.clone();
        let change = ChangeKind::of(delete);

        let job = txn_job(move |turn| {
            Box::pin(async move {
                if !authorization.authorization_valid_now()
                    || !transaction_authorization.authorization_valid_now()
                {
                    return Err(query::QueryError::permission_denied(
                        "request authorization expired before the transaction turn",
                    )
                    .into());
                }
                turn.begin_statement()?;
                let result = async {
                    let ctx = turn.context()?;
                    let fetcher = ctx.doc_fetcher();
                    let collection_provider = ctx.collection_provider().ok_or_else(|| {
                        query::QueryError::execution("transaction-bound schema is unavailable")
                    })?;
                    let (actor, resolved) = relationships
                        .requestor_in_txn(
                            fetcher.as_ref(),
                            collection_provider.as_ref(),
                            change,
                            &identity,
                            &collection,
                            [&doc_id, &relation, &target],
                        )
                        .await?;
                    let wrap = change.wrap();
                    if turn.is_readonly() {
                        return Err(query::QueryError::execution(
                            "relationship writes require a writable transaction",
                        )
                        .into());
                    }
                    let Some(acp) = fetcher.transaction_acp(provider.clone()).await? else {
                        // A provider that cannot join the transaction is owed
                        // the change by it.
                        let outbox =
                            fetcher.transaction_outbox(provider).await?.ok_or_else(|| {
                                query::QueryError::execution(
                                    "transaction-bound document authorization is unavailable",
                                )
                            })?;
                        let first = outbox.staged() == 0;
                        let staged = outbox
                            .stage(acp::StageRelationship {
                                txn: Some(txn.route),
                                actor: &actor,
                                subject: resolved.target,
                                policy_id: &resolved.policy_id,
                                resource: &resolved.resource_name,
                                object: &doc_id,
                                relation: &relation,
                                managing_relations: &resolved.managing_relations,
                                op: if delete {
                                    acp::OwedOp::Delete
                                } else {
                                    acp::OwedOp::Add
                                },
                            })
                            .await
                            .map_err(|error| wrap(error.into()))?;
                        drop(outbox);
                        // One pass after the commit serves every change the
                        // transaction staged. A statement that is undone
                        // takes its hook and its count with it.
                        if first {
                            let staged_by = txn.route;
                            ctx.on_commit(OnCommit::Async(Box::new(move || {
                                Box::pin(settle_owed_changes(unbound, staged_by))
                            })))?;
                        }
                        return Ok(RelationshipChange::Staged {
                            seq: staged.seq,
                            authority_checked: staged.authority_checked,
                        });
                    };
                    let ordinary = if delete {
                        acp.delete_relationship(
                            &actor,
                            resolved.target.clone(),
                            &resolved.policy_id,
                            &resolved.resource_name,
                            &doc_id,
                            &relation,
                            &resolved.managing_relations,
                        )
                        .await
                    } else {
                        acp.add_relationship(
                            &actor,
                            resolved.target.clone(),
                            &resolved.policy_id,
                            &resolved.resource_name,
                            &doc_id,
                            &relation,
                            &resolved.managing_relations,
                        )
                        .await
                    };
                    let changed = match ordinary {
                        Ok(changed) => changed,
                        Err(
                            original
                            @ (acp::Error::NotOwner { .. } | acp::Error::NotManager { .. }),
                        ) if node_admin == NodeAdmin::Admitted => {
                            match fetcher
                                .change_node_admin_relationship(
                                    acp.clone(),
                                    &actor,
                                    resolved.target,
                                    &resolved.policy_id,
                                    &resolved.resource_name,
                                    &doc_id,
                                    &relation,
                                    delete,
                                )
                                .await
                            {
                                Ok(changed) => changed,
                                Err(
                                    acp::Error::PermissionDenied(_)
                                    | acp::Error::UnsupportedAdministrativeRelationship,
                                ) => return Err(wrap(original.into())),
                                Err(error) => return Err(wrap(error.into())),
                            }
                        }
                        Err(error) => return Err(wrap(error.into())),
                    };
                    if changed && !delete {
                        let collection_id = resolved.collection_id;
                        let doc = doc_id.clone();
                        ctx.on_commit(OnCommit::Async(Box::new(move || {
                            Box::pin(async move {
                                publish_document_update(&database, &collection_id, &doc).await
                            })
                        })))?;
                    }
                    Ok(RelationshipChange::Applied {
                        existed: if delete { changed } else { !changed },
                    })
                }
                .await;
                turn.end_statement(result.is_ok())?;
                result
            })
        });
        self.registry
            .run_as(transaction_context, &handle, None, job)
            .await?
    }
}

#[cfg(test)]
#[path = "../../../../tests/access/autocommit_relationship.rs"]
mod tests;
