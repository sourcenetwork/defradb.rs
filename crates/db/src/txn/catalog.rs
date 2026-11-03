//! A transaction's own view of the collections: the ones it loaded or
//! changed, the ones it created, and the branchable-collection ACP
//! registrations its creates owe before the commit.
//!
//! Every handle a transaction is lent to (a request's fetcher, mutator and
//! collection provider) reads and adds to it through `&self`, with no lock:
//! the collections are one immutable snapshot swapped whole, which readers
//! never wait on, and the owed registrations a queue. Only the transaction's
//! owner writes, one request at a time, so a swap never races another. A
//! statement savepoint is a snapshot of both, taken and put back while the
//! transaction is held alone.

use rapidhash::{HashSetExt, RapidHashSet};
use regolith::sync::{Atom, SegQueue};

use super::OwedAcpRegistration;
use crate::collection::cache::CollectionCache;
use crate::collection::Collection;

#[derive(Clone, Default)]
struct Collections {
    cache: CollectionCache,
    created: RapidHashSet<String>,
}

pub(super) struct Catalog {
    collections: Atom<Collections>,
    owed: SegQueue<OwedAcpRegistration>,
}

/// What a statement savepoint puts back.
pub(super) struct Snapshot {
    collections: Collections,
    owed: Vec<OwedAcpRegistration>,
}

impl Catalog {
    pub(super) fn new() -> Self {
        Self {
            collections: Atom::new(Collections {
                cache: CollectionCache::new(),
                created: RapidHashSet::new(),
            }),
            owed: SegQueue::new(),
        }
    }

    fn update(&self, mut change: impl FnMut(&mut Collections)) {
        self.collections.rcu(|current| {
            let mut next = current.clone();
            change(&mut next);
            next
        });
    }

    pub(super) fn get(&self, name: &str) -> Option<Collection> {
        self.collections
            .peek(|collections| collections.cache.get(name).cloned())
    }

    pub(super) fn add(&self, collection: Collection) {
        self.update(|collections| collections.cache.add(collection.clone()));
    }

    pub(super) fn remove(&self, name: &str) {
        self.update(|collections| {
            collections.cache.remove(name);
        });
    }

    /// Replaces every cached collection with `all`, the full set.
    pub(super) fn populate(&self, all: Vec<Collection>) {
        self.update(|collections| collections.cache.populate(all.iter().cloned()));
    }

    pub(super) fn is_fully_populated(&self) -> bool {
        self.collections
            .peek(|collections| collections.cache.is_fully_populated())
    }

    pub(super) fn names(&self) -> Vec<String> {
        self.collections
            .peek(|collections| collections.cache.names())
    }

    pub(super) fn mark_created(&self, collection_id: String) {
        self.update(|collections| {
            collections.created.insert(collection_id.clone());
        });
    }

    pub(super) fn was_created(&self, collection_id: &str) -> bool {
        self.collections
            .peek(|collections| collections.created.contains(collection_id))
    }

    pub(super) fn owe(&self, registration: OwedAcpRegistration) {
        self.owed.push(registration);
    }

    pub(super) fn owes(&self) -> bool {
        !self.owed.is_empty()
    }

    /// Takes every owed registration, in the order they were staged.
    pub(super) fn take_owed(&self) -> Vec<OwedAcpRegistration> {
        std::iter::from_fn(|| self.owed.pop()).collect()
    }

    /// What a savepoint set now puts back. Called with the transaction held
    /// alone.
    pub(super) fn snapshot(&self) -> Snapshot {
        let owed = self.take_owed();
        owed.iter()
            .cloned()
            .for_each(|registration| self.owed.push(registration));
        Snapshot {
            collections: self.collections.load_clone(),
            owed,
        }
    }

    /// Puts `snapshot` back. Called with the transaction held alone.
    pub(super) fn restore(&self, snapshot: Snapshot) {
        self.collections.store(snapshot.collections);
        while self.owed.pop().is_some() {}
        snapshot
            .owed
            .into_iter()
            .for_each(|registration| self.owed.push(registration));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use schema::CollectionVersion;

    fn collection(name: &str) -> Collection {
        Collection::new(
            CollectionVersion::new(name, name, name, vec![]),
            crdt::Crdts::default(),
        )
    }

    #[test]
    fn handles_sharing_the_catalog_see_each_others_changes() {
        let catalog = Catalog::new();
        let (first, second) = (&catalog, &catalog);
        first.add(collection("users"));
        assert_eq!(
            second.get("users").map(|c| c.name().to_owned()),
            Some("users".into())
        );
        second.mark_created("users".into());
        assert!(first.was_created("users"));
        first.remove("users");
        assert!(second.get("users").is_none());
        assert!(
            second.was_created("users"),
            "creation outlives the cache entry"
        );
    }

    #[test]
    fn a_full_load_replaces_the_cache_and_a_removal_ends_its_completeness() {
        let catalog = Catalog::new();
        catalog.add(collection("stale"));
        catalog.populate(vec![collection("a"), collection("b")]);
        assert!(catalog.is_fully_populated());
        let mut names = catalog.names();
        names.sort();
        assert_eq!(names, ["a", "b"]);
        catalog.remove("a");
        assert!(!catalog.is_fully_populated());
    }

    #[test]
    fn a_savepoint_puts_back_the_collections_and_the_owed_registrations() {
        let catalog = Catalog::new();
        catalog.add(collection("kept"));
        let snapshot = catalog.snapshot();
        catalog.add(collection("undone"));
        catalog.mark_created("undone".into());
        catalog.remove("kept");
        catalog.restore(snapshot);
        assert!(catalog.get("kept").is_some());
        assert!(catalog.get("undone").is_none());
        assert!(!catalog.was_created("undone"));
        assert!(!catalog.owes());
    }
}
