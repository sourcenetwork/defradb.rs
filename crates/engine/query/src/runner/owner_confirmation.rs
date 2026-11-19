//! A commit that confirms the provider owner answers its transaction used.
//!
//! A transaction on a document ACP that cannot join it may have been told a
//! document's owner by the provider ([`crate::txn::ObservedOwners`]). Nothing
//! in storage sees that answer change, so the commit is a prepared-input
//! attempt: the edge reads every answer again from the provider, outside the
//! transaction, in one round, and the commit runs only when each is the same.
//! The round starts after the commit began, so every answer is confirmed at a
//! revision no older than the commit's start. A change the provider finalizes
//! between that round and the local commit is not caught: the two share no
//! order (proofs/tla/RemoteOwnerObservation.tla, WindowWitness).

use defra_core::RequestContext;

use super::{DocFetcher, QueryRunner};
use crate::error::TransactionError;
use crate::prepared::PreparedInputMemo;
use crate::txn::{txn_job, TransactionHandle, TransactionRegistry};

impl<F, R> QueryRunner<F, R>
where
    F: DocFetcher + 'static,
    R: TransactionRegistry,
{
    /// Commit `handle`, confirming first what its transaction was told by the
    /// provider. A transaction told nothing commits as it always did, after
    /// one look at what it kept; on a provider that joins transactions there
    /// is nothing to look at. A
    /// provider that cannot be reached refuses the commit retryably, with
    /// nothing committed and the transaction still open.
    pub(super) async fn commit_confirming(
        &self,
        context: &RequestContext,
        handle: &TransactionHandle,
    ) -> std::result::Result<(), TransactionError> {
        // A provider that joins the transaction answers inside it: nothing to confirm.
        if self.acp.supports_transactions() {
            return self.registry.commit_as(context, handle).await;
        }
        let job = txn_job(|turn| {
            Box::pin(async move {
                Ok::<_, TransactionError>(
                    turn.context()?
                        .observed_owners()
                        .map(|observed| observed.requests())
                        .unwrap_or_default(),
                )
            })
        });
        let requests = self
            .registry
            .run_as(context, handle, self.deadline(), job)
            .await??;
        let Some((first, rest)) = requests.split_first() else {
            return self.registry.commit_as(context, handle).await;
        };
        // One wave of prepared inputs, keyed by the exact request bytes, as
        // every other input a transaction step waits for at the edge.
        let inputs = PreparedInputMemo::default();
        for request in rest {
            inputs.want(request.clone());
        }
        inputs
            .answer_wave(first, |wave| async move {
                futures::future::try_join_all(
                    wave.iter()
                        .map(|request| self.resolve_acp_decision_input(request)),
                )
                .await
            })
            .await
            .map_err(|error| {
                TransactionError::execution(format!(
                    "the document ACP provider could not confirm the owners this transaction \
                     was told, so nothing was committed; commit again: {error}"
                ))
            })?;
        let confirmations = requests
            .into_iter()
            .filter_map(|request| inputs.get(&request).map(|answer| (request, answer)))
            .collect();
        self.registry
            .commit_confirmed_as(context, handle, confirmations)
            .await
    }
}
