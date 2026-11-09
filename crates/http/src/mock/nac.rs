//! Mock NAC (Node Access Control) operations for testing.

use async_trait::async_trait;
use identity::Did;
use kovan::{Atom, AtomOption};
use kovan_map::HopscotchMap;
use rapidhash::fast::RandomState;
use std::sync::Arc;

use crate::mock::update_vec;
use crate::router::{NacStatus, NodeAcpOperations, NodePermission};

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

    async fn enable(&self, owner: &Did) -> Result<(), String> {
        let status = *self.status.load();
        if status != NacStatus::NotConfigured {
            return Err("NAC is already configured".into());
        }
        self.status.store(NacStatus::Enabled);
        self.owner.store_some(owner.clone());
        Ok(())
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

    async fn enable(&self, _owner: &Did) -> Result<(), String> {
        Err(self.error.clone())
    }
}
