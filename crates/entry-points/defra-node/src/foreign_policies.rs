//! Policies another Vera deployment owns, decided against their owner's
//! home ([`vera::ForeignPolicies`]): added and removed while the node runs,
//! as an invite to another account's space adds one.

use crate::{Error, Node, Result};

impl Node {
    /// Decide `policy_id` against `home` from now on: every access decision,
    /// owner and registration read for a document under it is a proof of
    /// that deployment, verified under its consensus key, and the node
    /// registers, archives and changes nothing under it. A home that is away
    /// is added all the same; its decisions are retryably unavailable until
    /// it answers. Fails on a node without Vera document ACP, and for a
    /// policy the node's own deployment holds
    /// ([`vera::ProviderError::OwnPolicyRouted`]), whose documents that home
    /// would otherwise decide.
    pub async fn add_foreign_policy(
        &self,
        policy_id: &str,
        home: &vera::ForeignHome,
    ) -> Result<()> {
        let acp = self.vera_acp().ok_or_else(|| {
            Error::VeraProvider(vera::ProviderError::Config(
                "foreign policies need Vera document ACP".into(),
            ))
        })?;
        let edge = match self.network.as_ref() {
            Some(network) => network.edge().clone(),
            None => edge::on_demand().cloned().map_err(Error::EdgeRuntime)?,
        };
        acp.add_foreign_policy(policy_id, home, &vera::AcpTuning::default(), &edge)
            .await
            .map_err(Error::VeraProvider)
    }

    /// Decide `policy_id` against the node's own deployment again. Whether
    /// another deployment decided it.
    pub fn remove_foreign_policy(&self, policy_id: &str) -> bool {
        self.vera_acp()
            .is_some_and(|acp| acp.foreign_policies().remove(policy_id))
    }
}
