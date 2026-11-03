use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Weak};

use kovan_map::HopscotchMap;
use kovan_queue::seg_queue::SegQueue;
use rapidhash::fast::RandomState;
use regolith::sync::Lazy;

use super::{GraphQLSubscriptionState, NodeHandle, NodeState, SubscriptionHandle};

/// A registered node: the single owning reference plus a borrowing handle.
///
/// The map retires a removed entry instead of dropping it, so the owning
/// reference sits in a queue that hands it back by value at `remove`. Closing
/// a node then releases its store, and with it the on-disk lock, at the
/// removal rather than whenever reclamation catches up.
struct NodeSlot {
    owner: SegQueue<Arc<NodeState>>,
    reader: Weak<NodeState>,
}

/// Global registry of active nodes.
pub struct NodeRegistry {
    nodes: HopscotchMap<NodeHandle, Arc<NodeSlot>, RandomState>,
}

impl NodeRegistry {
    /// Create a new empty registry.
    fn new() -> Self {
        Self {
            nodes: HopscotchMap::with_hasher(RandomState::default()),
        }
    }

    /// Insert a new node state and return its handle.
    pub fn insert(&self, state: NodeState) -> NodeHandle {
        let handle = super::handle::next_handle();
        let state = Arc::new(state);
        let slot = NodeSlot {
            owner: SegQueue::new(),
            reader: Arc::downgrade(&state),
        };
        slot.owner.push(state);
        self.nodes.insert(handle, Arc::new(slot));
        handle
    }

    /// Apply `f` to a node state.
    ///
    /// Returns None if the handle is invalid.
    pub fn get<F, R>(&self, handle: NodeHandle, f: F) -> Option<R>
    where
        F: FnOnce(&NodeState) -> R,
    {
        let slot = self.nodes.get(&handle)?;
        let state = slot.reader.upgrade()?;
        Some(f(&state))
    }

    /// Remove and return a node state.
    ///
    /// Returns None if the handle is invalid.
    pub fn remove(&self, handle: NodeHandle) -> Option<Arc<NodeState>> {
        self.nodes.remove(&handle)?.owner.pop()
    }

    /// Check if a handle is valid.
    pub fn contains(&self, handle: NodeHandle) -> bool {
        self.nodes.contains_key(&handle)
    }
}

/// The nodes the C API has open, by handle.
pub static NODES: Lazy<NodeRegistry> = Lazy::new(NodeRegistry::new);

/// Global registry of active GraphQL subscriptions.
pub struct GraphQLSubscriptionRegistry {
    subscriptions: HopscotchMap<SubscriptionHandle, Arc<GraphQLSubscriptionState>, RandomState>,
    next_handle: AtomicUsize,
}

impl GraphQLSubscriptionRegistry {
    fn new() -> Self {
        Self {
            subscriptions: HopscotchMap::with_hasher(RandomState::default()),
            next_handle: AtomicUsize::new(1),
        }
    }

    /// Insert a new GraphQL subscription state and return its handle.
    pub fn insert(&self, state: GraphQLSubscriptionState) -> SubscriptionHandle {
        let handle = self.next_handle.fetch_add(1, Ordering::Relaxed);
        self.subscriptions.insert(handle, Arc::new(state));
        handle
    }

    /// Get a GraphQL subscription state.
    pub fn get(&self, handle: SubscriptionHandle) -> Option<Arc<GraphQLSubscriptionState>> {
        self.subscriptions.get(&handle)
    }

    /// Remove and return a GraphQL subscription state.
    pub fn remove(&self, handle: SubscriptionHandle) -> Option<Arc<GraphQLSubscriptionState>> {
        self.subscriptions.remove(&handle)
    }

    /// Remove all GraphQL subscriptions for a given node handle.
    pub fn remove_for_node(&self, node_handle: NodeHandle) -> Vec<Arc<GraphQLSubscriptionState>> {
        let handles_to_remove: Vec<SubscriptionHandle> = self
            .subscriptions
            .iter()
            .filter(|(_, state)| state.node_handle == node_handle)
            .map(|(handle, _)| handle)
            .collect();

        handles_to_remove
            .into_iter()
            .filter_map(|handle| self.subscriptions.remove(&handle))
            .collect()
    }
}

/// The GraphQL subscriptions the C API has open, by handle.
pub static GRAPHQL_SUBSCRIPTIONS: Lazy<GraphQLSubscriptionRegistry> =
    Lazy::new(GraphQLSubscriptionRegistry::new);

#[cfg(test)]
mod tests {
    use super::NODES;
    use crate::test_support::{close, node};

    #[test]
    fn a_node_handle_is_registered_until_its_node_closes() {
        let first = node();
        let second = node();
        assert_ne!(first, 0);
        assert_ne!(first, second, "every node has its own handle");
        assert!(NODES.contains(first));
        assert_eq!(NODES.get(first, |_| ()), Some(()));

        close(first);
        assert!(!NODES.contains(first));
        assert!(NODES.get(first, |_| ()).is_none());
        assert!(NODES.remove(first).is_none(), "a handle is removed once");
        assert!(NODES.contains(second), "closing one node leaves the other");
        close(second);
    }
}
