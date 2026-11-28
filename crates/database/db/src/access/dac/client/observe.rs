//! Observing one exact relationship inside a transaction.

use defra_core::RequestContext;
use identity::Did;
use query::txn::{txn_job, TransactionHandle, TransactionRegistry};
use storage::corekv::Store;

use super::{DocumentRelationships, Result};

/// One exact relationship observation, owned so it can cross into a
/// transaction turn.
pub(super) struct Observation {
    pub(super) requester: Did,
    pub(super) target: Did,
    pub(super) policy: String,
    pub(super) resource: String,
    pub(super) document: String,
    pub(super) relation: String,
}

impl Observation {
    pub(super) fn borrowed(&self) -> acp::ActorRelationshipObservation<'_> {
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

impl<S> DocumentRelationships<S>
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
        let provider = acp::bound_to_request(&self.provider, context);
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
