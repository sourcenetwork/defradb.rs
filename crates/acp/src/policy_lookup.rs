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
