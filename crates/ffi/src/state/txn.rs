use std::sync::Arc;

use kovan_map::HopscotchMap;
use rapidhash::fast::RandomState;
use regolith::sync::Lazy;

use super::NodeHandle;

/// Type alias for transaction handles (opaque to FFI callers).
pub type TxnHandle = usize;

/// An explicit transaction a C caller holds a handle to.
pub struct TxnState {
    /// The node the transaction runs on.
    pub node: NodeHandle,
    /// The transaction's ID.
    pub id: defra_core::TxnId,
}

/// Global registry of open transactions.
pub struct TxnRegistry {
    txns: HopscotchMap<TxnHandle, Arc<TxnState>, RandomState>,
}

impl TxnRegistry {
    fn new() -> Self {
        Self {
            txns: HopscotchMap::with_hasher(RandomState::default()),
        }
    }

    /// Insert an open transaction and return its handle.
    pub fn insert(&self, state: TxnState) -> TxnHandle {
        let handle = super::handle::next_handle();
        self.txns.insert(handle, Arc::new(state));
        handle
    }

    /// The transaction a handle names, if it names one.
    pub fn get(&self, handle: TxnHandle) -> Option<Arc<TxnState>> {
        self.txns.get(&handle)
    }

    /// Remove and return a transaction.
    pub fn remove(&self, handle: TxnHandle) -> Option<Arc<TxnState>> {
        self.txns.remove(&handle)
    }

    /// Remove every transaction of a node, as the node closes.
    pub fn remove_for_node(&self, node: NodeHandle) {
        let handles: Vec<TxnHandle> = self
            .txns
            .iter()
            .filter(|(_, state)| state.node == node)
            .map(|(handle, _)| handle)
            .collect();
        for handle in handles {
            self.txns.remove(&handle);
        }
    }
}

/// Global TXNS registry.
pub static TXNS: Lazy<TxnRegistry> = Lazy::new(TxnRegistry::new);
