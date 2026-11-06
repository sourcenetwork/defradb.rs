use std::sync::Arc;

use defra_core::RequestContext;
use identity::Did;
use query::txn::{txn_job, TransactionHandle, TransactionRegistry};
use storage::corekv::{OnCommit, Store};

use super::DbAcpOps;
use crate::{Error, Result};

pub(crate) struct Relationship<'a> {
    pub(crate) collection: &'a str,
    pub(crate) doc_id: &'a str,
    pub(crate) relation: &'a str,
    pub(crate) target: &'a str,
}

impl<S> DbAcpOps<S>
where
    S: Store + 'static,
{
    pub(super) async fn relationship_in_txn(
        &self,
        transaction_context: &RequestContext,
        actor_context: &RequestContext,
        identity: &str,
        relationship: Relationship<'_>,
        delete: bool,
    ) -> Result<bool> {
        let Relationship {
            collection,
            doc_id,
            relation,
            target,
        } = relationship;
        if !transaction_context.authorization_valid_now()
            || !actor_context.authorization_valid_now()
            || actor_context.identity() != Some(identity)
            || actor_context
                .txn()
                .is_some_and(|id| Some(id) != transaction_context.txn())
        {
            return Err(query::QueryError::permission_denied(
                "request identity or authorization is invalid",
            )
            .into());
        }
        if relation == acp::OWNER_RELATION {
            return Err(if delete {
                Error::OwnerRelationDelete
            } else {
                Error::OwnerRelationAdd
            });
        }
        let actor = Did::new(identity).map_err(Error::InvalidIdentityDid)?;
        let target = acp::parse_target_subject(target).map_err(Error::InvalidTarget)?;
        let txn = transaction_context
            .txn()
            .ok_or_else(|| query::QueryError::execution("an explicit transaction is required"))?;
        let handle = TransactionHandle::new(txn.to_string());
        let collection = collection.to_owned();
        let doc_id = doc_id.to_owned();
        let relation = relation.to_owned();
        let provider = acp::bound_to_request(&self.document_acp, actor_context);
        let database = Arc::clone(&self.database);
        let event_bus = Arc::clone(&self.event_bus);
        let nac_configured = database.nac_manager().is_some();
        let node_did = database.node_did();
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
                        "relationship writes require a writable transaction",
                    )
                    .into());
                }
                turn.begin_statement()?;
                let result = async {
                    let ctx = turn.context()?;
                    let fetcher = ctx.doc_fetcher();
                    if nac_configured && node_did.as_ref() != Some(&actor) {
                        let permission = if delete {
                            acp::NodePermission::DacRelationDelete
                        } else {
                            acp::NodePermission::DacRelationAdd
                        };
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
                    let acp = fetcher.transaction_acp(provider).await?.ok_or_else(|| {
                        query::QueryError::execution(
                            "transaction-bound document authorization is unavailable",
                        )
                    })?;
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
                    let policy = fetcher
                        .transaction_dac_policy(&binding.id)
                        .await?
                        .ok_or_else(|| Error::PolicyNotFound(binding.id.clone()))?;
                    let resource = policy
                        .get_resource(&binding.resource_name)
                        .ok_or_else(|| Error::ResourceNotFound(binding.resource_name.clone()))?;
                    if resource.get_relation(&relation).is_none() {
                        return Err(Error::RelationNotFound {
                            relation: relation.clone(),
                            resource: binding.resource_name.clone(),
                        });
                    }
                    let managers = policy
                        .get_managers_for_relation(&binding.resource_name, &relation)
                        .into_iter()
                        .map(str::to_owned)
                        .collect::<Vec<_>>();
                    let ordinary = if delete {
                        acp.delete_relationship(
                            &actor,
                            target.clone(),
                            &binding.id,
                            &binding.resource_name,
                            &doc_id,
                            &relation,
                            &managers,
                        )
                        .await
                    } else {
                        acp.add_relationship(
                            &actor,
                            target.clone(),
                            &binding.id,
                            &binding.resource_name,
                            &doc_id,
                            &relation,
                            &managers,
                        )
                        .await
                    };
                    let changed = match ordinary {
                        Ok(changed) => changed,
                        Err(original @ (acp::Error::NotOwner { .. } | acp::Error::NotManager { .. })) => {
                            match fetcher.change_node_admin_relationship(
                                acp.clone(), &actor, target, &binding.id,
                                &binding.resource_name, &doc_id, &relation, delete,
                            ).await {
                                Ok(changed) => changed,
                                Err(acp::Error::PermissionDenied(_) | acp::Error::UnsupportedAdministrativeRelationship) => return Err(original.into()),
                                Err(error) => return Err(error.into()),
                            }
                        }
                        Err(error) => return Err(error.into()),
                    };
                    if changed && !delete {
                        let collection_id = definition.collection_id.clone();
                        let doc = doc_id.clone();
                        ctx.on_commit(OnCommit::Async(Box::new(move || {
                            Box::pin(async move {
                                match db::block::reader::read_latest_composite_block(
                                    &database, &doc,
                                )
                                .await
                                {
                                    Ok(result) => event_bus.publish(events::Message::update(
                                        events::Update::new(
                                            doc,
                                            result.cid,
                                            collection_id,
                                            result.block,
                                            false,
                                            false,
                                        ),
                                    )),
                                    Err(error) => tracing::warn!(
                                        doc_id = doc,
                                        error,
                                        "failed to publish document update after DAC grant"
                                    ),
                                }
                            })
                        })))?;
                    }
                    Ok(if delete { changed } else { !changed })
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
