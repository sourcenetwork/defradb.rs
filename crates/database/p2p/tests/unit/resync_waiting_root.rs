use super::*;
use crate::sync::pending_store::PendingDagStore;
use blockstore::Blockstore;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Counts the reads of one block.
struct CountingGets {
    inner: DefraBlockstore<RegolithStore>,
    watched: Cid,
    gets: AtomicUsize,
}

#[async_trait::async_trait]
impl Blockstore for CountingGets {
    async fn get(&self, cid: &Cid) -> blockstore::Result<Option<bytes::Bytes>> {
        if *cid == self.watched {
            self.gets.fetch_add(1, Ordering::SeqCst);
        }
        self.inner.get(cid).await
    }

    async fn get_size(&self, cid: &Cid) -> blockstore::Result<Option<usize>> {
        self.inner.get_size(cid).await
    }

    async fn put(&self, cid: &Cid, data: &[u8]) -> blockstore::Result<()> {
        self.inner.put(cid, data).await
    }

    async fn put_many(&self, blocks: &[(&Cid, &[u8])]) -> blockstore::Result<()> {
        self.inner.put_many(blocks).await
    }

    async fn has(&self, cid: &Cid) -> blockstore::Result<bool> {
        self.inner.has(cid).await
    }

    async fn delete(&self, cid: &Cid) -> blockstore::Result<()> {
        self.inner.delete(cid).await
    }

    async fn all_cids(&self) -> blockstore::Result<Vec<Cid>> {
        self.inner.all_cids().await
    }

    fn hash_on_read(&self, enabled: bool) {
        self.inner.hash_on_read(enabled)
    }

    async fn is_merged(&self, cid: &Cid) -> blockstore::Result<bool> {
        self.inner.is_merged(cid).await
    }

    async fn mark_as_merged(&self, cid: &Cid) -> blockstore::Result<()> {
        self.inner.mark_as_merged(cid).await
    }

    async fn get_unmerged(&self) -> blockstore::Result<Vec<Cid>> {
        self.inner.get_unmerged().await
    }
}

/// The resync sweep reads and walks a durable root only when the map has a
/// place for it. A head acknowledged on its record alone while the map is
/// full is, at every sweep until an entry frees, a root the map would refuse
/// for capacity: it keeps its record without its block being read or its
/// DAG walked.
#[nuclei::test]
async fn the_sweep_does_not_walk_a_root_the_map_has_no_place_for() {
    let heads: Vec<(&str, &str, Cid, Vec<u8>)> = [("a", "peer-1"), ("b", "peer-2")]
        .into_iter()
        .map(|(field, peer)| {
            let (leaf, _) = lww_leaf(field);
            let (root, bytes) = composite_node(field, leaf, 1);
            (field, peer, root, bytes)
        })
        .collect();
    let (resident, waiting) = (heads[0].2, heads[1].2);
    let blockstore = Arc::new(CountingGets {
        inner: DefraBlockstore::new(Arc::new(RegolithStore::in_memory().unwrap()), true),
        watched: waiting,
        gets: AtomicUsize::new(0),
    });
    let (manager, _events) = SyncManager::new(
        Arc::clone(&blockstore),
        Arc::new(PeerStateTracker::new()),
        SyncConfig {
            max_pending_dags: 1,
            ..Default::default()
        },
    );
    manager
        .install_pending_dag_store(Arc::new(PendingDagStore::new(Arc::new(
            RegolithStore::in_memory().unwrap(),
        ))))
        .await;

    for (field, peer, root, bytes) in heads {
        manager
            .process_pushlog(&broadcast(field, root, bytes), Some(peer), false, None)
            .await
            .unwrap_or_else(|error| panic!("head {field} is acknowledged: {error}"));
    }
    assert_eq!(manager.pending_dag_cids(), vec![resident]);
    assert!(manager.persisted.contains(&waiting));

    let before = blockstore.gets.load(Ordering::SeqCst);
    for _ in 0..3 {
        assert_eq!(manager.resync_persisted_pending_dags().await, 0);
    }

    assert_eq!(
        blockstore.gets.load(Ordering::SeqCst) - before,
        0,
        "a root the full map has no place for is not read or walked"
    );
    assert_eq!(manager.pending_dag_cids(), vec![resident]);
    assert!(manager.persisted.contains(&waiting));
}
