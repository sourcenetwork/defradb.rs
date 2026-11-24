//! Wake pending DAG retries when remote keys become available.

use std::sync::{Arc, Weak};

use blockstore::Blockstore;
use p2p::{sync::SyncCoordinator, P2PTransport};

struct KeyArrivalPromoter<B, T>
where
    B: Blockstore,
    T: P2PTransport,
{
    coordinator: Weak<SyncCoordinator<B, T>>,
}

impl<B, T> kms::KeyArrivalListener for KeyArrivalPromoter<B, T>
where
    B: Blockstore + 'static,
    T: P2PTransport + 'static,
    SyncCoordinator<B, T>: Send + Sync,
{
    fn keys_arrived(&self, _cids: &[kms::EncryptionCid]) {
        if let Some(coordinator) = self.coordinator.upgrade() {
            coordinator.manager().promote_pending_retries();
        }
    }
}

/// Create a listener that does not keep the coordinator alive.
pub fn key_arrival_promoter<B, T>(
    coordinator: Weak<SyncCoordinator<B, T>>,
) -> Arc<dyn kms::KeyArrivalListener>
where
    B: Blockstore + 'static,
    T: P2PTransport + 'static,
    SyncCoordinator<B, T>: Send + Sync,
{
    Arc::new(KeyArrivalPromoter { coordinator })
}
