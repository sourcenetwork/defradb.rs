//! Exact rooted CAR authorization, independent of response pagination.

use rapidhash::{HashSetExt, RapidHashMap, RapidHashSet};
use std::collections::VecDeque;
use std::time::Duration;

use blockstore::Blockstore;
use cid::Cid;

use crate::error::{Error, Result};

// Bound server work below the transport request deadline. Exhausting this
// budget is a retryable request failure, never a truncated authorization set.
const AUTHORIZATION_TIMEOUT: Duration = Duration::from_secs(2);

/// Deterministic bound on nodes expanded per rooted walk, so a request for
/// adversarially unreachable CIDs terminates on a count instead of leaning on
/// wall-clock timing. Legitimate histories stop far earlier: the walk ends as
/// soon as every requested CID has been reached. Counting carries across
/// resumed passes, so repeated retries cannot multiply a single root's work.
const AUTHORIZATION_MAX_NODES: usize = 200_000;

/// Roots whose incomplete traversal state is retained for continuation.
const RETAINED_INCOMPLETE_ROOTS: usize = 64;

/// A single traversal pass: what is still looked for, what has been reached,
/// and the not-yet-expanded frontier. Nodes are remembered at push time, so a
/// frontier full of duplicate links costs one queue entry per distinct block.
struct WalkState {
    remaining: RapidHashSet<Cid>,
    authorized: RapidHashSet<Cid>,
    seen: RapidHashSet<Cid>,
    queue: VecDeque<Cid>,
    expanded: usize,
    /// The node popped for the expansion currently in flight. If the budget
    /// fires mid-read it goes back to the front of the frontier, or the
    /// resumed walk could never pass it: it is already remembered as seen,
    /// so nobody would enqueue it again.
    in_flight: Option<Cid>,
}

impl WalkState {
    fn fresh(root: Cid, requested: RapidHashSet<Cid>) -> Self {
        let mut walk = Self {
            remaining: requested,
            authorized: RapidHashSet::new(),
            seen: RapidHashSet::new(),
            queue: VecDeque::new(),
            expanded: 0,
            in_flight: None,
        };
        walk.push(root);
        walk
    }

    /// Resume a pass from a prior incomplete one. The prior grant set is kept
    /// only where this request still asks for it, and the frontier continues
    /// from where the budget ran out, so nothing is re-read and no CID is
    /// granted that the walk has not reached.
    fn resumed(prior: IncompleteWalk, requested: RapidHashSet<Cid>) -> Self {
        let mut authorized = RapidHashSet::new();
        let mut remaining = RapidHashSet::new();
        for cid in requested {
            if prior.authorized.contains(&cid) {
                authorized.insert(cid);
            } else {
                remaining.insert(cid);
            }
        }
        let mut queue = VecDeque::new();
        for cid in prior.frontier {
            queue.push_back(cid);
        }
        Self {
            remaining,
            authorized,
            seen: prior.seen,
            queue,
            expanded: 0,
            in_flight: None,
        }
    }

    fn push(&mut self, cid: Cid) {
        if self.seen.insert(cid) {
            self.queue.push_back(cid);
        }
    }

    async fn run<B: Blockstore>(
        &mut self,
        blockstore: &B,
        budget: Duration,
        max_nodes: usize,
    ) -> Result<()> {
        let outcome = n0_future::time::timeout(budget, async {
            loop {
                if self.remaining.is_empty() {
                    self.in_flight = None;
                    return Ok(());
                }
                let Some(cid) = self.queue.pop_front() else {
                    self.in_flight = None;
                    return Ok(());
                };
                if self.expanded >= max_nodes {
                    self.in_flight = None;
                    return Err(authorization_exhausted(false, self.expanded));
                }
                self.in_flight = Some(cid);
                self.expanded += 1;
                // In-memory blockstores may never suspend. Yield so the
                // budget and other bounded serve tasks can make progress on
                // deep graphs.
                if self.expanded.is_multiple_of(128) {
                    tokio::task::yield_now().await;
                }
                let size = blockstore
                    .get_size(&cid)
                    .await
                    .map_err(Error::from_blockstore)?;
                let Some(size) = size else {
                    continue;
                };
                if self.remaining.remove(&cid) {
                    self.authorized.insert(cid);
                }
                // An oversized block is a blob leaf in practice: authorize it
                // if requested, but never read its payload for links, so
                // per-node work stays bounded however large a linked block is.
                if size > crate::sync::car::CAR_MAX_BYTES {
                    continue;
                }
                let data = blockstore.get(&cid).await.map_err(Error::from_blockstore)?;
                let Some(data) = data else {
                    continue;
                };
                // Preserve the existing KMS boundary: encryption links never
                // confer CAR authority, even under an authorized document
                // root.
                let links = super::manager::links::extract_ipld_links(&data).unwrap_or_default();
                for child in links {
                    self.push(child);
                }
                self.in_flight = None;
            }
        })
        .await;
        match outcome {
            Ok(inner) => inner,
            Err(_) => {
                if let Some(cid) = self.in_flight.take() {
                    self.queue.push_front(cid);
                }
                Err(authorization_exhausted(true, self.expanded))
            }
        }
    }
}

fn authorization_exhausted(timed_out: bool, expanded: usize) -> Error {
    tracing::debug!(
        expanded,
        timed_out,
        "Rooted authorization walk exhausted its budget; failing closed to per-block grants"
    );
    Error::ResponseTimeout
}

/// Continuation state for rooted walks that exhausted a budget, so a retried
/// request resumes the traversal instead of restarting at the root. A valid
/// history deeper than one request budget then converges across retries
/// without broadening authority: a CID is granted only once the walk reaches
/// it, and the node budget carries across passes.
#[derive(Default)]
pub(crate) struct RootedAuthorizationProgress {
    incomplete: std::sync::Mutex<RapidHashMap<Cid, IncompleteWalk>>,
    eviction_order: std::sync::Mutex<VecDeque<Cid>>,
}

struct IncompleteWalk {
    seen: RapidHashSet<Cid>,
    frontier: Vec<Cid>,
    authorized: RapidHashSet<Cid>,
}

impl RootedAuthorizationProgress {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// One rooted authorization attempt, resuming and updating retained
    /// progress for this root as needed.
    pub(crate) async fn walk<B: Blockstore>(
        &self,
        blockstore: &B,
        root: Cid,
        requested: RapidHashSet<Cid>,
    ) -> Result<RapidHashSet<Cid>> {
        self.walk_with_limits(
            blockstore,
            root,
            requested,
            AUTHORIZATION_TIMEOUT,
            AUTHORIZATION_MAX_NODES,
        )
        .await
    }

    pub(crate) async fn walk_with_limits<B: Blockstore>(
        &self,
        blockstore: &B,
        root: Cid,
        requested: RapidHashSet<Cid>,
        budget: Duration,
        max_nodes: usize,
    ) -> Result<RapidHashSet<Cid>> {
        let prior = self.take_incomplete(&root);
        let expanded_before;
        let mut state = match prior {
            Some(prior) => {
                expanded_before = prior.seen.len();
                WalkState::resumed(prior, requested)
            }
            None => {
                expanded_before = 0;
                WalkState::fresh(root, requested)
            }
        };
        state.expanded = expanded_before;
        match state.run(blockstore, budget, max_nodes).await {
            Ok(()) => {
                self.forget(&root);
                Ok(state.authorized)
            }
            Err(error) => {
                self.retain(
                    root,
                    IncompleteWalk {
                        seen: state.seen,
                        frontier: state.queue.into_iter().collect(),
                        authorized: state.authorized,
                    },
                );
                Err(error)
            }
        }
    }

    fn take_incomplete(&self, root: &Cid) -> Option<IncompleteWalk> {
        self.incomplete.lock().ok()?.remove(root)
    }

    fn forget(&self, root: &Cid) {
        if let Ok(mut incomplete) = self.incomplete.lock() {
            incomplete.remove(root);
        }
    }

    fn retain(&self, root: Cid, walk: IncompleteWalk) {
        let Ok(mut incomplete) = self.incomplete.lock() else {
            return;
        };
        let Ok(mut order) = self.eviction_order.lock() else {
            return;
        };
        if !incomplete.contains_key(&root) {
            order.push_back(root);
            while order.len() > RETAINED_INCOMPLETE_ROOTS {
                let evicted = order.pop_front();
                if let Some(evicted) = evicted {
                    incomplete.remove(&evicted);
                }
            }
        }
        incomplete.insert(root, walk);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blockstore::DefraBlockstore;
    use ipld_core::{codec::Codec, ipld, ipld::Ipld};
    use multihash_codetable::{Code, MultihashDigest};
    use serde_ipld_dagcbor::codec::DagCborCodec;
    use std::sync::Arc;
    use storage::RegolithStore;

    async fn walk_once<B: Blockstore>(
        store: &B,
        root: Cid,
        requested: RapidHashSet<Cid>,
        budget: Duration,
        max_nodes: usize,
    ) -> Result<RapidHashSet<Cid>> {
        RootedAuthorizationProgress::new()
            .walk_with_limits(store, root, requested, budget, max_nodes)
            .await
    }

    fn ipld_block(value: u64) -> (Cid, Vec<u8>) {
        let data = DagCborCodec::encode_to_vec(&ipld!({"value": value})).unwrap();
        let cid = Cid::new_v1(0x71, Code::Sha2_256.digest(&data));
        (cid, data)
    }

    #[tokio::test]
    async fn root_authority_excludes_unrelated_and_encryption_blocks() {
        use defra_core::{Block, CrdtDelta, DAGLink, LwwDeltaPayload};

        let store = DefraBlockstore::new(Arc::new(RegolithStore::in_memory().unwrap()), true);
        let mut cids = Vec::new();
        for value in 0..3 {
            let (cid, data) = ipld_block(value);
            store.put(&cid, &data).await.unwrap();
            cids.push(cid);
        }
        let block = Block::new_with_options(
            CrdtDelta::Lww(LwwDeltaPayload {
                field_name: "secret".to_string(),
                priority: 1,
                schema_version_id: "schema".to_string(),
                data: b"ciphertext".to_vec(),
            }),
            vec![],
            vec![DAGLink::new("secret", cids[0])],
            Some(cids[1]),
            None,
        );
        let data = block.to_dag_cbor().unwrap();
        let root = block.generate_cid().unwrap();
        store.put(&root, &data).await.unwrap();
        let authorized = RootedAuthorizationProgress::new()
            .walk(&store, root, cids.iter().copied().collect())
            .await
            .unwrap();
        assert_eq!(authorized, RapidHashSet::from_iter([cids[0]]));
    }

    #[tokio::test(start_paused = true)]
    async fn exhausted_budget_is_retryable_not_partial_authorization() {
        let store = DefraBlockstore::new(Arc::new(RegolithStore::in_memory().unwrap()), true);
        let (leaf, leaf_data) = ipld_block(0);
        store.put(&leaf, &leaf_data).await.unwrap();
        let mut root = leaf;
        for value in 1..=256 {
            let data =
                DagCborCodec::encode_to_vec(&ipld!({"child": root, "value": value})).unwrap();
            root = Cid::new_v1(0x71, Code::Sha2_256.digest(&data));
            store.put(&root, &data).await.unwrap();
        }
        let result = walk_once(
            &store,
            root,
            RapidHashSet::from_iter([root, leaf]),
            Duration::ZERO,
            AUTHORIZATION_MAX_NODES,
        )
        .await;
        let error =
            result.expect_err("a partial grant must not look like a complete authorization result");
        assert!(matches!(error, Error::ResponseTimeout));
        assert!(error.is_connection_like());
        let authorized = RootedAuthorizationProgress::new()
            .walk(&store, root, RapidHashSet::from_iter([leaf]))
            .await
            .unwrap();
        assert_eq!(authorized, RapidHashSet::from_iter([leaf]));
    }

    /// A dense DAG whose every node links every sibling expands each distinct
    /// block exactly once; duplicate frontier links cost no extra reads.
    #[tokio::test]
    async fn duplicate_frontier_links_expand_each_block_once() {
        struct CountingReads {
            inner: DefraBlockstore<RegolithStore>,
            reads: std::sync::atomic::AtomicUsize,
        }

        #[async_trait::async_trait]
        impl Blockstore for CountingReads {
            async fn get(&self, cid: &Cid) -> blockstore::Result<Option<bytes::Bytes>> {
                self.reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
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

        let inner = DefraBlockstore::new(Arc::new(RegolithStore::in_memory().unwrap()), true);
        let store = CountingReads {
            inner,
            reads: std::sync::atomic::AtomicUsize::new(0),
        };
        // A ring of nodes, every node linking its two neighbours both ways,
        // plus the root linking all of them: the naive frontier queues each
        // node up to three times.
        let mut ring = Vec::new();
        for value in 0..16 {
            let (cid, data) = ipld_block(value);
            store.inner.put(&cid, &data).await.unwrap();
            ring.push(cid);
        }
        let mut root_links = Vec::new();
        for (index, cid) in ring.iter().enumerate() {
            let previous = ring[(index + ring.len() - 1) % ring.len()];
            let next = ring[(index + 1) % ring.len()];
            let data =
                DagCborCodec::encode_to_vec(&ipld!({"prev": previous, "next": next, "self": cid}))
                    .unwrap();
            // Rewrite each ring node to carry its neighbour links.
            let linked = Cid::new_v1(0x71, Code::Sha2_256.digest(&data));
            store.inner.put(&linked, &data).await.unwrap();
            root_links.push(linked);
        }
        let root_links_ipld: Vec<_> = root_links.iter().map(|cid| Ipld::Link(*cid)).collect();
        let root_data = DagCborCodec::encode_to_vec(&ipld!({"nodes": root_links_ipld})).unwrap();
        let root = Cid::new_v1(0x71, Code::Sha2_256.digest(&root_data));
        store.inner.put(&root, &root_data).await.unwrap();

        let authorized = RootedAuthorizationProgress::new()
            .walk(&store, root, root_links.iter().copied().collect())
            .await
            .unwrap();
        assert_eq!(authorized.len(), root_links.len());
        assert_eq!(
            store.reads.load(std::sync::atomic::Ordering::SeqCst),
            root_links.len() + 1,
            "each distinct block is payload-read exactly once, root included"
        );
    }

    /// An oversized walk node is authorized when requested but never read for
    /// links: its descendants stay ungranted, failing closed.
    #[tokio::test]
    async fn oversized_walk_node_is_authorized_but_not_traversed() {
        let store = DefraBlockstore::new(Arc::new(RegolithStore::in_memory().unwrap()), true);
        let (leaf, leaf_data) = ipld_block(0);
        store.put(&leaf, &leaf_data).await.unwrap();
        let oversized_data = vec![0; crate::sync::car::CAR_MAX_BYTES + 1];
        let oversized = Cid::new_v1(0x71, Code::Sha2_256.digest(&oversized_data));
        store.put(&oversized, &oversized_data).await.unwrap();
        let root_data = DagCborCodec::encode_to_vec(&ipld!({"oversized": oversized})).unwrap();
        let root = Cid::new_v1(0x71, Code::Sha2_256.digest(&root_data));
        store.put(&root, &root_data).await.unwrap();

        let authorized = RootedAuthorizationProgress::new()
            .walk(&store, root, RapidHashSet::from_iter([oversized, leaf]))
            .await
            .unwrap();
        assert_eq!(
            authorized,
            RapidHashSet::from_iter([oversized]),
            "the oversized node is granted, the block behind it is not"
        );
    }

    /// Unreachable requested CIDs end the walk deterministically on the node
    /// budget instead of exploring forever or relying on wall-clock timing.
    #[tokio::test]
    async fn unreachable_requested_cids_end_on_the_node_budget() {
        let store = DefraBlockstore::new(Arc::new(RegolithStore::in_memory().unwrap()), true);
        let (leaf, leaf_data) = ipld_block(0);
        store.put(&leaf, &leaf_data).await.unwrap();
        let mut root = leaf;
        for value in 1..=64 {
            let data =
                DagCborCodec::encode_to_vec(&ipld!({"child": root, "value": value})).unwrap();
            root = Cid::new_v1(0x71, Code::Sha2_256.digest(&data));
            store.put(&root, &data).await.unwrap();
        }
        let (absent, absent_data) = ipld_block(9999);
        let result = walk_once(
            &store,
            root,
            RapidHashSet::from_iter([absent]),
            Duration::from_secs(3600),
            16,
        )
        .await;
        let error = result.expect_err("exploration must stop on the node budget");
        assert!(matches!(error, Error::ResponseTimeout));
        assert!(error.is_connection_like());
        drop(absent_data);
    }

    /// A history deeper than one budget converges across retries: the second
    /// pass resumes from the retained frontier instead of restarting, and the
    /// grant still only contains reached CIDs.
    #[tokio::test(start_paused = true)]
    async fn exhausted_walk_resumes_and_converges_on_retry() {
        struct SlowStore {
            inner: DefraBlockstore<RegolithStore>,
            delay: Duration,
        }

        #[async_trait::async_trait]
        impl Blockstore for SlowStore {
            async fn get(&self, cid: &Cid) -> blockstore::Result<Option<bytes::Bytes>> {
                tokio::time::sleep(self.delay).await;
                self.inner.get(cid).await
            }

            async fn get_size(&self, cid: &Cid) -> blockstore::Result<Option<usize>> {
                tokio::time::sleep(self.delay).await;
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

        let inner = DefraBlockstore::new(Arc::new(RegolithStore::in_memory().unwrap()), true);
        let store = SlowStore {
            inner,
            delay: Duration::from_millis(20),
        };
        // Twelve nodes at 20ms per read (40ms with the size preflight)
        // against a 200ms budget: no single pass can walk the whole chain, so
        // a walk that restarts at the root every retry never finishes. Only
        // resuming from the retained frontier converges.
        let (deep, deep_data) = ipld_block(0);
        store.inner.put(&deep, &deep_data).await.unwrap();
        let mut root = deep;
        for value in 1..=11 {
            let data =
                DagCborCodec::encode_to_vec(&ipld!({"child": root, "value": value})).unwrap();
            root = Cid::new_v1(0x71, Code::Sha2_256.digest(&data));
            store.inner.put(&root, &data).await.unwrap();
        }

        let progress = RootedAuthorizationProgress::new();
        let requested = RapidHashSet::from_iter([deep]);
        let budget = Duration::from_millis(200);
        let mut converged = false;
        for attempt in 0..10 {
            match progress
                .walk_with_limits(
                    &store,
                    root,
                    requested.clone(),
                    budget,
                    AUTHORIZATION_MAX_NODES,
                )
                .await
            {
                Ok(authorized) => {
                    assert_eq!(authorized, RapidHashSet::from_iter([deep]));
                    assert!(attempt > 0, "a single pass cannot finish this walk");
                    converged = true;
                    break;
                }
                Err(error) => assert!(matches!(error, Error::ResponseTimeout)),
            }
        }
        assert!(
            converged,
            "resumed walks must converge where restarting walks cannot"
        );
    }
}
