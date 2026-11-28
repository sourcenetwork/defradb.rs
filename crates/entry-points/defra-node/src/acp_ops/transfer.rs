//! Giving a document to another owner inside a node-owned transaction.
//!
//! With document ACP that joins the transaction the owner tuple is replaced
//! there, and the change commits or rolls back with the transaction's rows.
//! With a remote provider (Vera) the transfer is owed in the transaction,
//! behind the document's registration and the transaction's earlier
//! changes on it, and sent after the commit (`proofs/tla/OwnerTransfer.tla`).

use std::sync::Arc;

use defra_core::RequestContext;
use identity::Did;
use query::txn::{txn_job, TransactionHandle, TransactionRegistry};
use storage::corekv::{OnCommit, Store};

use super::DbAcpOps;
use crate::{Error, Result};

/// The document a transfer gives away, and to whom.
pub(crate) struct Transfer<'a> {
    pub(crate) collection: &'a str,
    pub(crate) doc_id: &'a str,
    pub(crate) new_owner: &'a str,
}

impl<S> DbAcpOps<S>
where
    S: Store + 'static,
{
    pub(super) async fn transfer_in_txn(
        &self,
        transaction_context: &RequestContext,
        actor_context: &RequestContext,
        transfer: Transfer<'_>,
    ) -> Result<()> {
        let identity = actor_context.identity().ok_or_else(|| {
            query::QueryError::permission_denied("an authenticated document owner is required")
        })?;
        let actor = Did::new(identity).map_err(Error::InvalidIdentityDid)?;
        if actor.is_wildcard() {
            return Err(query::QueryError::permission_denied(
                "an explicit document owner is required",
            )
            .into());
        }
        if !transaction_context.authorization_valid_now()
            || !actor_context.authorization_valid_now()
            || actor_context
                .txn()
                .is_some_and(|id| Some(id) != transaction_context.txn())
        {
            return Err(query::QueryError::permission_denied(
                "request identity or authorization is invalid",
            )
            .into());
        }
        // `*`, every actor, is no DID: nobody is given the document.
        let new_owner = Did::new(transfer.new_owner).map_err(Error::InvalidNewOwner)?;
        let txn = transaction_context
            .txn()
            .ok_or_else(|| query::QueryError::execution("an explicit transaction is required"))?;
        let handle = TransactionHandle::new(txn.to_string());
        let collection = transfer.collection.to_owned();
        let doc_id = transfer.doc_id.to_owned();
        let provider = acp::bound_to_request(&self.document_acp, actor_context);
        let unbound = Arc::clone(&self.document_acp);
        let nac_configured = self.database.nac_manager().is_some();
        let node_did = self.database.node_did();
        let authorization = actor_context.clone();
        let transaction_authorization = transaction_context.clone();

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
                if turn.is_readonly() {
                    return Err(query::QueryError::execution(
                        "ownership transfers require a writable transaction",
                    )
                    .into());
                }
                turn.begin_statement()?;
                let result = async {
                    let ctx = turn.context()?;
                    let fetcher = ctx.doc_fetcher();
                    // A transfer deletes one owner tuple and adds another.
                    if nac_configured && node_did.as_ref() != Some(&actor) {
                        for permission in [
                            acp::NodePermission::DacRelationAdd,
                            acp::NodePermission::DacRelationDelete,
                        ] {
                            match fetcher
                                .check_bound_node_permission(&actor, permission, false)
                                .await?
                            {
                                Some(true) => {}
                                Some(false) => {
                                    return Err(query::QueryError::permission_denied(
                                        "node relationship permission denied",
                                    )
                                    .into())
                                }
                                None => {
                                    return Err(query::QueryError::execution(
                                        "transaction-bound node authorization is unavailable",
                                    )
                                    .into())
                                }
                            }
                        }
                    }
                    let collection_provider = ctx.collection_provider().ok_or_else(|| {
                        query::QueryError::execution("transaction-bound schema is unavailable")
                    })?;
                    let definition = collection_provider
                        .get_collection(&collection)
                        .await?
                        .ok_or_else(|| Error::CollectionMissing(collection.clone()))?;
                    let binding = definition
                        .policy
                        .as_ref()
                        .ok_or(Error::CollectionWithoutPolicy)?;
                    fetcher
                        .transaction_dac_policy(&binding.id)
                        .await?
                        .ok_or_else(|| Error::PolicyNotFound(binding.id.clone()))?
                        .get_resource(&binding.resource_name)
                        .ok_or_else(|| Error::ResourceNotFound(binding.resource_name.clone()))?;
                    if let Some(acp) = fetcher.transaction_acp(provider.clone()).await? {
                        acp.transfer_doc_object(
                            &actor,
                            &binding.id,
                            &binding.resource_name,
                            &doc_id,
                            &new_owner,
                        )
                        .await?;
                        return Ok(());
                    }
                    // A provider that cannot join the transaction is owed
                    // the transfer by it.
                    let outbox = fetcher.transaction_outbox(provider).await?.ok_or_else(|| {
                        query::QueryError::execution(
                            "transaction-bound document authorization is unavailable",
                        )
                    })?;
                    let first = outbox.staged() == 0;
                    let staged = outbox
                        .stage_transfer(acp::StageTransfer {
                            txn: Some(txn.route),
                            actor: &actor,
                            new_owner: &new_owner,
                            policy_id: &binding.id,
                            resource: &binding.resource_name,
                            object: &doc_id,
                        })
                        .await?;
                    drop(outbox);
                    // One pass after the commit serves every change the
                    // transaction staged, as for a relationship change.
                    if first && staged.is_some() {
                        let staged_by = txn.route;
                        ctx.on_commit(OnCommit::Async(Box::new(move || {
                            Box::pin(super::transactional::settle_owed_changes(
                                unbound, staged_by,
                            ))
                        })))?;
                    }
                    Ok(())
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
