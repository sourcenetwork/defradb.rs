//! NacManagerApi trait implementation for NacManager.

use acp::nac::{NacStatus, NodePermission};
use acp::ZanzibarStore;
use async_trait::async_trait;
use identity::Did;

use crate::nac::error::Result;

use super::{NacInfo, NacManager, NacManagerApi};

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S: ZanzibarStore + ?Sized + 'static> NacManagerApi for NacManager<S> {
    async fn initialize(&self, owner_identity: Option<&Did>) -> Result<()> {
        NacManager::initialize(self, owner_identity).await
    }
    async fn status(&self) -> NacStatus {
        NacManager::status(self).await
    }
    async fn owner(&self) -> Option<Did> {
        NacManager::owner(self).await
    }
    async fn is_enabled(&self) -> bool {
        NacManager::is_enabled(self).await
    }
    async fn check_permission(&self, identity: &Did, permission: NodePermission) -> Result<bool> {
        NacManager::check_permission(self, identity, permission).await
    }
    #[cfg_attr(target_arch = "wasm32", allow(clippy::arc_with_non_send_sync))]
    async fn check_permission_in_txn(
        &self,
        view: datastore::NamespaceView,
        identity: &Did,
        permission: NodePermission,
    ) -> Result<bool> {
        if !self.nac.supports_transactions() {
            return self.check_permission(identity, permission).await;
        }
        let scoped = self.nac.scoped(std::sync::Arc::new(
            acp::zanzibar::store::TransactionZanzibarStore::new(view),
        ));
        scoped
            .load()
            .await
            .map_err(|error| super::error::Error::Other(error.to_string()))?;
        scoped
            .check_permission(identity, permission)
            .await
            .map_err(|error| super::error::Error::Other(error.to_string()))
    }
    #[cfg_attr(target_arch = "wasm32", allow(clippy::arc_with_non_send_sync))]
    async fn bind_initial_bootstrap_owner_in_txn(
        &self,
        view: datastore::NamespaceView,
        actor: &Did,
        policy_id: &str,
        resource: &str,
        document: &str,
        owner: &Did,
        permissions: &[NodePermission],
    ) -> Result<()> {
        if !self.nac.supports_transactions() {
            return Err(super::error::Error::Other(
                "transaction-bound bootstrap binding is unsupported".into(),
            ));
        }
        let scoped = self.nac.scoped(std::sync::Arc::new(
            acp::zanzibar::store::TransactionZanzibarStore::new(view),
        ));
        scoped
            .load()
            .await
            .map_err(|error| super::error::Error::Acp(error.to_string()))?;
        scoped
            .bind_initial_bootstrap_owner(actor, policy_id, resource, document, owner, permissions)
            .await
            .map_err(|error| super::error::Error::Acp(error.to_string()))
    }
    async fn is_admin(&self, identity: &Did) -> Result<bool> {
        NacManager::is_admin(self, identity).await
    }
    async fn is_admin_persisted(&self, identity: &Did) -> Result<bool> {
        NacManager::is_admin_persisted(self, identity).await
    }
    async fn is_owner(&self, identity: &Did) -> bool {
        NacManager::is_owner(self, identity).await
    }
    async fn enable(&self, owner: &Did) -> Result<()> {
        NacManager::enable(self, owner).await
    }
    async fn disable(&self, requestor: &Did) -> Result<()> {
        NacManager::disable(self, requestor).await
    }
    async fn re_enable(&self, requestor: &Did) -> Result<()> {
        NacManager::re_enable(self, requestor).await
    }
    async fn purge(&self, requestor: &Did) -> Result<()> {
        NacManager::purge(self, requestor).await
    }
    async fn add_admin(&self, requestor: &Did, target: &Did) -> Result<bool> {
        NacManager::add_admin(self, requestor, target).await
    }
    async fn remove_admin(&self, requestor: &Did, target: &Did) -> Result<bool> {
        NacManager::remove_admin(self, requestor, target).await
    }
    async fn add_permission_grant(
        &self,
        requestor: &Did,
        target: &Did,
        permission: NodePermission,
    ) -> Result<bool> {
        NacManager::add_permission_grant(self, requestor, target, permission).await
    }
    async fn remove_permission_grant(
        &self,
        requestor: &Did,
        target: &Did,
        permission: NodePermission,
    ) -> Result<bool> {
        NacManager::remove_permission_grant(self, requestor, target, permission).await
    }
    async fn info(&self) -> NacInfo {
        NacManager::info(self).await
    }
}
