//! Replicator registry for tracking authorized peers per collection.
//!
//! Uses string-based peer IDs so the registry works with both libp2p and iroh transports.

use std::collections::BTreeMap;

use kovan::Atom;

use crate::replicator::ReplicatorInfo;

type Replicators = BTreeMap<String, ReplicatorInfo>;

/// Tracks which peers are authorized replicators for which collections.
///
/// This is the fast-path access check used by Go DefraDB. Replicators
/// automatically have access to all blocks in their subscribed collections.
///
/// Peer IDs are stored as strings to support both libp2p PeerIds and
/// iroh EndpointIds without coupling to either transport.
///
/// Reads take a snapshot; writes rebuild the map under RCU, which keeps every
/// per-peer read-modify-write atomic. Writes are admin operations, so the copy
/// per write is not on any hot path.
#[derive(Debug, Default)]
pub struct ReplicatorRegistry {
    /// Map of peer ID string -> persisted replicator metadata.
    replicators: Atom<Replicators>,
}

/// Store a peer's record, keeping it when its collection list is empty.
///
/// Go keeps a replicator added before any collection existed: it is stored
/// with no collection IDs, listed, and restored on start (#2702). It is a
/// replicator of nothing, so it grants no access and receives no pushes.
/// Only removing a peer's last collection deletes its entry, as in Go's
/// `deleteReplicator` (internal/db/p2p/replicator.go).
fn store_info(replicators: &mut Replicators, info: ReplicatorInfo) {
    replicators.insert(info.peer_id_str().to_string(), info);
}

impl ReplicatorRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a peer as a replicator for a collection.
    pub fn add_replicator(&self, collection_id: &str, peer_id: &str) {
        self.replicators.rcu(|current| {
            let mut replicators = current.clone();
            let info = replicators.entry(peer_id.to_string()).or_insert_with(|| {
                ReplicatorInfo::from_raw(peer_id.to_string(), Vec::new(), Vec::new())
            });
            if !info.collections.iter().any(|id| id == collection_id) {
                info.collections.push(collection_id.to_string());
            }
            replicators
        });
    }

    /// Replace the full set of collections replicated by a peer. An empty
    /// list keeps the peer's entry with no collections; it does not remove it.
    pub fn set_peer_collections(&self, peer_id: &str, collections: &[String]) {
        if peer_id.is_empty() {
            return;
        }
        self.replicators.rcu(|current| {
            let mut info =
                ReplicatorInfo::from_raw(peer_id.to_string(), collections.to_vec(), Vec::new());
            if let Some(existing) = current.get(peer_id) {
                info.addresses = existing.addresses.clone();
                info.status = existing.status;
                info.last_status_change = existing.last_status_change;
            }
            let mut replicators = current.clone();
            store_info(&mut replicators, info);
            replicators
        });
    }

    /// Replace a peer's metadata while preserving its existing status and
    /// status-change time. An empty collection list does not remove the peer.
    pub fn set_replicator_info(&self, info: ReplicatorInfo) {
        if info.peer_id_str().is_empty() {
            return;
        }
        self.replicators.rcu(|current| {
            let mut info = info.clone();
            if let Some(existing) = current.get(info.peer_id_str()) {
                info.status = existing.status;
                info.last_status_change = existing.last_status_change;
            }
            let mut replicators = current.clone();
            store_info(&mut replicators, info);
            replicators
        });
    }

    /// Remove a peer as a replicator for a collection.
    pub fn remove_replicator(&self, collection_id: &str, peer_id: &str) {
        self.remove_peer_collections(peer_id, std::slice::from_ref(&collection_id.to_string()));
    }

    /// Remove a peer from all collections.
    pub fn remove_peer(&self, peer_id: &str) {
        self.replicators.rcu(|current| {
            let mut replicators = current.clone();
            replicators.remove(peer_id);
            replicators
        });
    }

    /// Remove a peer from specific collections. Returns true if the peer no
    /// longer replicates any collections afterwards.
    pub fn remove_peer_collections(&self, peer_id: &str, collections: &[String]) -> bool {
        let mut fully_removed = true;
        self.replicators.rcu(|current| {
            let mut replicators = current.clone();
            fully_removed = match replicators.get_mut(peer_id) {
                None => true,
                Some(info) => {
                    for collection_id in collections {
                        info.collections.retain(|id| id != collection_id);
                        info.filters
                            .retain(|collection, _| collection != collection_id);
                    }
                    info.collections.is_empty()
                }
            };
            if fully_removed {
                replicators.remove(peer_id);
            }
            replicators
        });
        fully_removed
    }

    /// Check if a peer is a replicator for a collection.
    pub fn is_replicator(&self, collection_id: &str, peer_id: &str) -> bool {
        self.replicators
            .load()
            .get(peer_id)
            .map(|info| info.collections.iter().any(|id| id == collection_id))
            .unwrap_or(false)
    }

    /// Check if a peer has a filter for a collection it replicates.
    pub fn is_filtered_replicator(&self, collection_id: &str, peer_id: &str) -> bool {
        self.replicators
            .load()
            .get(peer_id)
            .map(|info| info.is_filtered_for_collection(collection_id))
            .unwrap_or(false)
    }

    /// Check if a peer is a replicator for any collection. A peer kept with
    /// no collections is not.
    pub fn is_any_replicator(&self, peer_id: &str) -> bool {
        self.replicators
            .load()
            .get(peer_id)
            .is_some_and(|info| !info.collections.is_empty())
    }

    /// Get all replicator peer ID strings for a collection.
    pub fn get_replicators(&self, collection_id: &str) -> Vec<String> {
        self.replicators
            .load()
            .iter()
            .filter(|(_, info)| info.collections.iter().any(|id| id == collection_id))
            .map(|(peer_id, _)| peer_id.clone())
            .collect()
    }

    /// Get all collections a peer is replicating.
    pub fn get_collections(&self, peer_id: &str) -> Vec<String> {
        self.replicators
            .load()
            .get(peer_id)
            .map(|info| info.collections.clone())
            .unwrap_or_default()
    }

    /// Get all registered replicators as ReplicatorInfo.
    pub fn list_replicator_info(&self) -> Vec<ReplicatorInfo> {
        self.replicators.load().values().cloned().collect()
    }

    /// Load replicators from ReplicatorInfo records.
    ///
    /// Existing state is cleared before loading. Accepts any peer ID format
    /// (libp2p or iroh) since we store as strings. A record with no
    /// collections is loaded and listed, as Go restores it.
    pub fn load_from_infos(&self, infos: &[ReplicatorInfo]) -> (usize, usize) {
        let mut replicators = Replicators::new();
        let mut loaded = 0;
        let mut skipped = 0;

        for info in infos {
            let peer_id_str = info.peer_id_str();
            if peer_id_str.is_empty() {
                tracing::warn!(
                    collections = ?info.collections,
                    "Skipping replicator with empty peer ID during load"
                );
                skipped += 1;
                continue;
            }

            replicators.insert(peer_id_str.to_string(), info.clone());
            loaded += 1;
        }

        self.replicators.store(replicators);
        (loaded, skipped)
    }

    /// Get replicator info for a specific peer.
    pub fn get_replicator_info(&self, peer_id: &str) -> Option<ReplicatorInfo> {
        self.replicators.load().get(peer_id).cloned()
    }

    /// Get all unique peer ID strings that are replicators.
    pub fn get_all_peer_ids(&self) -> Vec<String> {
        self.replicators.load().keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;
    use crate::replicator::{ReplicationFilter, ReplicationFilters};

    fn random_peer_id() -> String {
        static NEXT_PEER_ID: AtomicU64 = AtomicU64::new(0);
        format!("test-peer-{}", NEXT_PEER_ID.fetch_add(1, Ordering::Relaxed))
    }

    #[test]
    fn replacing_replicator_keeps_status_timestamp() {
        let registry = ReplicatorRegistry::new();
        let peer = random_peer_id();
        let mut first = ReplicatorInfo::from_raw(peer.clone(), vec!["users".into()], vec![]);
        first.status = crate::replicator::ReplicatorStatus::Inactive;
        first.last_status_change = chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        registry.set_replicator_info(first.clone());

        let mut update = ReplicatorInfo::from_raw(peer.clone(), vec!["posts".into()], vec![]);
        update.last_status_change = chrono::DateTime::from_timestamp(1_800_000_000, 0).unwrap();
        registry.set_replicator_info(update);

        let stored = registry.get_replicator_info(&peer).unwrap();
        assert_eq!(stored.collections, ["posts"]);
        assert_eq!(stored.last_status_change, first.last_status_change);
        assert_eq!(stored.status, first.status);
    }

    #[test]
    fn test_replicator_registry_add_remove() {
        let registry = ReplicatorRegistry::new();
        let peer = random_peer_id();

        registry.add_replicator("users", &peer);
        assert!(registry.is_replicator("users", &peer));
        assert!(!registry.is_replicator("posts", &peer));

        registry.remove_replicator("users", &peer);
        assert!(!registry.is_replicator("users", &peer));
    }

    #[test]
    fn test_replicator_registry_multiple_collections() {
        let registry = ReplicatorRegistry::new();
        let peer = random_peer_id();

        registry.add_replicator("users", &peer);
        registry.add_replicator("posts", &peer);

        assert!(registry.is_replicator("users", &peer));
        assert!(registry.is_replicator("posts", &peer));
        assert!(registry.is_any_replicator(&peer));

        let collections = registry.get_collections(&peer);
        assert_eq!(collections.len(), 2);
    }

    #[test]
    fn test_replicator_registry_remove_peer() {
        let registry = ReplicatorRegistry::new();
        let peer = random_peer_id();

        registry.add_replicator("users", &peer);
        registry.add_replicator("posts", &peer);

        registry.remove_peer(&peer);

        assert!(!registry.is_replicator("users", &peer));
        assert!(!registry.is_replicator("posts", &peer));
        assert!(!registry.is_any_replicator(&peer));
    }

    #[test]
    fn test_replicator_registry_add_same_peer_twice() {
        let registry = ReplicatorRegistry::new();
        let peer = random_peer_id();

        registry.add_replicator("users", &peer);
        registry.add_replicator("users", &peer);

        let replicators = registry.get_replicators("users");
        assert_eq!(replicators.len(), 1);
        assert!(replicators.contains(&peer));
    }

    #[test]
    fn test_replicator_registry_remove_nonexistent() {
        let registry = ReplicatorRegistry::new();
        let peer = random_peer_id();

        registry.remove_replicator("nonexistent", &peer);

        let other_peer = random_peer_id();
        registry.add_replicator("users", &other_peer);
        registry.remove_replicator("users", &peer);

        assert!(registry.is_replicator("users", &other_peer));
    }

    #[test]
    fn test_replicator_registry_concurrent_modifications() {
        use std::thread;

        let registry = std::sync::Arc::new(ReplicatorRegistry::new());
        let mut handles = vec![];

        for i in 0..10 {
            let registry_clone = std::sync::Arc::clone(&registry);
            let handle = thread::spawn(move || {
                let peer = random_peer_id();
                let collection = format!("collection_{}", i % 3);

                registry_clone.add_replicator(&collection, &peer);
                assert!(registry_clone.is_any_replicator(&peer));

                if i % 2 == 0 {
                    registry_clone.remove_replicator(&collection, &peer);
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().expect("Thread panicked");
        }

        let _ = registry.get_replicators("collection_0");
        let _ = registry.get_replicators("collection_1");
        let _ = registry.get_replicators("collection_2");
    }

    #[test]
    fn test_replicator_registry_list_replicator_info() {
        let registry = ReplicatorRegistry::new();
        let peer1 = random_peer_id();
        let peer2 = random_peer_id();

        registry.add_replicator("users", &peer1);
        registry.add_replicator("posts", &peer1);
        registry.add_replicator("users", &peer2);

        let infos = registry.list_replicator_info();
        assert_eq!(infos.len(), 2);

        let peer1_info = infos.iter().find(|i| i.peer_id_str() == peer1).unwrap();
        assert_eq!(peer1_info.collections.len(), 2);
        assert!(peer1_info.collections.contains(&"users".to_string()));
        assert!(peer1_info.collections.contains(&"posts".to_string()));

        let peer2_info = infos.iter().find(|i| i.peer_id_str() == peer2).unwrap();
        assert_eq!(peer2_info.collections.len(), 1);
        assert!(peer2_info.collections.contains(&"users".to_string()));
    }

    #[test]
    fn test_replicator_registry_load_from_infos() {
        let registry = ReplicatorRegistry::new();
        let peer1 = random_peer_id();
        let peer2 = random_peer_id();

        let infos = vec![
            ReplicatorInfo::from_raw(
                peer1.clone(),
                vec!["users".to_string(), "posts".to_string()],
                vec![],
            ),
            ReplicatorInfo::from_raw(peer2.clone(), vec!["comments".to_string()], vec![]),
        ];

        let (loaded, skipped) = registry.load_from_infos(&infos);
        assert_eq!(loaded, 2);
        assert_eq!(skipped, 0);

        assert!(registry.is_replicator("users", &peer1));
        assert!(registry.is_replicator("posts", &peer1));
        assert!(!registry.is_replicator("comments", &peer1));

        assert!(registry.is_replicator("comments", &peer2));
        assert!(!registry.is_replicator("users", &peer2));
    }

    #[test]
    fn test_replicator_registry_load_clears_existing() {
        let registry = ReplicatorRegistry::new();
        let peer1 = random_peer_id();
        let peer2 = random_peer_id();

        registry.add_replicator("users", &peer1);
        assert!(registry.is_replicator("users", &peer1));

        let infos = vec![ReplicatorInfo::from_raw(
            peer2.clone(),
            vec!["comments".to_string()],
            vec![],
        )];
        let (loaded, skipped) = registry.load_from_infos(&infos);
        assert_eq!(loaded, 1);
        assert_eq!(skipped, 0);

        assert!(!registry.is_replicator("users", &peer1));
        assert!(!registry.is_any_replicator(&peer1));

        assert!(registry.is_replicator("comments", &peer2));
    }

    #[test]
    fn test_replicator_registry_get_replicator_info() {
        let registry = ReplicatorRegistry::new();
        let peer = random_peer_id();

        assert!(registry.get_replicator_info(&peer).is_none());

        registry.add_replicator("users", &peer);
        registry.add_replicator("posts", &peer);

        let info = registry.get_replicator_info(&peer).unwrap();
        assert_eq!(info.peer_id_str(), peer);
        assert_eq!(info.collections.len(), 2);
        assert!(info.collections.contains(&"users".to_string()));
        assert!(info.collections.contains(&"posts".to_string()));
    }

    #[test]
    fn test_replicator_registry_preserves_filters() {
        let registry = ReplicatorRegistry::new();
        let peer = random_peer_id();
        let mut filters = ReplicationFilters::new();
        filters.insert(
            "users".to_string(),
            ReplicationFilter::new("agent_did", serde_json::json!("did:key:z6M")),
        );

        registry.set_replicator_info(ReplicatorInfo::from_raw_with_filters(
            peer.clone(),
            vec!["users".to_string(), "posts".to_string()],
            vec![],
            filters.clone(),
        ));

        assert!(registry.is_replicator("users", &peer));
        assert!(registry.is_filtered_replicator("users", &peer));
        assert!(!registry.is_filtered_replicator("posts", &peer));

        let info = registry.get_replicator_info(&peer).unwrap();
        assert_eq!(info.filters, filters);

        registry.remove_replicator("users", &peer);
        let info = registry.get_replicator_info(&peer).unwrap();
        assert!(info.filters.is_empty());
        assert!(registry.is_replicator("posts", &peer));
    }

    #[test]
    fn test_remove_peer_collections_by_name_vs_cid() {
        // The push registry is keyed by collection CID. Removing by the collection
        // NAME must NOT clear the peer (it never matches the stored CID) — this is
        // exactly the divergence the adapter's name->CID resolution fixes. Removing
        // by the CID does clear the peer. Regression guard for #1038.
        let registry = ReplicatorRegistry::new();
        let peer = random_peer_id();
        let cid = "bafyCollectionCID".to_string();
        let name = "AgentDoc".to_string();

        registry.add_replicator(&cid, &peer);
        assert!(registry.is_replicator(&cid, &peer));

        // Buggy path: caller passed the NAME -> no match -> peer stays in push list.
        let fully_removed = registry.remove_peer_collections(&peer, &[name]);
        assert!(!fully_removed);
        assert!(registry.is_replicator(&cid, &peer));
        assert!(registry.get_replicators(&cid).contains(&peer));

        // Fixed path: caller passed the resolved CID -> peer removed from push list.
        let fully_removed = registry.remove_peer_collections(&peer, std::slice::from_ref(&cid));
        assert!(fully_removed);
        assert!(!registry.is_replicator(&cid, &peer));
        assert!(registry.get_replicators(&cid).is_empty());
    }

    #[test]
    fn test_replicator_registry_get_all_peer_ids() {
        let registry = ReplicatorRegistry::new();
        let peer1 = random_peer_id();
        let peer2 = random_peer_id();
        let peer3 = random_peer_id();

        registry.add_replicator("users", &peer1);
        registry.add_replicator("users", &peer2);
        registry.add_replicator("posts", &peer2);
        registry.add_replicator("comments", &peer3);

        let peer_ids = registry.get_all_peer_ids();
        assert_eq!(peer_ids.len(), 3);
        assert!(peer_ids.contains(&peer1));
        assert!(peer_ids.contains(&peer2));
        assert!(peer_ids.contains(&peer3));
    }

    #[test]
    fn test_replicator_registry_load_accepts_iroh_peer_ids() {
        let registry = ReplicatorRegistry::new();

        let infos = vec![
            ReplicatorInfo::from_raw(
                "iroh-endpoint-id-abc123".to_string(),
                vec!["users".to_string()],
                vec![],
            ),
            ReplicatorInfo::from_raw(
                "iroh-endpoint-id-def456".to_string(),
                vec!["posts".to_string()],
                vec![],
            ),
        ];

        let (loaded, skipped) = registry.load_from_infos(&infos);
        assert_eq!(loaded, 2);
        assert_eq!(skipped, 0);

        assert!(registry.is_replicator("users", "iroh-endpoint-id-abc123"));
        assert!(registry.is_replicator("posts", "iroh-endpoint-id-def456"));
    }

    #[test]
    fn test_replicator_registry_load_empty_collections() {
        let registry = ReplicatorRegistry::new();
        let peer = random_peer_id();

        let infos = vec![ReplicatorInfo::from_raw(peer.clone(), vec![], vec![])];

        let (loaded, skipped) = registry.load_from_infos(&infos);
        assert_eq!(loaded, 1);
        assert_eq!(skipped, 0);

        // Go restores a stored replicator with no collections and lists it
        // (#2702); it is still not a replicator of anything.
        assert_eq!(registry.get_all_peer_ids(), vec![peer.clone()]);
        assert_eq!(registry.list_replicator_info().len(), 1);
        assert!(!registry.is_any_replicator(&peer));
        assert!(!registry.is_replicator("users", &peer));
    }

    #[test]
    fn a_replicator_with_no_collections_is_listed_and_grants_nothing() {
        let registry = ReplicatorRegistry::new();
        let set_info = random_peer_id();
        let set_collections = random_peer_id();

        registry.set_replicator_info(ReplicatorInfo::from_raw(set_info.clone(), vec![], vec![]));
        registry.set_peer_collections(&set_collections, &[]);

        let mut listed: Vec<String> = registry
            .list_replicator_info()
            .iter()
            .map(|info| info.peer_id_str().to_string())
            .collect();
        listed.sort();
        let mut expected = vec![set_info.clone(), set_collections.clone()];
        expected.sort();
        assert_eq!(listed, expected, "an empty replicator is listed, as in Go");

        for peer in [&set_info, &set_collections] {
            assert!(registry.get_replicator_info(peer).is_some());
            assert!(registry.get_collections(peer).is_empty());
            assert!(!registry.is_any_replicator(peer));
            assert!(!registry.is_replicator("users", peer));
            assert!(!registry.is_filtered_replicator("users", peer));
        }
        assert!(registry.get_replicators("users").is_empty());
    }

    #[test]
    fn removing_the_last_collection_still_drops_the_entry() {
        // Go deletes a replicator once its last collection is removed
        // (internal/db/p2p/replicator.go deleteReplicator).
        let registry = ReplicatorRegistry::new();
        let peer = random_peer_id();

        registry.add_replicator("users", &peer);
        assert!(registry.remove_peer_collections(&peer, &["users".to_string()]));
        assert!(registry.get_replicator_info(&peer).is_none());
        assert!(registry.list_replicator_info().is_empty());

        registry.set_peer_collections(&peer, &[]);
        assert!(registry.remove_peer_collections(&peer, &[]));
        assert!(registry.get_replicator_info(&peer).is_none());
    }

    #[test]
    fn test_replicator_registry_roundtrip() {
        let registry1 = ReplicatorRegistry::new();
        let peer1 = random_peer_id();
        let peer2 = random_peer_id();

        registry1.add_replicator("users", &peer1);
        registry1.add_replicator("posts", &peer1);
        registry1.add_replicator("users", &peer2);
        registry1.add_replicator("comments", &peer2);

        let infos = registry1.list_replicator_info();

        let registry2 = ReplicatorRegistry::new();
        let (loaded, skipped) = registry2.load_from_infos(&infos);
        assert_eq!(loaded, 2);
        assert_eq!(skipped, 0);

        assert_eq!(
            registry1.is_replicator("users", &peer1),
            registry2.is_replicator("users", &peer1)
        );
        assert_eq!(
            registry1.is_replicator("posts", &peer1),
            registry2.is_replicator("posts", &peer1)
        );
        assert_eq!(
            registry1.is_replicator("users", &peer2),
            registry2.is_replicator("users", &peer2)
        );
        assert_eq!(
            registry1.is_replicator("comments", &peer2),
            registry2.is_replicator("comments", &peer2)
        );
    }
}
