//! Transaction-scoped collection cache.
//!
//! This module provides a per-transaction cache for collection metadata,
//! matching the Go DefraDB pattern. Each transaction gets its own cache
//! that is populated lazily from the SystemStore.

use std::sync::Arc;

use imbl::shared_ptr::DefaultSharedPtr;
use imbl::GenericHashMap;
use rapidhash::fast::RandomState;

use crate::collection::Collection;

type ByName = GenericHashMap<Arc<str>, Arc<Collection>, RandomState, DefaultSharedPtr>;

/// A transaction-scoped cache for collection metadata.
///
/// This cache lives within a transaction and provides:
/// - O(1) average-case lookups by collection name
/// - Lazy loading from SystemStore on cache miss
/// - Isolation between transactions (each txn has its own cache)
///
/// The cache is populated lazily - individual collections are loaded on
/// first access, and the full collection list is loaded when needed.
///
/// A transaction publishes its cache as an immutable snapshot and replaces
/// it on every load, so the map is persistent: a clone shares every entry,
/// and an add copies only the nodes on the path to its own entry, never the
/// collections already loaded.
#[derive(Debug, Clone)]
pub struct CollectionCache {
    /// Collections indexed by name
    by_name: ByName,

    /// Whether the full collection set has been loaded from store
    is_fully_populated: bool,
}

impl CollectionCache {
    /// Create a new empty collection cache.
    pub fn new() -> Self {
        Self {
            by_name: ByName::default(),
            is_fully_populated: false,
        }
    }

    /// Get a collection by name.
    ///
    /// Returns `None` if the collection is not in the cache.
    /// Note: A `None` result does not mean the collection doesn't exist -
    /// it may not have been loaded yet. Use the transaction's lazy loading
    /// methods to check the store.
    pub fn get(&self, name: &str) -> Option<&Collection> {
        self.by_name.get(name).map(Arc::as_ref)
    }

    /// Check if a collection exists in the cache.
    ///
    /// Note: `false` does not mean the collection doesn't exist - it may
    /// not have been loaded yet.
    pub fn contains(&self, name: &str) -> bool {
        self.by_name.contains_key(name)
    }

    /// Add a collection to the cache, keyed by its name.
    ///
    /// The key is derived from `collection.name()` to prevent key-name mismatches.
    pub fn add(&mut self, collection: Collection) {
        self.by_name
            .insert(Arc::from(collection.name()), Arc::new(collection));
    }

    /// Remove a collection from the cache.
    ///
    /// Returns the removed collection if it existed.
    /// Note: This resets the `is_fully_populated` flag since the cache
    /// no longer contains all collections.
    pub fn remove(&mut self, name: &str) -> Option<Collection> {
        let removed = self.by_name.remove(name)?;
        self.is_fully_populated = false;
        Some(Arc::unwrap_or_clone(removed))
    }

    /// Get all collection names in the cache.
    pub fn names(&self) -> Vec<String> {
        self.by_name.keys().map(|name| name.to_string()).collect()
    }

    /// Get the number of collections in the cache.
    pub fn len(&self) -> usize {
        self.by_name.len()
    }

    /// Check if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }

    /// Check if the cache has been fully populated from the store.
    pub fn is_fully_populated(&self) -> bool {
        self.is_fully_populated
    }

    /// Populate the cache with a full set of collections.
    ///
    /// This replaces any existing cache contents, marks the cache as fully
    /// populated, and derives keys from each collection's name.
    pub fn populate(&mut self, collections: impl IntoIterator<Item = Collection>) {
        self.by_name = collections
            .into_iter()
            .map(|c| (Arc::from(c.name()), Arc::new(c)))
            .collect();
        self.is_fully_populated = true;
    }
}

impl Default for CollectionCache {
    fn default() -> Self {
        Self::new()
    }
}
