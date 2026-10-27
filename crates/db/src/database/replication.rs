use std::sync::{Arc, Weak};

use rapidhash::fast::RandomState;
use regolith::sync::{Event, HopscotchMap};

pub type InstallNotifications = HopscotchMap<(String, u64), Arc<Event>, RandomState>;
type Owners = HopscotchMap<(String, u64), Weak<InstallOwner>, RandomState>;

#[cfg(test)]
#[path = "../../tests/database/replication_runtime.rs"]
mod tests;

/// All transport adapters share replay ownership and completion notifications.
pub struct ReplicationRuntime {
    notifications: Arc<InstallNotifications>,
    owners: Arc<Owners>,
}

impl Default for ReplicationRuntime {
    fn default() -> Self {
        Self {
            notifications: Arc::new(HopscotchMap::with_hasher(RandomState::default())),
            owners: Arc::new(HopscotchMap::with_hasher(RandomState::default())),
        }
    }
}

impl ReplicationRuntime {
    /// Register before the install transaction; its committed plan owns the token.
    pub fn own_install(&self, peer: &str, id: u64) -> Option<Arc<InstallOwner>> {
        let key = (peer.to_owned(), id);
        let owner = Arc::new_cyclic(|identity| InstallOwner {
            key: key.clone(),
            identity: identity.clone(),
            owners: Arc::clone(&self.owners),
        });
        if self
            .owners
            .insert_if_absent(key, Arc::downgrade(&owner))
            .is_some()
        {
            None
        } else {
            Some(owner)
        }
    }

    /// Recovery must never settle an install whose replay still has an owner.
    pub fn install_is_running(&self, peer: &str, id: u64) -> bool {
        self.owners
            .get(&(peer.to_owned(), id))
            .and_then(|owner| owner.upgrade())
            .is_some()
    }

    pub fn notifications(&self) -> Arc<InstallNotifications> {
        Arc::clone(&self.notifications)
    }
}

/// A replay's lifetime, not a guard on storage or on other callers.
pub struct InstallOwner {
    key: (String, u64),
    identity: Weak<Self>,
    owners: Arc<Owners>,
}

impl std::fmt::Debug for InstallOwner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("InstallOwner").field(&self.key).finish()
    }
}

impl Drop for InstallOwner {
    fn drop(&mut self) {
        self.owners
            .remove_if(&self.key, |owner| Weak::ptr_eq(owner, &self.identity));
    }
}
