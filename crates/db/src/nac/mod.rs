//! Node Access Control state, factory and trait wiring.

mod edge;
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
    #[cfg(not(target_arch = "wasm32"))]
    fn attempts(&self) -> u32 {
        storage::corekv::engine_retry_options(
            self.max_txn_retries
                .unwrap_or(storage::corekv::DEFAULT_MAX_TXN_RETRIES),
        )
        .max_attempts
    }
}

/// One source-scoped native permission relationship change.
pub struct SourcePermissionChange<'a> {
    pub permission: NodePermission,
    pub parent_source: Option<&'a str>,
    pub subject_kind: &'a str,
    pub subject: &'a str,
    pub delete: bool,
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
    /// [`Self::check_permission`] for each of `permissions` in order, up to
    /// and including the first that is not granted: one outcome per
    /// permission checked. A provider may read them all at one snapshot.
    async fn check_permissions(
        &self,
        identity: &Did,
        permissions: &[NodePermission],
    ) -> Vec<Result<bool>> {
        let mut outcomes = Vec::with_capacity(permissions.len());
        for &permission in permissions {
            let outcome = self.check_permission(identity, permission).await;
            let granted = matches!(outcome, Ok(true));
            outcomes.push(outcome);
            if !granted {
                break;
            }
        }
        outcomes
    }
    /// [`Self::check_permissions`] answered from memory, without reading
    /// storage: `None` when this provider cannot. A caller off the
    /// database's workers can take this answer in place.
    async fn check_permissions_in_memory(
        &self,
        _identity: &Did,
        _permissions: &[NodePermission],
    ) -> Option<Vec<Result<bool>>> {
        None
    }
    /// A transaction on `view` may write NAC tuples without this manager (a
    /// document projector writes them straight into the ACP store): withdraw
    /// the edge snapshot now and rebuild it once the transaction commits, as
    /// a write made through this manager does.
    async fn refresh_edge_on_commit_of(&self, _view: &datastore::NamespaceView) -> Result<()> {
        Ok(())
    }
    async fn check_permission_in_txn(
        &self,
        _view: datastore::NamespaceView,
        identity: &Did,
        permission: NodePermission,
    ) -> Result<bool> {
        self.check_permission(identity, permission).await
    }
    /// Evaluate NAC through the given transaction. None means the provider
    /// cannot bind, or NAC is inactive when active NAC is required. The view
    /// must share this manager's store; this method does not verify identity.
    async fn check_bound_permission_in_txn(
        &self,
        _view: datastore::NamespaceView,
        _identity: &Did,
        _permission: NodePermission,
        _require_active: bool,
    ) -> Result<Option<bool>> {
        Ok(None)
    }
    /// Change a source relationship only through the caller's bound store.
    async fn change_source_permission_in_txn(
        &self,
        _view: datastore::NamespaceView,
        _requestor: &Did,
        _change: &SourcePermissionChange<'_>,
    ) -> Result<Option<bool>> {
        Ok(None)
    }
    /// Observe whether a source edge already matches, on the bound transaction.
    async fn source_permission_matches_in_txn(
        &self,
        _view: datastore::NamespaceView,
        _change: &SourcePermissionChange<'_>,
    ) -> Result<Option<bool>> {
        Ok(None)
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
    /// Every way `subject` holds each node permission, for a requester who
    /// is `subject`, an admin, or holds `NacStatus`. A refusal is
    /// [`Error::PermissionDenied`].
    async fn explain_subject(
        &self,
        _requestor: &Did,
        _subject: &Did,
    ) -> Result<Vec<acp::nac::PermissionProvenance>> {
        Err(Error::Other(
            "node permission explanations are unsupported by this provider".into(),
        ))
    }
}

/// `nac`'s check of each of `permissions` in order, up to and including the
/// first that is not granted.
async fn first_refusal(
    nac: &dyn NacManagerApi,
    identity: &Did,
    permissions: &[NodePermission],
) -> Vec<Result<bool>> {
    let mut outcomes = Vec::with_capacity(permissions.len());
    for &permission in permissions {
        let outcome = nac.check_permission(identity, permission).await;
        let granted = matches!(outcome, Ok(true));
        outcomes.push(outcome);
        if !granted {
            break;
        }
    }
    outcomes
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
    transaction_store_bound: bool,
    /// The committed grants edge checks read; see [`edge`].
    edge: Arc<edge::EdgeGrants>,
}

impl<S: ZanzibarStore + ?Sized + 'static> NacManager<S> {
    /// Create a new NAC manager with the given store and config.
    pub fn new(store: Arc<S>, config: NacConfig) -> Self {
        Self {
            nac: Arc::new(NodeACP::new(store)),
            config,
            transaction_store_bound: false,
            edge: Arc::new(edge::EdgeGrants::new()),
        }
    }

    /// The caller supplies the same storage engine used by its database transactions.
    pub fn new_bound(store: Arc<S>, config: NacConfig) -> Self {
        Self {
            nac: Arc::new(NodeACP::new(store)),
            config,
            transaction_store_bound: true,
            edge: Arc::new(edge::EdgeGrants::new()),
        }
    }

    /// A manager over `store` (a transaction or a scope) that shares this
    /// one's policy snapshot. It has no edge snapshot of its own: its checks
    /// read `store`.
    fn scoped<T>(&self, store: Arc<T>) -> NacManager<T>
    where
        T: ZanzibarStore + ?Sized + 'static,
    {
        NacManager {
            nac: Arc::new(self.nac.scoped(store)),
            config: self.config.clone(),
            transaction_store_bound: self.transaction_store_bound,
            edge: Arc::new(edge::EdgeGrants::new()),
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
            self.reload_after_commit().await?;
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
        if let Some(grants) = self.edge.current() {
            return grants.check(identity, permission).await;
        }
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

    /// [`Self::check_permission`] for each of `permissions` in order, read in
    /// one scope, up to and including the first that is not granted.
    pub async fn check_permissions(
        &self,
        identity: &Did,
        permissions: &[NodePermission],
    ) -> Vec<Result<bool>> {
        if let Some(grants) = self.edge.current() {
            return grants.check_all(identity, permissions).await;
        }
        let scope = match self.nac.begin_scope(true).await {
            Ok(scope) => scope,
            Err(error) => return vec![Err(Error::Acp(error.to_string()))],
        };
        let Some(scope) = scope else {
            return first_refusal(self, identity, permissions).await;
        };
        let scoped = self.scoped(scope.store());
        if let Err(error) = scoped.nac.load().await {
            return vec![Err(Error::Acp(error.to_string()))];
        }
        let scoped: Box<dyn NacManagerApi> = Box::new(scoped);
        let outcomes = first_refusal(scoped.as_ref(), identity, permissions).await;
        drop(scoped);
        // A scope that fails to commit fails the first check, as it does
        // when each permission is read in a scope of its own.
        if let Err(error) = scope.commit().await {
            return vec![Err(Error::Acp(error.to_string()))];
        }
        outcomes
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
            self.reload_after_commit().await?;
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
            self.reload_after_commit().await?;
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
    /// Every way `subject` holds each node permission, read in one snapshot.
    pub async fn explain_subject(
        &self,
        requestor: &Did,
        subject: &Did,
    ) -> Result<Vec<acp::nac::PermissionProvenance>> {
        let refused = |error: acp::error::Error| match error {
            acp::error::Error::NotOwner { .. } => Error::PermissionDenied,
            other => Error::Acp(other.to_string()),
        };
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
            let result = scoped
                .nac
                .explain_subject(requestor, subject)
                .await
                .map_err(refused)?;
            drop(scoped);
            scope
                .commit()
                .await
                .map_err(|error| Error::Acp(error.to_string()))?;
            return Ok(result);
        }
        self.nac
            .explain_subject(requestor, subject)
            .await
            .map_err(refused)
    }

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
