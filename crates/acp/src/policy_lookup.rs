//! Where a node reads a policy by ID, so a schema can be checked against it.

use async_trait::async_trait;
use std::sync::Arc;
use storage::corekv::MaybeSendSync;
use zanzibar::{Policy, ZanzibarStore};

/// Reads a stored policy. `Ok(None)` means the policy does not exist; an
/// error means the backend could not be asked.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait PolicyLookup: MaybeSendSync {
    async fn get_policy(&self, policy_id: &str) -> Result<Option<Policy>, String>;
}

/// The policies of a local Zanzibar store.
pub struct StorePolicies(pub Arc<dyn ZanzibarStore>);

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl PolicyLookup for StorePolicies {
    async fn get_policy(&self, policy_id: &str) -> Result<Option<Policy>, String> {
        self.0
            .get_policy(policy_id)
            .await
            .map_err(|e| e.to_string())
    }
}

/// A remote ACP that owns the node's DAC policies (Vera): it creates them and
/// answers for them. A node keeps a local copy of each one it uses.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait RemotePolicies: MaybeSendSync {
    /// Create the YAML `policy` on the remote ACP for `creator`, a DID, and
    /// return the ID the remote ACP gave it.
    async fn add_policy(&self, creator: &str, policy: &str) -> Result<String, String>;

    /// The remote ACP's policy `policy_id`; `Ok(None)` when it has none.
    async fn get_policy(&self, policy_id: &str) -> Result<Option<Policy>, String>;

    /// Where the node's light client of the remote ACP stands.
    fn light_client_status(&self) -> Result<LightClientStatus, String>;
}

/// The state of a node's light client of its remote ACP.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LightClientStatus {
    pub height: u64,
    pub module_state_root: String,
    pub cache_entries: usize,
    pub last_invalidation_height: u64,
    pub connected: bool,
}
