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

/// What a relationship change made inside a transaction did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelationshipChange {
    /// Document ACP joined the transaction and the change is among its
    /// writes: it takes effect, for every reader, when the transaction
    /// commits. `existed` says whether the relationship was there before the
    /// change: an add of one that existed changed nothing, and so did a
    /// delete of one that did not.
    Applied { existed: bool },
    /// Document ACP is a remote provider that cannot join a transaction. The
    /// change is recorded in the transaction as owed to the provider, at
    /// place `seq` of the node's queue, and is submitted after the commit.
    /// Whether the relationship exists on the provider is not known here.
    Staged {
        seq: u64,
        /// Whether the acting DID's authority was checked before staging.
        /// `false` for an add staged while the provider could not be
        /// reached: the provider decides at submission, and the change is
        /// removed if it refuses.
        authority_checked: bool,
    },
}

impl RelationshipChange {
    /// What an add answers as a boolean: whether the relationship existed
    /// already. A staged add answers `false`: it is reported as new.
    pub fn existed_already(self) -> bool {
        match self {
            Self::Applied { existed } => existed,
            Self::Staged { .. } => false,
        }
    }

    /// What a delete answers as a boolean: whether a relationship was found
    /// to delete. A staged delete answers `true`: it is reported as found.
    pub fn record_found(self) -> bool {
        match self {
            Self::Applied { existed } => existed,
            Self::Staged { .. } => true,
        }
    }

    /// The boolean of the change's kind.
    pub(crate) fn reported(self, delete: bool) -> bool {
        if delete {
            self.record_found()
        } else {
            self.existed_already()
        }
    }
}

/// Whether a native node administrator may change a relationship it does not
/// manage. Go's public routes refuse one; only a trusted embedder's route
/// admits it.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum NodeAdmin {
    Refused,
    Admitted,
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
        node_admin: NodeAdmin,
    ) -> Result<RelationshipChange> {
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
        let unbound = Arc::clone(&self.document_acp);
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
                                subject: target,
                                policy_id: &binding.id,
                                resource: &binding.resource_name,
                                object: &doc_id,
                                relation: &relation,
                                managing_relations: &managers,
                                op: if delete {
                                    acp::OwedOp::Delete
                                } else {
                                    acp::OwedOp::Add
                                },
                            })
                            .await?;
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
                        Err(
                            original
                            @ (acp::Error::NotOwner { .. } | acp::Error::NotManager { .. }),
                        ) if node_admin == NodeAdmin::Admitted => {
                            match fetcher
                                .change_node_admin_relationship(
                                    acp.clone(),
                                    &actor,
                                    target,
                                    &binding.id,
                                    &binding.resource_name,
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
                                ) => return Err(original.into()),
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

/// One exact relationship observation, owned so it can cross into a
/// transaction turn.
pub(crate) struct Observation {
    pub(crate) requester: Did,
    pub(crate) target: Did,
    pub(crate) policy: String,
    pub(crate) resource: String,
    pub(crate) document: String,
    pub(crate) relation: String,
}

impl Observation {
    fn borrowed(&self) -> acp::ActorRelationshipObservation<'_> {
        acp::ActorRelationshipObservation {
            requester: &self.requester,
            target: &self.target,
            policy: &self.policy,
            resource: &self.resource,
            document: &self.document,
            relation: &self.relation,
        }
    }
}

impl<S> DbAcpOps<S>
where
    S: Store + 'static,
{
    /// Observe a relationship as the transaction `context` names sees it. A
    /// document ACP that joins the transaction reads its writes; an external
    /// provider answers at the edge, before the turn, and its answer stands
    /// only when the transaction sees no change of the tuple owed to it.
    pub(super) async fn observe_in_txn(
        &self,
        context: &RequestContext,
        observation: Observation,
    ) -> Result<bool> {
        let txn = context
            .txn()
            .ok_or_else(|| query::QueryError::execution("an explicit transaction is required"))?;
        let handle = TransactionHandle::new(txn.to_string());
        let provider = acp::bound_to_request(&self.document_acp, context);
        let remote = if provider.supports_transactions() {
            None
        } else {
            Some(
                provider
                    .observe_actor_relationship(observation.borrowed())
                    .await?,
            )
        };
        let job = txn_job(move |turn| {
            Box::pin(async move {
                let ctx = turn.context()?;
                let fetcher = ctx.doc_fetcher();
                let Some(present) = remote else {
                    let acp = fetcher.transaction_acp(provider).await?.ok_or_else(|| {
                        query::QueryError::execution(
                            "transaction-bound document authorization is unavailable",
                        )
                    })?;
                    return Ok(acp
                        .observe_actor_relationship(observation.borrowed())
                        .await?);
                };
                let outbox = fetcher.transaction_outbox(provider).await?.ok_or_else(|| {
                    query::QueryError::execution(
                        "transaction-bound document authorization is unavailable",
                    )
                })?;
                let unsettled = outbox
                    .relationship_unsettled(
                        &observation.policy,
                        &observation.resource,
                        &observation.document,
                        &observation.relation,
                        &observation.target,
                    )
                    .await?;
                drop(outbox);
                if unsettled {
                    return Err(acp::Error::ProviderUnavailable(
                        "relationship change is unsettled".into(),
                    )
                    .into());
                }
                Ok(present)
            })
        });
        self.registry.run_as(context, &handle, None, job).await?
    }
}

/// The longest a commit waits for the provider to take what its transaction
/// staged. Past it the commit returns, and what is not settled stays owed
/// for the next pass.
pub(crate) const OWED_SETTLE_DEADLINE: std::time::Duration = std::time::Duration::from_secs(10);

/// Submit what the committed transaction `staged_by` now owes the provider:
/// the changes on the objects it staged on, within
/// [`OWED_SETTLE_DEADLINE`]. The commit stands whatever this comes to.
pub(super) async fn settle_owed_changes(acp: Arc<dyn acp::DocumentACP>, staged_by: u64) {
    // The node's drainer takes up whatever this wait leaves.
    acp.owed_changed();
    let pass = acp.settle_owed_changes(acp::OwedScope::Transaction(staged_by));
    match workers::timeout(OWED_SETTLE_DEADLINE, pass).await {
        Ok(Ok(settlement)) if settlement.provider_unavailable => tracing::warn!(
            settled = settlement.settled,
            "ACP provider unavailable; owed relationship changes stay staged"
        ),
        Ok(Ok(_)) => {}
        Ok(Err(error)) => tracing::warn!(
            %error,
            "Could not settle owed ACP relationship changes; they stay owed"
        ),
        // Dropping the pass leaves its submission in flight. It cannot land
        // after a later change of its tuple, because the provider takes one
        // signer's requests in order (`DocumentACP::submit_owed_change`).
        Err(workers::Error::TimedOut) => tracing::warn!(
            deadline = ?OWED_SETTLE_DEADLINE,
            "ACP provider did not take the staged relationship changes in time; they stay owed"
        ),
        Err(error) => tracing::warn!(
            %error,
            "Could not wait for the staged relationship changes to settle; they stay owed"
        ),
    }
}
