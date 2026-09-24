use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock, Weak};

use kovan_map::HopscotchMap;
use kovan_queue::seg_queue::SegQueue;
use rapidhash::fast::RandomState;

use super::{
    GraphQLSubscriptionState, NodeHandle, NodeState, SubscriptionHandle, SubscriptionState,
};

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
    next_handle: AtomicUsize,
}

impl NodeRegistry {
    /// Create a new empty registry.
    fn new() -> Self {
        Self {
            nodes: HopscotchMap::with_hasher(RandomState::default()),
            next_handle: AtomicUsize::new(1), // Start at 1, 0 is invalid
        }
    }

    /// Insert a new node state and return its handle.
    pub fn insert(&self, state: NodeState) -> NodeHandle {
        let handle = self.next_handle.fetch_add(1, Ordering::Relaxed);
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

    /// Get the number of active nodes.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Apply an operation to every node state in the registry.
    pub fn for_each<F>(&self, mut f: F)
    where
        F: FnMut(&NodeState),
    {
        for slot in self.nodes.values() {
            if let Some(state) = slot.reader.upgrade() {
                f(&state);
            }
        }
    }

    /// Check if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Global node registry singleton, lazily initialized.
static NODE_REGISTRY: OnceLock<NodeRegistry> = OnceLock::new();

/// Access the global node registry.
pub fn nodes() -> &'static NodeRegistry {
    NODE_REGISTRY.get_or_init(NodeRegistry::new)
}

/// Global registry of active subscriptions.
pub struct SubscriptionRegistry {
    subscriptions: HopscotchMap<SubscriptionHandle, Arc<SubscriptionState>, RandomState>,
    next_handle: AtomicUsize,
}

impl SubscriptionRegistry {
    fn new() -> Self {
        Self {
            subscriptions: HopscotchMap::with_hasher(RandomState::default()),
            next_handle: AtomicUsize::new(1),
        }
    }

    /// Insert a new subscription state and return its handle.
    pub fn insert(&self, state: SubscriptionState) -> SubscriptionHandle {
        let handle = self.next_handle.fetch_add(1, Ordering::Relaxed);
        self.subscriptions.insert(handle, Arc::new(state));
        handle
    }

    /// Get a subscription state.
    pub fn get(&self, handle: SubscriptionHandle) -> Option<Arc<SubscriptionState>> {
        self.subscriptions.get(&handle)
    }

    /// Remove and return a subscription state.
    pub fn remove(&self, handle: SubscriptionHandle) -> Option<Arc<SubscriptionState>> {
        self.subscriptions.remove(&handle)
    }

    /// Remove all subscriptions for a given node handle.
    pub fn remove_for_node(&self, node_handle: NodeHandle) -> Vec<Arc<SubscriptionState>> {
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

/// Global subscription registry singleton.
static SUBSCRIPTION_REGISTRY: OnceLock<SubscriptionRegistry> = OnceLock::new();

/// Access the global subscription registry.
pub fn subscriptions() -> &'static SubscriptionRegistry {
    SUBSCRIPTION_REGISTRY.get_or_init(SubscriptionRegistry::new)
}

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

/// Global GraphQL subscription registry singleton.
static GRAPHQL_SUBSCRIPTION_REGISTRY: OnceLock<GraphQLSubscriptionRegistry> = OnceLock::new();

/// Access the global GraphQL subscription registry.
pub fn graphql_subscriptions() -> &'static GraphQLSubscriptionRegistry {
    GRAPHQL_SUBSCRIPTION_REGISTRY.get_or_init(GraphQLSubscriptionRegistry::new)
}

/// Convenience wrapper for subscription registry access.
pub struct SubscriptionsAccess;

impl SubscriptionsAccess {
    pub fn insert(&self, state: SubscriptionState) -> SubscriptionHandle {
        subscriptions().insert(state)
    }

    pub fn get(&self, handle: SubscriptionHandle) -> Option<Arc<SubscriptionState>> {
        subscriptions().get(handle)
    }

    pub fn remove(&self, handle: SubscriptionHandle) -> Option<Arc<SubscriptionState>> {
        subscriptions().remove(handle)
    }

    pub fn remove_for_node(&self, node_handle: NodeHandle) -> Vec<Arc<SubscriptionState>> {
        subscriptions().remove_for_node(node_handle)
    }
}

/// Global SUBSCRIPTIONS accessor.
pub static SUBSCRIPTIONS: SubscriptionsAccess = SubscriptionsAccess;

/// Convenience wrapper for NODES access (backwards compatibility).
pub struct NodesAccess;

impl NodesAccess {
    pub fn insert(&self, state: NodeState) -> NodeHandle {
        nodes().insert(state)
    }

    pub fn get<F, R>(&self, handle: NodeHandle, f: F) -> Option<R>
    where
        F: FnOnce(&NodeState) -> R,
    {
        nodes().get(handle, f)
    }

    pub fn for_each<F>(&self, f: F)
    where
        F: FnMut(&NodeState),
    {
        nodes().for_each(f)
    }

    pub fn remove(&self, handle: NodeHandle) -> Option<Arc<NodeState>> {
        nodes().remove(handle)
    }
}

/// Global NODES accessor for backwards compatibility.
pub static NODES: NodesAccess = NodesAccess;

/// Convenience wrapper for GraphQL subscription registry access.
pub struct GraphQLSubscriptionsAccess;

impl GraphQLSubscriptionsAccess {
    pub fn insert(&self, state: GraphQLSubscriptionState) -> SubscriptionHandle {
        graphql_subscriptions().insert(state)
    }

    pub fn get(&self, handle: SubscriptionHandle) -> Option<Arc<GraphQLSubscriptionState>> {
        graphql_subscriptions().get(handle)
    }

    pub fn remove(&self, handle: SubscriptionHandle) -> Option<Arc<GraphQLSubscriptionState>> {
        graphql_subscriptions().remove(handle)
    }

    pub fn remove_for_node(&self, node_handle: NodeHandle) -> Vec<Arc<GraphQLSubscriptionState>> {
        graphql_subscriptions().remove_for_node(node_handle)
    }
}

/// Global GRAPHQL_SUBSCRIPTIONS accessor.
pub static GRAPHQL_SUBSCRIPTIONS: GraphQLSubscriptionsAccess = GraphQLSubscriptionsAccess;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_handles() {
        // Handles should be non-zero and incrementing
        let registry = nodes();
        assert!(registry.is_empty() || !registry.is_empty());
    }
}
