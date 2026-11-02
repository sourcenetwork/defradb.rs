//! DAC policies as the caller `context` names. Policies are not
//! transactional, so a transaction the context carries does not apply.

use std::sync::Arc;

use defra_core::RequestContext;

use crate::ops::request::transaction_error;
use crate::{Node, Result};

impl Node {
    /// Add the YAML `policy` with the identity acting in `context` as its
    /// creator, and return its ID. Go's `AddDACPolicy`: without an identity
    /// there is no creator, and the node's own identity does not stand in.
    pub async fn add_dac_policy_as(
        &self,
        context: &RequestContext,
        policy: &str,
    ) -> Result<String> {
        let runner = Arc::clone(self.runner());
        let (context, policy) = (context.clone(), policy.to_owned());
        self.on_acp(move || async move {
            runner
                .add_dac_policy_as(&context, &policy)
                .await
                .map_err(transaction_error)
        })
        .await
    }

    /// [`Self::add_dac_policy_as`] with the local policy ID built from
    /// `counter`.
    ///
    /// Local ACP policy IDs are `sha256(policy fields || counter)` with a
    /// per-node counter, so two nodes that register the same policy after
    /// different histories derive different IDs. A collection bound to a
    /// policy embeds the ID in its SDL, and the SDL determines the collection
    /// ID, so every peer joining a policy-bound collection must reproduce the
    /// creator's policy ID. Passing the creator's counter (typically 1, its
    /// first policy) makes the ID reproducible, and storing an existing ID
    /// overwrites it, so registering again is idempotent. Local ACP only.
    pub async fn add_dac_policy_with_counter_as(
        &self,
        context: &RequestContext,
        policy: &str,
        counter: u64,
    ) -> Result<String> {
        let database = Arc::clone(self.database());
        let (context, policy) = (context.clone(), policy.to_owned());
        self.on_acp(move || async move {
            Ok(database
                .add_dac_policy_with_counter_as(&context, &policy, counter)
                .await?)
        })
        .await
    }

    /// Every DAC policy the node holds.
    pub async fn list_dac_policies_as(&self, context: &RequestContext) -> Result<Vec<acp::Policy>> {
        let runner = Arc::clone(self.runner());
        let context = self.caller(context);
        self.on_acp(move || async move {
            runner
                .list_dac_policies_as(&context)
                .await
                .map_err(transaction_error)
        })
        .await
    }

    /// The DAC policy `policy_id`; `None` when no ACP has it.
    pub async fn get_dac_policy_as(
        &self,
        context: &RequestContext,
        policy_id: &str,
    ) -> Result<Option<acp::Policy>> {
        let runner = Arc::clone(self.runner());
        let (context, policy_id) = (self.caller(context), policy_id.to_owned());
        self.on_acp(move || async move {
            runner
                .get_dac_policy_as(&context, &policy_id)
                .await
                .map_err(transaction_error)
        })
        .await
    }

    /// The remote ACP's light client status.
    pub async fn acp_light_client_status_as(
        &self,
        context: &RequestContext,
    ) -> Result<acp::LightClientStatus> {
        self.runner()
            .acp_light_client_status_as(&self.caller(context))
            .await
            .map_err(transaction_error)
    }
}
