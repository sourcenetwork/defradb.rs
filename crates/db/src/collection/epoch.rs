//! The process-wide collection cache behind a schema epoch.
//!
//! Wraps the RCU'd [`CollectionMap`] so every successful swap bumps a
//! monotonic epoch. An unchanged epoch proves the committed collection set is
//! unchanged, which is what the introspection schema cache keys on. The
//! wrapper owns the bump rather than each of the many swap sites, so a swap
//! that skips it cannot be written.

use kovan::Atom;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::collection::CollectionMap;

/// The process-wide [`CollectionMap`] with an epoch that advances on every swap.
pub(crate) struct EpochedCollections {
    map: Atom<CollectionMap>,
    epoch: AtomicU64,
}

impl EpochedCollections {
    pub(crate) fn new(map: CollectionMap) -> Self {
        Self {
            map: Atom::new(map),
            epoch: AtomicU64::new(0),
        }
    }

    /// Read the map without cloning it.
    pub(crate) fn peek<R>(&self, f: impl FnOnce(&CollectionMap) -> R) -> R {
        self.map.peek(f)
    }

    /// Clone the current map out.
    pub(crate) fn load_clone(&self) -> CollectionMap {
        self.map.load_clone()
    }

    /// Swap the map via read-copy-update, then advance the epoch.
    ///
    /// Bump after the swap so a reader can only pair a newer map with an
    /// older epoch (a harmless rebuild), never the reverse.
    pub(crate) fn rcu<F>(&self, f: F)
    where
        F: FnMut(&CollectionMap) -> CollectionMap,
    {
        self.map.rcu(f);
        self.epoch.fetch_add(1, Ordering::AcqRel);
    }

    /// The current schema epoch.
    pub(crate) fn epoch(&self) -> u64 {
        self.epoch.load(Ordering::Acquire)
    }
}
