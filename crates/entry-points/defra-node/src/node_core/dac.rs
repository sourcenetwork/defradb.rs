//! DAC policies as the caller `context` names. Policies are not
//! transactional, so a transaction the context carries does not apply.

use std::sync::Arc;

use defra_core::RequestContext;

use super::NodeCore;
use crate::ops::transaction_error;
use crate::Result;

impl NodeCore<'_> {
    /// Add the YAML `policy` with the identity acting in `context` as its
    /// creator, and return its ID. Go's `AddDACPolicy`: without an identity
    /// there is no creator, and the node's own identity does not stand in.
    ///
    /// With a `nonce`, the policy ID is the one the remote ACP (vera.rs)
    /// derives from the definition, the creator and the nonce, so the caller
    /// knows it beforehand (`VeraRsProvider::predict_policy_id`,
    /// `EmbeddedVeraPolicy::id`). Sent again with the same nonce, as after an
    /// answer that was lost, it returns the same policy and makes no second
    /// one; a host adopts a policy its embedded deployment was seeded with
    /// the same way. Without a nonce a caller that sends a create again makes
    /// a second policy. A node on local ACP refuses a nonce.
    pub(crate) async fn add_dac_policy(
        &self,
        context: &RequestContext,
        policy: &str,
        nonce: Option<acp::PolicyNonce>,
    ) -> Result<String> {
        let runner = Arc::clone(self.node.runner());
        let (context, policy) = (context.clone(), policy.to_owned());
        self.node
            .on_acp(move || async move {
                runner
                    .add_dac_policy_as(&context, &policy, nonce)
                    .await
                    .map_err(transaction_error)
            })
            .await
    }

    /// [`Self::add_dac_policy`] with the local policy ID built from
    /// `counter`.
    ///
    /// Local ACP policy IDs are `sha256(policy fields || counter)` with a
    /// per-node counter, so two nodes that register the same policy after
    /// different histories derive different IDs. A collection bound to a
    /// policy embeds the ID in its SDL, and the SDL determines the collection
    /// ID, so every peer joining a policy-bound collection must reproduce the
    /// creator's policy ID. Passing the creator's counter (typically 1, its
    /// first policy) makes the ID reproducible, and storing an existing ID
    /// overwrites it, so registering again is idempotent. Local ACP only: a
    /// node on vera.rs names a policy by its nonce
    /// ([`Self::add_dac_policy`] with a nonce) and refuses a counter.
    pub(crate) async fn add_dac_policy_with_counter(
        &self,
        context: &RequestContext,
        policy: &str,
        counter: u64,
    ) -> Result<String> {
        let database = Arc::clone(self.node.database());
        let (context, policy) = (context.clone(), policy.to_owned());
        self.node
            .on_acp(move || async move {
                Ok(database
                    .add_dac_policy_with_counter_as(&context, &policy, counter)
                    .await?)
            })
            .await
    }

    /// Every DAC policy the node holds.
    pub(crate) async fn list_dac_policies(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<acp::Policy>> {
        let runner = Arc::clone(self.node.runner());
        let context = self.node.admitted_caller(context)?;
        self.node
            .on_acp(move || async move {
                runner
                    .list_dac_policies_as(&context)
                    .await
                    .map_err(transaction_error)
            })
            .await
    }

    /// The DAC policy `policy_id`; `None` when no ACP has it.
    pub(crate) async fn get_dac_policy(
        &self,
        context: &RequestContext,
        policy_id: &str,
    ) -> Result<Option<acp::Policy>> {
        let runner = Arc::clone(self.node.runner());
        let (context, policy_id) = (self.node.admitted_caller(context)?, policy_id.to_owned());
        self.node
            .on_acp(move || async move {
                runner
                    .get_dac_policy_as(&context, &policy_id)
                    .await
                    .map_err(transaction_error)
            })
            .await
    }

    /// The remote ACP's light client status.
    pub(crate) async fn acp_light_client_status(
        &self,
        context: &RequestContext,
    ) -> Result<acp::LightClientStatus> {
        self.node
            .runner()
            .acp_light_client_status_as(&self.node.admitted_caller(context)?)
            .await
            .map_err(transaction_error)
    }

    pub(crate) async fn certified_assignments(
        &self,
        context: &RequestContext,
        query: acp::AssignmentQuery,
    ) -> Result<acp::CertifiedAssignments> {
        let database = Arc::clone(self.node.database());
        let context = self.node.admitted_caller(context)?;
        self.node
            .on_acp(move || async move {
                Ok(database.certified_assignments_as(&context, &query).await?)
            })
            .await
    }
}
