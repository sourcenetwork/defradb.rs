//! The process-wide collection cache behind a schema epoch.
//!
//! Wraps the RCU'd [`CollectionMap`] so every successful swap bumps a
//! monotonic epoch. An unchanged epoch proves the committed collection set is
//! unchanged, which is what the introspection schema cache keys on. The
//! wrapper owns the bump rather than each of the many swap sites, so a swap
//! that skips it cannot be written.

use crate::collection::CollectionMap;
use kovan::Atom;

#[derive(Clone)]
struct VersionedMap {
    map: CollectionMap,
    epoch: u64,
}

/// The process-wide [`CollectionMap`] with an epoch that advances on every swap.
pub(crate) struct EpochedCollections {
    map: Atom<VersionedMap>,
}

impl EpochedCollections {
    pub(crate) fn new(map: CollectionMap) -> Self {
        Self {
            map: Atom::new(VersionedMap { map, epoch: 0 }),
        }
    }

    /// Read the map without cloning it.
    pub(crate) fn peek<R>(&self, f: impl FnOnce(&CollectionMap) -> R) -> R {
        self.map.peek(|versioned| f(&versioned.map))
    }

    /// Clone the current map out.
    pub(crate) fn load_clone(&self) -> CollectionMap {
        self.map.load_clone().map
    }

    /// Swap the map and its epoch in one RCU publication.
    pub(crate) fn rcu<F>(&self, mut f: F)
    where
        F: FnMut(&CollectionMap) -> CollectionMap,
    {
        self.map.rcu(|old| VersionedMap {
            map: f(&old.map),
            epoch: old.epoch.wrapping_add(1),
        });
    }

    /// Publish a reload only if no cache update has landed since it began.
    pub(crate) fn rcu_if_epoch<F>(&self, expected: u64, mut f: F) -> bool
    where
        F: FnMut(&CollectionMap) -> CollectionMap,
    {
        let mut applied = false;
        self.map.rcu(|old| {
            if old.epoch != expected {
                applied = false;
                old.clone()
            } else {
                applied = true;
                VersionedMap {
                    map: f(&old.map),
                    epoch: old.epoch.wrapping_add(1),
                }
            }
        });
        applied
    }

    /// The current schema epoch.
    pub(crate) fn epoch(&self) -> u64 {
        self.map.peek(|versioned| versioned.epoch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reload_cannot_overwrite_a_newer_publication() {
        let collections = EpochedCollections::new(CollectionMap::default());
        let snapshot_epoch = collections.epoch();

        collections.rcu(|map| map.clone());

        assert!(!collections.rcu_if_epoch(snapshot_epoch, |map| map.clone()));
        assert_eq!(collections.epoch(), snapshot_epoch + 1);
    }

    #[test]
    fn reload_publishes_when_no_publication_intervened() {
        let collections = EpochedCollections::new(CollectionMap::default());
        let snapshot_epoch = collections.epoch();

        assert!(collections.rcu_if_epoch(snapshot_epoch, |map| map.clone()));
        assert_eq!(collections.epoch(), snapshot_epoch + 1);
    }
}
