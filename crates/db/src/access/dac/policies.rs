//! Where a node's DAC policies live.

use std::sync::Arc;

use acp::{Policy, RemotePolicies, StorePolicyOptions, ZanzibarStore};
use async_trait::async_trait;

use super::error::DacPolicyError;

/// A node's DAC policies: its local Zanzibar store, or a remote ACP (Vera)
/// that owns them, with each one the node uses copied into a local store so
/// documents and collections are checked against it.
#[derive(Clone)]
pub struct DacPolicies {
    store: Arc<dyn ZanzibarStore>,
    remote: Option<Arc<dyn RemotePolicies>>,
}

impl DacPolicies {
    /// Policies kept in the local ACP's `store`.
    pub fn local(store: Arc<dyn ZanzibarStore>) -> Self {
        Self {
            store,
            remote: None,
        }
    }

    /// Policies owned by `remote`, copied into `mirror` as the node uses them.
    pub fn remote(remote: Arc<dyn RemotePolicies>, mirror: Arc<dyn ZanzibarStore>) -> Self {
        Self {
            store: mirror,
            remote: Some(remote),
        }
    }

    /// Go's name for the ACP, as its errors carry it.
    pub(super) fn kind(&self) -> &'static str {
        match self.remote {
            Some(_) => "Remote DAC",
            None => "Local DAC",
        }
    }

    pub(super) fn store(&self) -> &Arc<dyn ZanzibarStore> {
        &self.store
    }

    pub(super) fn remote_policies(&self) -> Option<&Arc<dyn RemotePolicies>> {
        self.remote.as_ref()
    }

    /// Store `policy` locally, checked as a document policy must be.
    pub(super) async fn keep(&self, policy: &Policy) -> Result<(), String> {
        let options = StorePolicyOptions::new()
            .with_validation()
            .with_dpi_enforcement();
        self.store
            .store_policy_with_options(policy, &options)
            .await
            .map_err(|e| e.to_string())
    }

    /// The policy `policy_id`: the local copy, or the remote ACP's, which is
    /// then kept locally. `Ok(None)` when neither has it.
    pub async fn get(&self, policy_id: &str) -> Result<Option<Policy>, DacPolicyError> {
        let lookup = |message: String| DacPolicyError::Lookup {
            message,
            policy_id: policy_id.to_owned(),
        };
        if let Some(policy) = self
            .store
            .get_policy(policy_id)
            .await
            .map_err(|e| lookup(e.to_string()))?
        {
            return Ok(Some(policy));
        }
        let Some(remote) = &self.remote else {
            return Ok(None);
        };
        let Some(policy) = remote.get_policy(policy_id).await.map_err(lookup)? else {
            return Ok(None);
        };
        self.keep(&policy).await.map_err(lookup)?;
        Ok(Some(policy))
    }

    /// Every policy the node holds locally.
    pub async fn list(&self) -> Result<Vec<Policy>, DacPolicyError> {
        self.store
            .list_policies()
            .await
            .map_err(|e| DacPolicyError::List(e.to_string()))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl acp::PolicyLookup for DacPolicies {
    async fn get_policy(&self, policy_id: &str) -> Result<Option<Policy>, String> {
        self.get(policy_id).await.map_err(|e| e.to_string())
    }
}
