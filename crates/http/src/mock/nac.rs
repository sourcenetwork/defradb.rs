//! Mock NAC (Node Access Control) operations for testing.

use async_trait::async_trait;
use identity::Did;
use kovan::{Atom, AtomOption};
use kovan_map::HopscotchMap;
use rapidhash::fast::RandomState;
use std::sync::Arc;

use crate::mock::update_vec;
use crate::router::{NacStatus, NacStatusInfo, NodeAcpOperations, NodePermission};

/// Mock NAC operations for testing NAC-protected handlers.
///
/// Configurable mock that allows controlling:
/// - NAC status (enabled, disabled, not configured)
/// - Owner identity
/// - Admin identities
/// - Permission grants
pub struct MockNodeAcpOperations {
    status: Arc<Atom<NacStatus>>,
    owner: Arc<AtomOption<Did>>,
    admins: Arc<HopscotchMap<Did, (), RandomState>>,
    /// Permission grants: (identity, permission) pairs
    grants: Arc<Atom<Vec<(Did, NodePermission)>>>,
}

impl Clone for MockNodeAcpOperations {
    fn clone(&self) -> Self {
        Self {
            status: Arc::clone(&self.status),
            owner: Arc::clone(&self.owner),
            admins: Arc::clone(&self.admins),
            grants: Arc::clone(&self.grants),
        }
    }
}

impl Default for MockNodeAcpOperations {
    fn default() -> Self {
        Self::new()
    }
}

impl MockNodeAcpOperations {
    /// Create a new mock NAC with NAC not configured (permissive).
    pub fn new() -> Self {
        Self {
            status: Arc::new(Atom::new(NacStatus::NotConfigured)),
            owner: Arc::new(AtomOption::none()),
            admins: Arc::new(HopscotchMap::with_hasher(RandomState::default())),
            grants: Arc::new(Atom::new(vec![])),
        }
    }

    /// Create with NAC enabled and the given owner.
    pub fn enabled_with_owner(owner: Did) -> Self {
        Self {
            status: Arc::new(Atom::new(NacStatus::Enabled)),
            owner: Arc::new(AtomOption::some(owner)),
            admins: Arc::new(HopscotchMap::with_hasher(RandomState::default())),
            grants: Arc::new(Atom::new(vec![])),
        }
    }

    /// Create with NAC disabled temporarily.
    pub fn disabled() -> Self {
        Self {
            status: Arc::new(Atom::new(NacStatus::DisabledTemporarily)),
            owner: Arc::new(AtomOption::none()),
            admins: Arc::new(HopscotchMap::with_hasher(RandomState::default())),
            grants: Arc::new(Atom::new(vec![])),
        }
    }

    /// Add an admin identity.
    pub fn with_admin(self, admin: Did) -> Self {
        self.admins.insert(admin, ());
        self
    }

    /// Add a permission grant.
    pub fn with_grant(self, identity: Did, permission: NodePermission) -> Self {
        update_vec(&self.grants, |grants| {
            grants.push((identity.clone(), permission))
        });
        self
    }
}

#[async_trait]
impl NodeAcpOperations for MockNodeAcpOperations {
    async fn check_permission(
        &self,
        identity: &Did,
        permission: NodePermission,
    ) -> Result<bool, String> {
        let status = *self.status.load();

        // If NAC is not enabled, allow all
        if status != NacStatus::Enabled {
            return Ok(true);
        }

        // Check if owner
        if let Some(owner) = self.owner.load() {
            if *owner == *identity {
                return Ok(true);
            }
        }

        // Check if admin (admins have all permissions)
        if self.admins.contains_key(identity) {
            return Ok(true);
        }

        // Check specific permission grants
        Ok(self.grants.peek(|grants| {
            grants
                .iter()
                .any(|(grantee, perm)| grantee == identity && *perm == permission)
        }))
    }

    async fn get_status(&self) -> NacStatus {
        *self.status.load()
    }

    async fn owner(&self) -> Option<Did> {
        self.owner.load().map(|owner| (*owner).clone())
    }

    async fn is_admin(&self, identity: &Did) -> Result<bool, String> {
        let status = *self.status.load();
        if status != NacStatus::Enabled {
            return Ok(true); // Everyone is admin when NAC is disabled
        }

        // Check if owner
        if let Some(owner) = self.owner.load() {
            if *owner == *identity {
                return Ok(true);
            }
        }

        // Check admins list
        Ok(self.admins.contains_key(identity))
    }

    async fn add_admin(&self, _requestor: &Did, target: &Did) -> Result<bool, String> {
        let status = *self.status.load();
        if status == NacStatus::DisabledTemporarily {
            return Err(
                "cannot modify relationships while NAC is disabled - re-enable NAC first".into(),
            );
        }

        Ok(self.admins.insert_if_absent(target.clone(), ()).is_none())
    }

    async fn remove_admin(&self, _requestor: &Did, target: &Did) -> Result<bool, String> {
        let status = *self.status.load();
        if status == NacStatus::DisabledTemporarily {
            return Err(
                "cannot modify relationships while NAC is disabled - re-enable NAC first".into(),
            );
        }

        // Cannot remove owner
        if let Some(owner) = self.owner.load() {
            if *owner == *target {
                return Err("cannot remove owner's admin access".into());
            }
        }

        Ok(self.admins.remove(target).is_some())
    }

    async fn disable(&self, _requestor: &Did) -> Result<(), String> {
        self.status.store(NacStatus::DisabledTemporarily);
        Ok(())
    }

    async fn re_enable(&self, _requestor: &Did) -> Result<(), String> {
        self.status.store(NacStatus::Enabled);
        Ok(())
    }

    async fn enable(&self, owner: &Did) -> Result<(), String> {
        let status = *self.status.load();
        if status != NacStatus::NotConfigured {
            return Err("NAC is already configured".into());
        }
        self.status.store(NacStatus::Enabled);
        self.owner.store_some(owner.clone());
        Ok(())
    }

    async fn add_relationship(
        &self,
        requestor: &Did,
        target: &Did,
        relation: &str,
    ) -> Result<bool, String> {
        if relation == "owner" {
            return Err("relation not in resource".into());
        }
        if relation == "admin" {
            return self.add_admin(requestor, target).await;
        }
        if NodePermission::parse(relation).is_some() {
            let status = *self.status.load();
            if status == NacStatus::DisabledTemporarily {
                return Err(
                    "cannot modify relationships while NAC is disabled - re-enable NAC first"
                        .into(),
                );
            }
            return Ok(true);
        }
        Err("relation not in resource".into())
    }

    async fn remove_relationship(
        &self,
        requestor: &Did,
        target: &Did,
        relation: &str,
    ) -> Result<bool, String> {
        if relation == "owner" {
            return Err("relation not in resource".into());
        }
        if relation == "admin" {
            return self.remove_admin(requestor, target).await;
        }
        if NodePermission::parse(relation).is_some() {
            let status = *self.status.load();
            if status == NacStatus::DisabledTemporarily {
                return Err(
                    "cannot modify relationships while NAC is disabled - re-enable NAC first"
                        .into(),
                );
            }
            return Ok(true);
        }
        Err("relation not in resource".into())
    }

    async fn info(&self) -> NacStatusInfo {
        let status = self.get_status().await;
        let owner = self.owner().await;
        NacStatusInfo {
            status: status.to_string(),
            configured_enabled: status == NacStatus::Enabled
                || status == NacStatus::DisabledTemporarily,
            dev_mode: false,
            owner: owner.map(|d| d.to_string()),
        }
    }
}

/// Mock NAC operations that always fails with a configurable error.
///
/// Use this to test error handling paths in handlers when NAC
/// permission checks fail with internal errors (not just permission denied).
#[derive(Debug, Clone)]
pub struct FailingMockNodeAcpOperations {
    error: String,
}

impl FailingMockNodeAcpOperations {
    /// Create a new failing mock with the given error message.
    pub fn new(error: impl Into<String>) -> Self {
        Self {
            error: error.into(),
        }
    }
}

#[async_trait]
impl NodeAcpOperations for FailingMockNodeAcpOperations {
    async fn check_permission(
        &self,
        _identity: &Did,
        _permission: NodePermission,
    ) -> Result<bool, String> {
        Err(self.error.clone())
    }

    async fn get_status(&self) -> NacStatus {
        NacStatus::Enabled
    }

    async fn owner(&self) -> Option<Did> {
        None
    }

    async fn is_admin(&self, _identity: &Did) -> Result<bool, String> {
        Err(self.error.clone())
    }

    async fn add_admin(&self, _requestor: &Did, _target: &Did) -> Result<bool, String> {
        Err(self.error.clone())
    }

    async fn remove_admin(&self, _requestor: &Did, _target: &Did) -> Result<bool, String> {
        Err(self.error.clone())
    }

    async fn disable(&self, _requestor: &Did) -> Result<(), String> {
        Err(self.error.clone())
    }

    async fn re_enable(&self, _requestor: &Did) -> Result<(), String> {
        Err(self.error.clone())
    }

    async fn enable(&self, _owner: &Did) -> Result<(), String> {
        Err(self.error.clone())
    }

    async fn add_relationship(
        &self,
        _requestor: &Did,
        _target: &Did,
        _relation: &str,
    ) -> Result<bool, String> {
        Err(self.error.clone())
    }

    async fn remove_relationship(
        &self,
        _requestor: &Did,
        _target: &Did,
        _relation: &str,
    ) -> Result<bool, String> {
        Err(self.error.clone())
    }

    async fn info(&self) -> NacStatusInfo {
        NacStatusInfo {
            status: "enabled".to_string(),
            configured_enabled: true,
            dev_mode: false,
            owner: None,
        }
    }
}
