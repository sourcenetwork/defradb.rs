//! NacManagerApi trait implementation for NacManager.

use acp::nac::{NacStatus, NodePermission};
use acp::ZanzibarStore;
use async_trait::async_trait;
use identity::Did;

use crate::nac::error::Result;

use super::{NacCaller, NacInfo, NacManager, NacManagerApi, NacRelationshipRequest};

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
    async fn check_permissions(
        &self,
        identity: &Did,
        permissions: &[NodePermission],
    ) -> Vec<Result<bool>> {
        NacManager::check_permissions(self, identity, permissions).await
    }
    async fn check_permissions_in_memory(
        &self,
        identity: &Did,
        permissions: &[NodePermission],
    ) -> Option<Vec<Result<bool>>> {
        let grants = self.edge_snapshot()?;
        Some(super::first_refusal(grants.as_ref(), identity, permissions).await)
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
    async fn check_bound_permission_in_txn(
        &self,
        view: datastore::NamespaceView,
        identity: &Did,
        permission: NodePermission,
        require_active: bool,
    ) -> Result<Option<bool>> {
        if !self.nac.supports_transactions() {
            return Ok(None);
        }
        let scoped = self.nac.scoped(std::sync::Arc::new(
            acp::zanzibar::store::TransactionZanzibarStore::new(view),
        ));
        scoped
            .load()
            .await
            .map_err(|error| super::error::Error::Other(error.to_string()))?;
        if require_active && scoped.status().await != NacStatus::Enabled {
            return Ok(None);
        }
        scoped
            .check_permission(identity, permission)
            .await
            .map(Some)
            .map_err(|error| super::error::Error::Other(error.to_string()))
    }
    #[cfg_attr(target_arch = "wasm32", allow(clippy::arc_with_non_send_sync))]
    async fn change_source_permission_in_txn(
        &self,
        view: datastore::NamespaceView,
        requestor: &Did,
        change: &super::SourcePermissionChange<'_>,
    ) -> Result<Option<bool>> {
        if !self.transaction_store_bound || !self.nac.supports_transactions() {
            return Ok(None);
        }
        self.refresh_edge_on_commit(&view).await?;
        let scoped = self.nac.scoped(std::sync::Arc::new(
            acp::zanzibar::store::TransactionZanzibarStore::new(view),
        ));
        scoped
            .load()
            .await
            .map_err(|error| super::error::Error::Other(error.to_string()))?;
        scoped
            .change_source_permission(
                requestor,
                change.permission,
                change.parent_source,
                change.subject_kind,
                change.subject,
                change.delete,
            )
            .await
            .map(Some)
            .map_err(|error| match error {
                acp::error::Error::NotOwner { .. } | acp::error::Error::PermissionDenied(_) => {
                    super::error::Error::PermissionDenied
                }
                other => super::error::Error::Other(other.to_string()),
            })
    }
    #[cfg_attr(target_arch = "wasm32", allow(clippy::arc_with_non_send_sync))]
    async fn source_permission_matches_in_txn(
        &self,
        view: datastore::NamespaceView,
        change: &super::SourcePermissionChange<'_>,
    ) -> Result<Option<bool>> {
        if !self.transaction_store_bound || !self.nac.supports_transactions() {
            return Ok(None);
        }
        let scoped = self.nac.scoped(std::sync::Arc::new(
            acp::zanzibar::store::TransactionZanzibarStore::new(view),
        ));
        scoped
            .source_permission_matches(
                change.permission,
                change.parent_source,
                change.subject_kind,
                change.subject,
                change.delete,
            )
            .await
            .map(Some)
            .map_err(|error| super::error::Error::Other(error.to_string()))
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
    async fn disable_as(&self, caller: &NacCaller) -> Result<()> {
        NacManager::disable_as(self, caller).await
    }
    async fn re_enable_as(&self, caller: &NacCaller) -> Result<()> {
        NacManager::re_enable_as(self, caller).await
    }
    async fn change_relationship_as(
        &self,
        caller: &NacCaller,
        request: &NacRelationshipRequest,
    ) -> Result<bool> {
        NacManager::change_relationship_as(self, caller, request).await
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
    async fn explain_subject(
        &self,
        requestor: &Did,
        subject: &Did,
    ) -> Result<Vec<acp::nac::PermissionProvenance>> {
        NacManager::explain_subject(self, requestor, subject).await
    }
}
