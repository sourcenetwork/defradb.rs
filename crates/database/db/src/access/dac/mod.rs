//! DAC policies: adding one (Go's `AddDACPolicy`), reading them, and checking
//! a collection's policy against them. One entry point per operation, which
//! every edge calls.

mod error;
mod policies;

use std::sync::Arc;

use acp::nac::NodePermission;
use acp::policy_yaml::{self, ParsedPolicy};
use acp::{LightClientStatus, Policy, PolicyNonce};
use defra_core::RequestContext;
use storage::corekv::Store;

use crate::database::DB;

pub use error::DacPolicyError;
pub use policies::DacPolicies;

/// The counter a policy is built with before a remote ACP names it: the
/// remote ACP assigns the ID, so this value never reaches storage.
const UNASSIGNED_ID_COUNTER: u64 = 1;

/// What a caller's policy ID is made from, beyond the definition.
#[derive(Clone, Copy)]
enum IdSeed {
    /// The ACP's own choice: the local store's next counter, or the remote
    /// ACP's random nonce.
    Next,
    /// A local ACP's counter.
    Counter(u64),
    /// A remote ACP's nonce.
    Nonce(PolicyNonce),
}

impl<S> DB<S>
where
    S: Store + 'static,
{
    /// Install the node's DAC policies. First call wins; install them where
    /// the document ACP is installed.
    pub fn set_dac_policies(&self, policies: Arc<DacPolicies>) {
        let _ = self.dac_policies.set(policies);
    }

    /// The node's DAC policies; `None` when it runs without document ACP.
    pub fn dac_policies(&self) -> Option<&Arc<DacPolicies>> {
        self.dac_policies.get()
    }

    /// Add the YAML `policy` as the identity acting in `context`, its
    /// creator, and return its ID. Go's `AddDACPolicy`.
    pub async fn add_dac_policy_as(
        &self,
        context: &RequestContext,
        policy: &str,
    ) -> Result<String, DacPolicyError> {
        self.add_policy(context, policy, IdSeed::Next).await
    }

    /// [`Self::add_dac_policy_as`] with the local policy ID built from
    /// `counter`, so nodes with different histories derive the same ID for
    /// the same policy. Local ACP only: a remote ACP refuses it.
    pub async fn add_dac_policy_with_counter_as(
        &self,
        context: &RequestContext,
        policy: &str,
        counter: u64,
    ) -> Result<String, DacPolicyError> {
        self.add_policy(context, policy, IdSeed::Counter(counter))
            .await
    }

    /// [`Self::add_dac_policy_as`] with the remote ACP's policy ID derived
    /// from `nonce`: vera.rs names a policy by its definition, its creator
    /// and the nonce, so the caller knows the ID before it exists. Sent again
    /// with the same nonce, it returns the same policy and makes no second
    /// one; that is how a host adopts a policy its deployment was seeded
    /// with. Remote ACP only: a local ACP refuses it.
    pub async fn add_dac_policy_with_nonce_as(
        &self,
        context: &RequestContext,
        policy: &str,
        nonce: PolicyNonce,
    ) -> Result<String, DacPolicyError> {
        self.add_policy(context, policy, IdSeed::Nonce(nonce)).await
    }

    /// Checked in Go's order: node access, an ACP, a creator, a policy;
    /// then whether the ACP can make the ID from `seed`. A remote ACP
    /// creates the policy and the node keeps a copy, so the collections and
    /// documents that name it are checked locally.
    async fn add_policy(
        &self,
        context: &RequestContext,
        policy: &str,
        seed: IdSeed,
    ) -> Result<String, DacPolicyError> {
        self.check_node_access_as(context, NodePermission::DacPolicyAdd)
            .await?;
        let policies = self.dac_policies().ok_or(DacPolicyError::AcpNotAvailable)?;
        let creator = context
            .identity()
            .filter(|creator| !creator.is_empty())
            .ok_or(DacPolicyError::CreatorEmpty)?;
        if policy.is_empty() {
            return Err(DacPolicyError::DataEmpty);
        }
        let failed = |message: String| DacPolicyError::AddFailed {
            message,
            acp: policies.kind(),
            creator: creator.to_owned(),
        };
        let parsed = parse(policy).map_err(failed)?;

        let Some(remote) = policies.remote_policies() else {
            let counter = match seed {
                IdSeed::Counter(counter) => counter,
                IdSeed::Next => policies
                    .store()
                    .next_policy_counter()
                    .await
                    .map_err(|e| failed(e.to_string()))?,
                IdSeed::Nonce(_) => return Err(DacPolicyError::NonceNeedsRemoteAcp),
            };
            let built = build(&parsed, counter).map_err(failed)?;
            policies.keep(&built).await.map_err(failed)?;
            return Ok(built.id);
        };
        let mut built = build(&parsed, UNASSIGNED_ID_COUNTER).map_err(failed)?;
        built.validate().map_err(|e| failed(e.to_string()))?;
        built.validate_dpi().map_err(|e| failed(e.to_string()))?;
        let nonce = match seed {
            IdSeed::Next => None,
            IdSeed::Nonce(nonce) => Some(nonce),
            IdSeed::Counter(_) => return Err(DacPolicyError::CounterNeedsLocalAcp),
        };
        let policy_id = remote
            .add_policy(creator, policy, nonce)
            .await
            .map_err(failed)?;
        built.id = policy_id.clone();
        policies.keep(&built).await.map_err(failed)?;
        Ok(policy_id)
    }

    /// Every policy the node holds, for a caller with `DacStatus`.
    pub async fn list_dac_policies_as(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<Policy>, DacPolicyError> {
        self.check_node_access_as(context, NodePermission::DacStatus)
            .await?;
        self.dac_policies()
            .ok_or(DacPolicyError::AcpNotAvailable)?
            .list()
            .await
    }

    /// The policy `policy_id`, for a caller with `DacStatus`; `None` when no
    /// ACP has it.
    pub async fn get_dac_policy_as(
        &self,
        context: &RequestContext,
        policy_id: &str,
    ) -> Result<Option<Policy>, DacPolicyError> {
        self.check_node_access_as(context, NodePermission::DacStatus)
            .await?;
        self.dac_policies()
            .ok_or(DacPolicyError::AcpNotAvailable)?
            .get(policy_id)
            .await
    }

    /// The remote ACP's light client status, for a caller with `DacStatus`.
    pub async fn acp_light_client_status_as(
        &self,
        context: &RequestContext,
    ) -> Result<LightClientStatus, DacPolicyError> {
        self.check_node_access_as(context, NodePermission::DacStatus)
            .await?;
        self.dac_policies()
            .ok_or(DacPolicyError::AcpNotAvailable)?
            .remote_policies()
            .ok_or(DacPolicyError::NoLightClient)?
            .light_client_status()
            .map_err(DacPolicyError::LightClient)
    }

    /// Whether resource `resource_name` of policy `policy_id` can guard a
    /// collection: the policy exists and the resource has the permissions
    /// documents need. Go's `ValidateResourceInterface`.
    pub async fn validate_resource_interface(
        &self,
        policy_id: &str,
        resource_name: &str,
    ) -> Result<(), DacPolicyError> {
        let stored = self
            .dac_policies()
            .ok_or(DacPolicyError::AcpNotAvailable)?
            .get(policy_id)
            .await?;
        acp::validate_resource_interface(policy_id, resource_name, stored.as_ref())
            .map_err(DacPolicyError::ResourceInterface)
    }
}

/// `policy` parsed and checked as Go's ACP checks it before storing it.
fn parse(policy: &str) -> Result<ParsedPolicy, String> {
    policy_yaml::check_duplicate_yaml_keys(policy)?;
    let parsed = policy_yaml::parse_policy_yaml(policy)?;
    if parsed.name.is_empty() {
        return Err("name required".to_owned());
    }
    policy_yaml::validate_policy_expressions(&parsed)?;
    Ok(parsed)
}

fn build(parsed: &ParsedPolicy, counter: u64) -> Result<Policy, String> {
    policy_yaml::build_policy(parsed, counter).map_err(|e| format!("invalid policy: {e}"))
}
