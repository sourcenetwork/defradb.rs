//! Transaction-scoped collection cache.
//!
//! This module provides a per-transaction cache for collection metadata,
//! matching the Go DefraDB pattern. Each transaction gets its own cache
//! that is populated lazily from the SystemStore.

use std::sync::Arc;

use rapidhash::{HashMapExt, RapidHashMap};

use crate::collection::Collection;

/// One cached collection under its name.
type Entry = (Arc<str>, Arc<Collection>);

/// How many loads the recent layer holds before they are folded into the
/// shared map.
const RECENT: usize = 16;

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
/// it with a changed clone on every load, so a clone is two references and a
/// load must not copy what the transaction already holds. The entries sit in
/// two layers: `folded`, a map every snapshot shares, and `recent`, the last
/// few loads in one small slice that shadows it. A load copies only
/// `recent`; once it holds `RECENT` entries the next load folds them into
/// a new `folded`. A load therefore copies at most `RECENT` entries, plus
/// the whole map once every `RECENT` loads, never a name or a collection
/// (`proofs/lean/CollectionCacheLayers.lean`).
#[derive(Debug, Clone, Default)]
pub struct CollectionCache {
    /// The folded entries, shared by every snapshot until the next fold.
    folded: Option<Arc<RapidHashMap<Arc<str>, Arc<Collection>>>>,
    /// The entries loaded since the last fold, at most [`RECENT`], each name
    /// once. A name here answers ahead of `folded`.
    recent: Option<Arc<[Entry]>>,
    /// Whether the full collection set has been loaded from store
    is_fully_populated: bool,
}

impl CollectionCache {
    /// Create a new empty collection cache.
    pub fn new() -> Self {
        Self::default()
    }

    fn recent(&self) -> &[Entry] {
        self.recent.as_deref().unwrap_or_default()
    }

    fn recent_entry(&self, name: &str) -> Option<&Entry> {
        self.recent().iter().find(|(entry, _)| &**entry == name)
    }

    fn folded_contains(&self, name: &str) -> bool {
        self.folded
            .as_ref()
            .is_some_and(|folded| folded.contains_key(name))
    }

    /// Get a collection by name.
    ///
    /// Returns `None` if the collection is not in the cache.
    /// Note: A `None` result does not mean the collection doesn't exist -
    /// it may not have been loaded yet. Use the transaction's lazy loading
    /// methods to check the store.
    pub fn get(&self, name: &str) -> Option<&Collection> {
        match self.recent_entry(name) {
            Some((_, collection)) => Some(collection),
            None => self.folded.as_ref()?.get(name).map(Arc::as_ref),
        }
    }

    /// Check if a collection exists in the cache.
    ///
    /// Note: `false` does not mean the collection doesn't exist - it may
    /// not have been loaded yet.
    pub fn contains(&self, name: &str) -> bool {
        self.recent_entry(name).is_some() || self.folded_contains(name)
    }

    /// Add a collection to the cache, keyed by its name.
    ///
    /// The key is derived from `collection.name()` to prevent key-name mismatches.
    pub fn add(&mut self, collection: Collection) {
        let entry: Entry = (Arc::from(collection.name()), Arc::new(collection));
        let recent = self.recent();
        if recent.iter().any(|(name, _)| *name == entry.0) {
            self.recent = Some(
                recent
                    .iter()
                    .map(|current| {
                        if current.0 == entry.0 {
                            entry.clone()
                        } else {
                            current.clone()
                        }
                    })
                    .collect(),
            );
        } else if recent.len() < RECENT {
            self.recent = Some(
                recent
                    .iter()
                    .cloned()
                    .chain(std::iter::once(entry))
                    .collect(),
            );
        } else {
            self.fold(entry);
        }
    }

    /// Folds `recent`, and `entry` after it, into a new `folded`.
    fn fold(&mut self, entry: Entry) {
        let recent = self.recent.take();
        let recent = recent.as_deref().unwrap_or_default();
        let folded_len = self.folded.as_ref().map_or(0, |folded| folded.len());
        let mut folded = RapidHashMap::with_capacity(folded_len + recent.len() + 1);
        if let Some(current) = &self.folded {
            folded.extend(
                current
                    .iter()
                    .map(|(name, collection)| (name.clone(), collection.clone())),
            );
        }
        folded.extend(recent.iter().cloned());
        folded.insert(entry.0, entry.1);
        self.folded = Some(Arc::new(folded));
    }

    /// Remove a collection from the cache.
    ///
    /// Returns the removed collection if it existed.
    /// Note: This resets the `is_fully_populated` flag since the cache
    /// no longer contains all collections.
    pub fn remove(&mut self, name: &str) -> Option<Collection> {
        let in_recent = self
            .recent_entry(name)
            .map(|(_, collection)| collection.clone());
        let in_folded = self
            .folded
            .as_ref()
            .and_then(|folded| folded.get(name).cloned());
        if in_recent.is_some() {
            let kept: Vec<Entry> = self
                .recent()
                .iter()
                .filter(|(entry, _)| &**entry != name)
                .cloned()
                .collect();
            self.recent = (!kept.is_empty()).then(|| kept.into());
        }
        if in_folded.is_some() {
            if let Some(folded) = self.folded.as_mut() {
                Arc::make_mut(folded).remove(name);
            }
        }
        let removed = in_recent.or(in_folded)?;
        self.is_fully_populated = false;
        Some(Arc::unwrap_or_clone(removed))
    }

    /// Get all collection names in the cache.
    pub fn names(&self) -> Vec<String> {
        let shadowed = |name: &Arc<str>| self.recent_entry(name).is_some();
        self.recent()
            .iter()
            .map(|(name, _)| name.to_string())
            .chain(self.folded.iter().flat_map(|folded| {
                folded
                    .keys()
                    .filter(|name| !shadowed(name))
                    .map(|name| name.to_string())
            }))
            .collect()
    }

    /// Get the number of collections in the cache.
    pub fn len(&self) -> usize {
        let fresh = self
            .recent()
            .iter()
            .filter(|(name, _)| !self.folded_contains(name))
            .count();
        self.folded.as_ref().map_or(0, |folded| folded.len()) + fresh
    }

    /// Check if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
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
        self.folded = Some(Arc::new(
            collections
                .into_iter()
                .map(|c| (Arc::from(c.name()), Arc::new(c)))
                .collect(),
        ));
        self.recent = None;
        self.is_fully_populated = true;
    }
}
