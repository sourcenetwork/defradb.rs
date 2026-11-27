//! The inputs a create in a gated collection reads: the collection's policy,
//! when only a remote provider holds it, and the provider's decision on
//! `create`. Both are resolved outside the transaction and read inside it.

use crate::error::{QueryError, Result};

use super::{PreparedInputKind, PreparedInputMemo, PreparedInputRequest};

/// What a create decision is asked for: the actor, the credential it acts
/// with, the policy, the resource, its `create` and the derived
/// relationships. The resolver checks the actor and credential against the
/// request it serves.
pub type CreateDecisionRequest = (
    String,
    Option<String>,
    String,
    String,
    String,
    Vec<acp::DerivedRelationship>,
);

impl PreparedInputMemo {
    /// Whether `actor` may create a document of `resource` under `policy`,
    /// over `derived`, as the provider decided `create` outside the
    /// transaction.
    pub fn create_decision(
        &self,
        actor: &identity::Did,
        context: &defra_core::RequestContext,
        policy: &str,
        resource: &str,
        create: &str,
        derived: &[acp::DerivedRelationship],
    ) -> Result<bool> {
        let key: CreateDecisionRequest = (
            actor.to_string(),
            context.bearer_token_for(actor.as_str()),
            policy.to_owned(),
            resource.to_owned(),
            create.to_owned(),
            derived.to_vec(),
        );
        let key =
            serde_json::to_vec(&key).map_err(|error| QueryError::execution(error.to_string()))?;
        let request = PreparedInputRequest::new(PreparedInputKind::CreateDecision, key);
        let bytes = self
            .get_or_want(&request)
            .ok_or(QueryError::NeedsInput(request))?;
        serde_json::from_slice(&bytes).map_err(|error| QueryError::execution(error.to_string()))
    }

    /// The policy `policy_id` as the remote provider holds it, fetched
    /// outside the transaction; `None` when the provider has none.
    pub fn dac_policy(&self, policy_id: &str) -> Result<Option<acp::Policy>> {
        let request =
            PreparedInputRequest::new(PreparedInputKind::DacPolicy, policy_id.as_bytes().to_vec());
        let bytes = self
            .get_or_want(&request)
            .ok_or(QueryError::NeedsInput(request))?;
        serde_json::from_slice(&bytes).map_err(|error| QueryError::execution(error.to_string()))
    }
}
