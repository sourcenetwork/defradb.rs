use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

/// Serialize installations through completion of their background replay.
#[derive(Default)]
pub(crate) struct ReplicatorInstalls {
    peers: Mutex<HashMap<String, Weak<AsyncMutex<()>>>>,
}

#[cfg(test)]
#[path = "../tests/unit/replicator_installs.rs"]
mod tests;

impl ReplicatorInstalls {
    pub(crate) async fn acquire(&self, peer: &str) -> OwnedMutexGuard<()> {
        let lock = {
            let mut peers = self.peers.lock().unwrap_or_else(|error| error.into_inner());
            // Installs are infrequent; discard idle entries rather than retaining every past peer.
            peers.retain(|_, lock| lock.strong_count() > 0);
            let entry = peers.entry(peer.to_owned()).or_default();
            match entry.upgrade() {
                Some(lock) => lock,
                None => {
                    let lock = Arc::new(AsyncMutex::new(()));
                    *entry = Arc::downgrade(&lock);
                    lock
                }
            }
        };
        lock.lock_owned().await
    }
}
