//! Node Access Control (NAC) implementation.
//!
//! NAC provides node-level access control using the Zanzibar permission model.
//!
//! ## Security Features
//!
//! - **Write blocking when disabled**: All relationship modifications are blocked
//!   when NAC is disabled to prevent privilege escalation attacks.

mod lifecycle;
mod operations;

use std::sync::Arc;
use storage::corekv::MaybeSendSync;

use crate::policy_cache::PolicyCache;
use async_trait::async_trait;
use identity::Did;
use kovan::{Atom, AtomOption};

use crate::error::{Error, Result};
use crate::nac::permission::NodePermission;
use crate::nac::policy::{
    validate_node_policy, NODE_POLICY_ID, NODE_RESOURCE_NAME, OWNER_RELATION,
};
use zanzibar::{Subject, ZanzibarStore};

/// The fixed object ID for the node resource.
/// There's only one "node" instance, so we use a constant ID.
pub const NODE_OBJECT_ID: &str = "singleton";

/// Sentinel relation name used to persist "disabled temporarily" status.
/// Stored as a relationship in the Zanzibar store so it survives restarts.
const DISABLED_RELATION: &str = "_disabled";

/// NAC lifecycle state machine.
///
/// ```text
/// NotConfigured --[enable()]--> Enabled
/// Enabled --[disable()]--> DisabledTemporarily
/// DisabledTemporarily --[re_enable()]--> Enabled
/// Enabled --[purge()]--> NotConfigured
/// DisabledTemporarily --[purge()]--> NotConfigured
/// ```
///
/// - `NotConfigured`: NAC has never been enabled or has been purged.
///   All node operations are unrestricted.
/// - `Enabled`: NAC is active. All node operations require permission checks.
/// - `DisabledTemporarily`: NAC is paused. Permission checks are bypassed but
///   write operations (adding/removing admins) are blocked to prevent
///   privilege escalation. State is preserved for re-enable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NacStatus {
    NotConfigured,
    Enabled,
    DisabledTemporarily,
}

impl std::fmt::Display for NacStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotConfigured => write!(f, "not configured"),
            Self::Enabled => write!(f, "enabled"),
            Self::DisabledTemporarily => write!(f, "disabled temporarily"),
        }
    }
}

/// Node Access Control implementation.
///
/// Uses a local Zanzibar store to manage node-level permissions.
/// Unlike DAC, NAC is always local (no Vera option).
pub struct NodeACP<S: ZanzibarStore + ?Sized + 'static> {
    store: Arc<S>,
    engine: PolicyCache<S>,
    status: Atom<NacStatus>,
    owner: AtomOption<Did>,
}

impl<S: ZanzibarStore + ?Sized + 'static> NodeACP<S> {
    pub async fn begin_scope(
        &self,
        readonly: bool,
    ) -> zanzibar::error::Result<Option<Box<dyn zanzibar::store::ZanzibarScope>>> {
        self.store.begin_scope(readonly).await
    }

    /// [`ZanzibarStore::transact`] on this NodeACP's store.
    pub async fn transact(
        &self,
        max_attempts: u32,
        attempt: zanzibar::store::ScopeAttempt,
    ) -> zanzibar::error::Result<Option<zanzibar::store::ScopeOutcome>> {
        self.store.transact(max_attempts, attempt).await
    }

    /// Take the status and owner a committed write left, as read by the
    /// scope that wrote it.
    pub fn publish(&self, status: NacStatus, owner: Option<Did>) {
        match owner {
            Some(owner) => self.owner.store_some(owner),
            None => self.owner.store_none(),
        }
        self.status.store(status);
    }

    pub fn supports_transactions(&self) -> bool {
        self.store.supports_transactions()
    }

    /// Create a new NodeACP with the given store.
    ///
    /// Starts in NotConfigured status. Call `enable()` to activate.
    pub fn new(store: Arc<S>) -> Self {
        Self {
            store: store.clone(),
            engine: PolicyCache::new(store),
            status: Atom::new(NacStatus::NotConfigured),
            owner: AtomOption::none(),
        }
    }

    /// A NodeACP over `store` (a transaction or a scope) that shares this
    /// one's policy snapshot. Its state is unknown until [`load`](Self::load).
    pub fn scoped<T>(&self, store: Arc<T>) -> NodeACP<T>
    where
        T: ZanzibarStore + ?Sized + 'static,
    {
        NodeACP {
            engine: self.engine.sharing(store.clone()),
            store,
            status: Atom::new(NacStatus::NotConfigured),
            owner: AtomOption::none(),
        }
    }

    /// Load existing NAC state from the store.
    ///
    /// Should be called during startup to restore NAC state. Whether NAC is
    /// configured, its owner and its disabled flag are read from the store;
    /// the policy body is parsed only when the shared snapshot lacks it. Only
    /// `enable` writes that body, always [`create_node_policy`], so publishing
    /// what a scope read cannot put a different body in the snapshot.
    ///
    /// [`create_node_policy`]: crate::nac::create_node_policy
    pub async fn load(&self) -> Result<()> {
        let configured = if self.engine.snapshot().lookup.has_policy(NODE_POLICY_ID) {
            self.store.has_policy(NODE_POLICY_ID).await?
        } else if let Some(policy) = self.store.get_policy(NODE_POLICY_ID).await? {
            validate_node_policy(&policy).map_err(Error::InvalidPolicy)?;
            self.engine.edit(|engine| engine.add_policy(&policy));
            true
        } else {
            false
        };
        if configured {
            let subjects = self
                .store
                .get_relation_subjects(
                    NODE_POLICY_ID,
                    NODE_RESOURCE_NAME,
                    NODE_OBJECT_ID,
                    OWNER_RELATION,
                )
                .await?;

            if let Some(Subject::Entity(owner_did)) = subjects.first() {
                self.owner.store_some(owner_did.clone());

                let disabled_subjects = self
                    .store
                    .get_relation_subjects(
                        NODE_POLICY_ID,
                        NODE_RESOURCE_NAME,
                        NODE_OBJECT_ID,
                        DISABLED_RELATION,
                    )
                    .await?;

                if disabled_subjects.is_empty() {
                    self.status.store(NacStatus::Enabled);
                } else {
                    self.status.store(NacStatus::DisabledTemporarily);
                }

                tracing::debug!(
                    target: "nac::audit",
                    event = "nac_loaded",
                    owner = %owner_did,
                    disabled = !disabled_subjects.is_empty(),
                    "NAC state loaded from store"
                );
            }
        } else {
            self.owner.store_none();
            self.status.store(NacStatus::NotConfigured);
        }

        Ok(())
    }

    pub async fn status(&self) -> NacStatus {
        *self.status.load()
    }

    pub async fn owner(&self) -> Option<Did> {
        self.owner.load().map(|g| (*g).clone())
    }
}

/// Trait for NAC operations accessible via HTTP.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait NodeAcpOperations: MaybeSendSync {
    async fn check_permission(&self, identity: &Did, permission: NodePermission) -> Result<bool>;
    async fn get_status(&self) -> NacStatus;
    async fn add_admin(&self, requestor: &Did, target: &Did) -> Result<bool>;
    async fn remove_admin(&self, requestor: &Did, target: &Did) -> Result<bool>;
    async fn owner(&self) -> Option<Did>;
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S: ZanzibarStore + ?Sized + 'static> NodeAcpOperations for NodeACP<S> {
    async fn check_permission(&self, identity: &Did, permission: NodePermission) -> Result<bool> {
        NodeACP::check_permission(self, identity, permission).await
    }

    async fn get_status(&self) -> NacStatus {
        self.status().await
    }

    async fn add_admin(&self, requestor: &Did, target: &Did) -> Result<bool> {
        NodeACP::add_admin(self, requestor, target).await
    }

    async fn remove_admin(&self, requestor: &Did, target: &Did) -> Result<bool> {
        NodeACP::remove_admin(self, requestor, target).await
    }

    async fn owner(&self) -> Option<Did> {
        NodeACP::owner(self).await
    }
}
