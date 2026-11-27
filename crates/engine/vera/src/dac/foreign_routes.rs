//! Routing a policy to the home of the deployment that owns it, and the one
//! precondition every route keeps: the node's own deployment does not hold
//! the policy (`proofs/tla/ForeignPolicy.tla` `INV_NoShadow`,
//! `proofs/lean/ForeignRoutes.lean` `NoShadow`). Were it held, its home
//! would decide the node's own documents under that policy.

use super::VeraDocumentACP;
use crate::foreign::route_key;
use crate::provider::ProviderError;
#[cfg(not(target_arch = "wasm32"))]
use crate::{AcpTuning, ForeignHome};

impl VeraDocumentACP {
    /// Decide `policy_id` against `home` from now on, over the network
    /// runtime `edge`: every access decision, owner and registration read
    /// for a document under it is a proof of that deployment, and the node
    /// registers, archives and changes nothing under it. A home that is
    /// away is added all the same; its decisions are retryably unavailable
    /// until it answers. A policy the node's own deployment holds is
    /// refused ([`ProviderError::OwnPolicyRouted`]).
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn add_foreign_policy(
        &self,
        policy_id: &str,
        home: &ForeignHome,
        tuning: &AcpTuning,
        edge: &edge::Edge,
    ) -> Result<(), ProviderError> {
        let _claim = self.unheld_here(policy_id).await?;
        self.foreign_policies()
            .add(policy_id, home, tuning, edge)
            .await
    }

    /// Route `policy_id` to `acp`, under the same precondition as
    /// [`Self::add_foreign_policy`], for tests that decide through a
    /// provider of their own; `connect` builds the home's client.
    #[cfg(test)]
    pub(crate) async fn route_for_test<F>(
        &self,
        policy_id: &str,
        deployment: &str,
        address: &str,
        connect: F,
    ) -> Result<(), ProviderError>
    where
        F: std::future::Future<Output = Result<std::sync::Arc<VeraDocumentACP>, ProviderError>>,
    {
        let _claim = self.unheld_here(policy_id).await?;
        self.foreign_policies()
            .route_to(policy_id, deployment, address, connect)
            .await
    }

    /// Claim `policy_id` for a route, and check that the node's own
    /// deployment does not hold it. The claim comes first, and is refused
    /// while a create of the policy is in flight, so a create that starts
    /// after this check sees the claim and is refused before it is sent;
    /// one that ended before it left the policy where this read sees it
    /// (`proofs/tla/ForeignPolicyCreate.tla`).
    async fn unheld_here(&self, policy_id: &str) -> Result<crate::foreign::Held, ProviderError> {
        let claim = self.foreign_policies().claim(policy_id)?;
        let policy = route_key(policy_id)?;
        let asked = policy.clone();
        let held = self
            .remote(move |provider| async move { provider.query_policy(&asked).await })
            .await?;
        if held.is_some() {
            return Err(ProviderError::OwnPolicyRouted { policy });
        }
        Ok(claim)
    }
}
