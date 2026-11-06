//! Node Access Control state, factory and trait wiring.

pub mod error;
pub mod factory;
mod toggle;
mod trait_impl;
mod write;

pub use error::{Error, Result};
pub use factory::create_memory_nac_manager;
#[cfg(not(target_arch = "wasm32"))]
pub use factory::create_persistent_nac_manager;
pub use toggle::NacCaller;
pub use write::{NacChange, NacRelation, NacRelationshipRequest};

use std::sync::Arc;

use acp::nac::{NacStatus, NodeACP, NodePermission};
use acp::ZanzibarStore;
use async_trait::async_trait;
use identity::Did;

/// NAC configuration options.
#[derive(Debug, Clone, Default)]
pub struct NacConfig {
    /// Whether NAC should be enabled.
    pub enabled: bool,

    /// Whether dev mode is enabled (allows purge operation).
    pub dev_mode: bool,

    /// Path to store NAC data (if using persistent storage).
    pub data_path: Option<String>,

    /// Re-runs of a status change after a lost commit; `None` keeps the
    /// engine default.
    pub max_txn_retries: Option<u32>,
}

impl NacConfig {
    /// Create a new NAC config.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable NAC.
    pub fn with_enabled(mut self) -> Self {
        self.enabled = true;
        self
    }

    /// Enable dev mode (allows purge).
    pub fn with_dev_mode(mut self) -> Self {
        self.dev_mode = true;
        self
    }

    /// Set the data path for persistent storage.
    pub fn with_data_path(mut self, path: impl Into<String>) -> Self {
        self.data_path = Some(path.into());
        self
    }

    /// Re-run a status change up to `retries` times after a lost commit.
    pub fn with_max_txn_retries(mut self, retries: u32) -> Self {
        self.max_txn_retries = Some(retries);
        self
    }

    /// Runs a status change gets in all, the first included.
    fn attempts(&self) -> u32 {
        storage::corekv::engine_retry_options(
            self.max_txn_retries
                .unwrap_or(storage::corekv::DEFAULT_MAX_TXN_RETRIES),
        )
        .max_attempts
    }
}

/// Trait for NAC manager operations, enabling dynamic dispatch over different store backends.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait NacManagerApi: defra_core::thread_bounds::MaybeSendSync {
    async fn initialize(&self, owner_identity: Option<&Did>) -> Result<()>;
    async fn status(&self) -> NacStatus;
    async fn owner(&self) -> Option<Did>;
    async fn is_enabled(&self) -> bool;
    async fn check_permission(&self, identity: &Did, permission: NodePermission) -> Result<bool>;
    async fn check_permission_in_txn(
        &self,
        _view: datastore::NamespaceView,
        identity: &Did,
        permission: NodePermission,
    ) -> Result<bool> {
        self.check_permission(identity, permission).await
    }
    async fn is_admin(&self, identity: &Did) -> Result<bool>;
    async fn is_admin_persisted(&self, identity: &Did) -> Result<bool>;
    async fn is_owner(&self, identity: &Did) -> bool;
    async fn enable(&self, owner: &Did) -> Result<()>;
    /// Turn NAC off for `caller`, deciding in the write that changes it.
    async fn disable_as(&self, caller: &NacCaller) -> Result<()>;
    /// Turn NAC back on for `caller`, deciding in the write that changes it.
    async fn re_enable_as(&self, caller: &NacCaller) -> Result<()>;
    /// Add or remove a relationship for `caller`, deciding in the write that
    /// makes the change; whether the relationship was there before.
    async fn change_relationship_as(
        &self,
        caller: &NacCaller,
        request: &NacRelationshipRequest,
    ) -> Result<bool>;
    async fn purge(&self, requestor: &Did) -> Result<()>;
    async fn add_admin(&self, requestor: &Did, target: &Did) -> Result<bool>;
    async fn remove_admin(&self, requestor: &Did, target: &Did) -> Result<bool>;
    async fn add_permission_grant(
        &self,
        requestor: &Did,
        target: &Did,
        permission: NodePermission,
    ) -> Result<bool>;
    async fn remove_permission_grant(
        &self,
        requestor: &Did,
        target: &Did,
        permission: NodePermission,
    ) -> Result<bool>;
    async fn info(&self) -> NacInfo;
}

/// NAC manager that provides config-aware NAC operations.
///
/// The manager wraps the `NodeACP` and provides:
/// - Initialization based on configuration
/// - Permission checking with config-aware fallbacks
/// - Admin management with authorization
pub struct NacManager<S: ZanzibarStore + ?Sized + 'static> {
    nac: Arc<NodeACP<S>>,
    config: NacConfig,
}

impl<S: ZanzibarStore + ?Sized + 'static> NacManager<S> {
    /// Create a new NAC manager with the given store and config.
    pub fn new(store: Arc<S>, config: NacConfig) -> Self {
        Self {
            nac: Arc::new(NodeACP::new(store)),
            config,
        }
    }

    /// A manager over `store` (a transaction or a scope) that shares this
    /// one's policy snapshot.
    fn scoped<T>(&self, store: Arc<T>) -> NacManager<T>
    where
        T: ZanzibarStore + ?Sized + 'static,
    {
        NacManager {
            nac: Arc::new(self.nac.scoped(store)),
            config: self.config.clone(),
        }
    }

    /// Initialize NAC from config and possibly enable it.
    ///
    /// This should be called during node startup. If NAC is configured to be
    /// enabled and an owner identity is provided, NAC will be activated.
    pub async fn initialize(&self, owner_identity: Option<&Did>) -> Result<()> {
        if let Some(scope) = self
            .nac
            .begin_scope(false)
            .await
            .map_err(|error| Error::Acp(error.to_string()))?
        {
            let scoped = self.scoped(scope.store());
            scoped
                .nac
                .load()
                .await
                .map_err(|error| Error::Acp(error.to_string()))?;
            let scoped: Box<dyn NacManagerApi> = Box::new(scoped);
            scoped.initialize(owner_identity).await?;
            drop(scoped);
            scope
                .commit()
                .await
                .map_err(|error| Error::Acp(error.to_string()))?;
            self.nac
                .load()
                .await
                .map_err(|error| Error::Acp(error.to_string()))?;
            return Ok(());
        }

        self.nac
            .load()
            .await
            .map_err(|e| Error::Other(format!("failed to load NAC state: {}", e)))?;

        if self.config.enabled {
            let current_status = self.nac.status().await;

            match current_status {
                NacStatus::NotConfigured => {
                    if let Some(owner) = owner_identity {
                        self.nac
                            .enable(owner)
                            .await
                            .map_err(|e| Error::Other(format!("failed to enable NAC: {}", e)))?;
                        tracing::info!(
                            target: "nac::startup",
                            owner = %owner,
                            "NAC enabled for the first time"
                        );
                    } else {
                        return Err(Error::Other("can not start nac without identity".into()));
                    }
                }
                NacStatus::Enabled => {
                    tracing::info!(
                        target: "nac::startup",
                        "NAC already enabled from previous session"
                    );
                }
                // Go starts a disabled NAC disabled: only re-enable turns it
                // back on.
                NacStatus::DisabledTemporarily => {
                    tracing::info!(
                        target: "nac::startup",
                        "NAC starts temporarily disabled; re-enable it to turn it back on"
                    );
                }
                _ => return Err(Error::Other("unexpected NacStatus variant".to_string())),
            }
        }

        Ok(())
    }

    /// Get the current NAC status.
    pub async fn status(&self) -> NacStatus {
        self.nac.status().await
    }

    /// Get the owner identity.
    pub async fn owner(&self) -> Option<Did> {
        self.nac.owner().await
    }

    /// Check if NAC is effectively enabled.
    pub async fn is_enabled(&self) -> bool {
        self.nac.status().await == NacStatus::Enabled
    }

    /// Check if an identity has a specific node permission.
    pub async fn check_permission(
        &self,
        identity: &Did,
        permission: NodePermission,
    ) -> Result<bool> {
        if let Some(scope) = self
            .nac
            .begin_scope(true)
            .await
            .map_err(|error| Error::Acp(error.to_string()))?
        {
            let scoped = self.scoped(scope.store());
            scoped
                .nac
                .load()
                .await
                .map_err(|error| Error::Acp(error.to_string()))?;
            let scoped: Box<dyn NacManagerApi> = Box::new(scoped);
            let result = scoped.check_permission(identity, permission).await?;
            drop(scoped);
            scope
                .commit()
                .await
                .map_err(|error| Error::Acp(error.to_string()))?;
            return Ok(result);
        }

        self.nac
            .check_permission(identity, permission)
            .await
            .map_err(|e| Error::Acp(format!("permission check failed: {}", e)))
    }

    /// Check if an identity is an admin.
    pub async fn is_admin(&self, identity: &Did) -> Result<bool> {
        if let Some(scope) = self
            .nac
            .begin_scope(true)
            .await
            .map_err(|error| Error::Acp(error.to_string()))?
        {
            let scoped = self.scoped(scope.store());
            scoped
                .nac
                .load()
                .await
                .map_err(|error| Error::Acp(error.to_string()))?;
            let scoped: Box<dyn NacManagerApi> = Box::new(scoped);
            let result = scoped.is_admin(identity).await?;
            drop(scoped);
            scope
                .commit()
                .await
                .map_err(|error| Error::Acp(error.to_string()))?;
            return Ok(result);
        }

        self.nac
            .is_admin(identity)
            .await
            .map_err(|e| Error::Acp(format!("admin check failed: {}", e)))
    }

    /// Check if an identity is an admin based on stored relationships.
    pub async fn is_admin_persisted(&self, identity: &Did) -> Result<bool> {
        if let Some(scope) = self
            .nac
            .begin_scope(true)
            .await
            .map_err(|error| Error::Acp(error.to_string()))?
        {
            let scoped = self.scoped(scope.store());
            scoped
                .nac
                .load()
                .await
                .map_err(|error| Error::Acp(error.to_string()))?;
            let scoped: Box<dyn NacManagerApi> = Box::new(scoped);
            let result = scoped.is_admin_persisted(identity).await?;
            drop(scoped);
            scope
                .commit()
                .await
                .map_err(|error| Error::Acp(error.to_string()))?;
            return Ok(result);
        }

        self.nac
            .is_admin_persisted(identity)
            .await
            .map_err(|e| Error::Acp(format!("admin check failed: {}", e)))
    }

    /// Check if an identity is the owner.
    pub async fn is_owner(&self, identity: &Did) -> bool {
        self.nac.is_owner(identity).await
    }

    /// Enable NAC with the given owner.
    pub async fn enable(&self, owner: &Did) -> Result<()> {
        if let Some(scope) = self
            .nac
            .begin_scope(false)
            .await
            .map_err(|error| Error::Acp(error.to_string()))?
        {
            let scoped = self.scoped(scope.store());
            scoped
                .nac
                .load()
                .await
                .map_err(|error| Error::Acp(error.to_string()))?;
            let scoped: Box<dyn NacManagerApi> = Box::new(scoped);
            scoped.enable(owner).await?;
            drop(scoped);
            scope
                .commit()
                .await
                .map_err(|error| Error::Acp(error.to_string()))?;
            self.nac
                .load()
                .await
                .map_err(|error| Error::Acp(error.to_string()))?;
            return Ok(());
        }

        self.nac
            .enable(owner)
            .await
            .map_err(|e| Error::Acp(format!("failed to enable NAC: {}", e)))
    }

    /// Purge all NAC state (dev mode only).
    pub async fn purge(&self, requestor: &Did) -> Result<()> {
        if let Some(scope) = self
            .nac
            .begin_scope(false)
            .await
            .map_err(|error| Error::Acp(error.to_string()))?
        {
            let scoped = self.scoped(scope.store());
            scoped
                .nac
                .load()
                .await
                .map_err(|error| Error::Acp(error.to_string()))?;
            let scoped: Box<dyn NacManagerApi> = Box::new(scoped);
            scoped.purge(requestor).await?;
            drop(scoped);
            scope
                .commit()
                .await
                .map_err(|error| Error::Acp(error.to_string()))?;
            self.nac
                .load()
                .await
                .map_err(|error| Error::Acp(error.to_string()))?;
            return Ok(());
        }

        if !self.config.dev_mode {
            return Err(Error::Acp("NAC purge is only allowed in dev mode".into()));
        }

        if !self.is_admin(requestor).await? {
            return Err(Error::Acp("only admins can purge NAC".into()));
        }

        self.nac
            .purge()
            .await
            .map_err(|e| Error::Acp(format!("failed to purge NAC: {}", e)))
    }

    /// Add an admin relationship.
    pub async fn add_admin(&self, requestor: &Did, target: &Did) -> Result<bool> {
        let asked = (requestor.clone(), target.clone());
        let written = self
            .in_write(move |manager| {
                let (requestor, target) = asked.clone();
                async move { manager.add_admin_direct(&requestor, &target).await }
            })
            .await?;
        if let Some(added) = written {
            return Ok(added);
        }
        self.add_admin_direct(requestor, target).await
    }

    /// [`add_admin`](Self::add_admin) on this manager's own store.
    async fn add_admin_direct(&self, requestor: &Did, target: &Did) -> Result<bool> {
        self.nac
            .add_admin(requestor, target)
            .await
            .map_err(|error| Error::acp("add admin", error))
    }

    /// Remove an admin relationship.
    pub async fn remove_admin(&self, requestor: &Did, target: &Did) -> Result<bool> {
        let asked = (requestor.clone(), target.clone());
        let written = self
            .in_write(move |manager| {
                let (requestor, target) = asked.clone();
                async move { manager.remove_admin_direct(&requestor, &target).await }
            })
            .await?;
        if let Some(removed) = written {
            return Ok(removed);
        }
        self.remove_admin_direct(requestor, target).await
    }

    /// [`remove_admin`](Self::remove_admin) on this manager's own store.
    async fn remove_admin_direct(&self, requestor: &Did, target: &Did) -> Result<bool> {
        self.nac
            .remove_admin(requestor, target)
            .await
            .map_err(|error| Error::acp("remove admin", error))
    }

    /// Grant a specific permission to an identity.
    pub async fn add_permission_grant(
        &self,
        requestor: &Did,
        target: &Did,
        permission: NodePermission,
    ) -> Result<bool> {
        let asked = (requestor.clone(), target.clone());
        let written = self
            .in_write(move |manager| {
                let (requestor, target) = asked.clone();
                async move {
                    manager
                        .add_permission_grant_direct(&requestor, &target, permission)
                        .await
                }
            })
            .await?;
        if let Some(added) = written {
            return Ok(added);
        }
        self.add_permission_grant_direct(requestor, target, permission)
            .await
    }

    /// [`add_permission_grant`](Self::add_permission_grant) on this
    /// manager's own store.
    async fn add_permission_grant_direct(
        &self,
        requestor: &Did,
        target: &Did,
        permission: NodePermission,
    ) -> Result<bool> {
        self.nac
            .add_permission_grant(requestor, target, permission)
            .await
            .map_err(|error| Error::acp("grant permission", error))
    }

    /// Revoke a specific permission from an identity.
    pub async fn remove_permission_grant(
        &self,
        requestor: &Did,
        target: &Did,
        permission: NodePermission,
    ) -> Result<bool> {
        let asked = (requestor.clone(), target.clone());
        let written = self
            .in_write(move |manager| {
                let (requestor, target) = asked.clone();
                async move {
                    manager
                        .remove_permission_grant_direct(&requestor, &target, permission)
                        .await
                }
            })
            .await?;
        if let Some(removed) = written {
            return Ok(removed);
        }
        self.remove_permission_grant_direct(requestor, target, permission)
            .await
    }

    /// [`remove_permission_grant`](Self::remove_permission_grant) on this
    /// manager's own store.
    async fn remove_permission_grant_direct(
        &self,
        requestor: &Did,
        target: &Did,
        permission: NodePermission,
    ) -> Result<bool> {
        self.nac
            .remove_permission_grant(requestor, target, permission)
            .await
            .map_err(|error| Error::acp("revoke permission", error))
    }

    /// Get the underlying NAC config.
    pub fn config(&self) -> &NacConfig {
        &self.config
    }

    /// Get NAC info for HTTP responses.
    pub async fn info(&self) -> NacInfo {
        let status = self.status().await;
        let owner = self.owner().await;

        NacInfo {
            status: status.to_string(),
            configured_enabled: self.config.enabled,
            dev_mode: self.config.dev_mode,
            owner: owner.map(|d| d.to_string()),
        }
    }
}

/// Information about NAC status for HTTP responses.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NacInfo {
    /// Current NAC status
    pub status: String,

    /// Whether NAC is configured to be enabled
    pub configured_enabled: bool,

    /// Whether dev mode is enabled
    pub dev_mode: bool,

    /// Owner DID if NAC is enabled
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
}
