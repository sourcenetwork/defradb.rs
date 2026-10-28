use std::sync::Arc;

use rapidhash::fast::RandomState;
use regolith::sync::{Event, HopscotchMap};

type Waiters = HopscotchMap<(String, u64), Arc<Event>, RandomState>;

/// Notifications are hints to recheck a durable install, never storage guards.
pub(crate) struct ReplicatorInstalls {
    waiters: Arc<Waiters>,
    pub(super) runtime: Arc<db::database::replication::ReplicationRuntime>,
}

impl Default for ReplicatorInstalls {
    fn default() -> Self {
        Self::new(Arc::default())
    }
}

pub(crate) struct InstallWatch {
    waiters: Arc<Waiters>,
    key: (String, u64),
    event: Arc<Event>,
}

impl ReplicatorInstalls {
    pub(crate) fn new(runtime: Arc<db::database::replication::ReplicationRuntime>) -> Self {
        Self {
            waiters: runtime.notifications(),
            runtime,
        }
    }

    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.waiters.is_empty()
    }

    pub(crate) fn watch(&self, peer: &str, id: u64) -> InstallWatch {
        let key = (peer.to_owned(), id);
        let candidate = Arc::new(Event::new());
        let event = self
            .waiters
            .insert_if_absent(key.clone(), candidate.clone())
            .unwrap_or(candidate);
        InstallWatch {
            waiters: Arc::clone(&self.waiters),
            key,
            event,
        }
    }

    pub(crate) fn finish(&self, peer: &str, id: u64) {
        if let Some(event) = self.waiters.remove(&(peer.to_owned(), id)) {
            event.set();
        }
    }
}

impl InstallWatch {
    pub(crate) async fn wait(&self) {
        self.event.wait().await;
    }
}

impl Drop for InstallWatch {
    fn drop(&mut self) {
        if let Some(event) = self
            .waiters
            .remove_if(&self.key, |event| Arc::ptr_eq(event, &self.event))
        {
            // Cancellation wakes other waiters so they can recheck storage.
            event.set();
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/replicator_installs.rs"]
mod tests;
