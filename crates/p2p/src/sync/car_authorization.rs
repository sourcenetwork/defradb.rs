//! Exact rooted CAR authorization, independent of response pagination.
//!
//! Invariant: a CID is granted only once the walk reaches it. Exhaustion
//! fails closed to independent per-block grants; a budget that outlives one
//! request resumes from a retained frontier without re-reading.

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
    /// Nodes whose payload read completed, across resumed passes. `seen`
    /// also holds unexpanded frontier members, so it cannot meter work: one
    /// wide block can enqueue hundreds of thousands of children, and
    /// charging those against the budget before any is read would exhaust
    /// it on bookkeeping alone.
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

    /// Resume a pass from a prior incomplete one. The prior grant set is
    /// kept only where this request still asks for it, and the frontier
    /// continues from where the budget ran out. `expanded` restarts at zero:
    /// the inherited frontier was never read, and only completed reads are
    /// work.
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
                // The budget gate runs before the pop: a CID popped past
                // the budget is already in `seen`, so neither the queue nor
                // the frontier could ever re-offer it to a resumed pass.
                if self.expanded >= max_nodes {
                    return Err(authorization_exhausted(false, self.expanded));
                }
                let Some(cid) = self.queue.pop_front() else {
                    self.in_flight = None;
                    return Ok(());
                };
                self.in_flight = Some(cid);
                self.expanded += 1;
                // In-memory blockstores may never suspend. Yield so the
                // budget and other bounded serve tasks can make progress on
                // deep graphs.
                if self.expanded.is_multiple_of(128) {
                    tokio::task::yield_now().await;
                }
                let size = match blockstore.get_size(&cid).await {
                    Ok(size) => size,
                    // Same rule as budget exhaustion: the CID returns to the
                    // frontier, or the error also deletes it.
                    Err(error) => {
                        return Err(Error::from_blockstore(error));
                    }
                };
                let Some(size) = size else {
                    self.in_flight = None;
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
                let data = match blockstore.get(&cid).await {
                    Ok(data) => data,
                    Err(error) => {
                        return Err(Error::from_blockstore(error));
                    }
                };
                let Some(data) = data else {
                    self.in_flight = None;
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
        // Every non-Ok exit — wall-clock timeout, inner error, budget —
        // returns the in-flight CID to the front of the frontier, or the
        // resumed pass could never revisit it.
        if !matches!(outcome, Ok(Ok(()))) {
            if let Some(cid) = self.in_flight.take() {
                self.queue.push_front(cid);
            }
        }
        match outcome {
            Ok(inner) => inner,
            Err(_) => Err(authorization_exhausted(true, self.expanded)),
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

/// Retained frontiers for walks that exhausted a budget, keyed by root and
/// valid only for the want-list they were authorizing.
#[derive(Default)]
pub(crate) struct RootedAuthorizationProgress {
    incomplete: std::sync::Mutex<RapidHashMap<Cid, IncompleteWalk>>,
    eviction_order: std::sync::Mutex<VecDeque<Cid>>,
}

struct IncompleteWalk {
    seen: RapidHashSet<Cid>,
    frontier: Vec<Cid>,
    authorized: RapidHashSet<Cid>,
    /// The want-list this walk was authorizing. `seen` is only valid
    /// continuation state for that list: an intermediate visited while
    /// authorizing a different CID set is neither granted nor reachable
    /// from this frontier, so reusing it would silently skip those CIDs.
    requested: RapidHashSet<Cid>,
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
        // An empty want-list authorizes nothing and must not disturb the
        // retained state: the serving filter walks every rooted request,
        // including collections that found no blocks, and a no-op request
        // taking a live cursor is a silent restart for the next real one.
        if requested.is_empty() {
            return Ok(RapidHashSet::new());
        }
        // A retained walk continues only the want-list it was authorizing;
        // a walk kept for a different list is discarded, not reused.
        let retained = self
            .take_incomplete(&root)
            .map(|prior| (prior.requested.clone(), prior));
        let wanted_for_retain = requested.clone();
        let mut state = match retained {
            Some((want_list, prior)) if want_list == requested => {
                WalkState::resumed(prior, requested)
            }
            _ => WalkState::fresh(root, requested),
        };
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
                        requested: wanted_for_retain,
                    },
                );
                Err(error)
            }
        }
    }

    fn take_incomplete(&self, root: &Cid) -> Option<IncompleteWalk> {
        let mut incomplete = self.incomplete.lock().ok()?;
        let walk = incomplete.remove(root)?;
        // The order slot goes with the entry, or a later retain for the
        // same root would queue a second slot for it and the cap would
        // count slots, not roots.
        if let Ok(mut order) = self.eviction_order.lock() {
            order.retain(|queued| queued != root);
        }
        Some(walk)
    }

    fn forget(&self, root: &Cid) {
        if let Ok(mut incomplete) = self.incomplete.lock() {
            incomplete.remove(root);
        }
        if let Ok(mut order) = self.eviction_order.lock() {
            order.retain(|queued| queued != root);
        }
    }

    fn retain(&self, root: Cid, walk: IncompleteWalk) {
        let Ok(mut incomplete) = self.incomplete.lock() else {
            return;
        };
        let Ok(mut order) = self.eviction_order.lock() else {
            return;
        };
        // Most-recently-retained last: a hot root's retries refresh their
        // recency instead of letting a quiet, still-incomplete root be
        // evicted by activity elsewhere.
        order.retain(|queued| queued != &root);
        order.push_back(root);
        while order.len() > RETAINED_INCOMPLETE_ROOTS {
            let evicted = order.pop_front();
            if let Some(evicted) = evicted {
                incomplete.remove(&evicted);
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
        // A two-pass ring: nodes exist first as placeholders so neighbour
        // CIDs are known, then each is rewritten with real payload bytes
        // linking prev, next, and itself. Every link target in the final
        // graph is a block that exists in the store and is on the requested
        // list, so the read count measures genuine dedup, not unvisited
        // shortcuts: the walk must authorize the full ring before it stops.
        let count = 16usize;
        let node_cid = |bytes: &[u8]| Cid::new_v1(0x71, Code::Sha2_256.digest(bytes));
        let nodes: Vec<_> = (0..count)
            .map(|value| {
                let (_, placeholder) = ipld_block(value as u64);
                node_cid(&placeholder)
            })
            .collect();
        let mut root_links = Vec::new();
        for index in 0..count {
            let previous = nodes[(index + count - 1) % count];
            let next = nodes[(index + 1) % count];
            let data = DagCborCodec::encode_to_vec(&ipld!({
                "prev": previous,
                "next": next,
                "self": nodes[index],
            }))
            .unwrap();
            let cid = node_cid(&data);
            store.inner.put(&cid, &data).await.unwrap();
            root_links.push(cid);
        }
        // The requested set covers the whole final ring: nothing is
        // authorized without being visited, and nothing visited is left
        // unread.
        let requested: RapidHashSet<Cid> = root_links.iter().copied().collect();
        let root_links_ipld: Vec<_> = root_links.iter().map(|cid| Ipld::Link(*cid)).collect();
        let root_data = DagCborCodec::encode_to_vec(&ipld!({"nodes": root_links_ipld})).unwrap();
        let root = Cid::new_v1(0x71, Code::Sha2_256.digest(&root_data));
        store.inner.put(&root, &root_data).await.unwrap();

        let authorized = RootedAuthorizationProgress::new()
            .walk(&store, root, requested)
            .await
            .unwrap();
        assert_eq!(
            authorized.len(),
            root_links.len(),
            "the whole ring is authorized: every link target was visited"
        );
        assert_eq!(
            store.reads.load(std::sync::atomic::Ordering::SeqCst),
            root_links.len() + 1,
            "each distinct block is payload-read exactly once, root included"
        );
    }

    /// A walk that exhausts its budget returns the popped CID to the frontier,
    /// so the resumed pass expands it rather than skipping it forever.
    #[tokio::test(start_paused = true)]
    async fn budget_exhaustion_returns_the_popped_cid_to_the_frontier() {
        let store = DefraBlockstore::new(Arc::new(RegolithStore::in_memory().unwrap()), true);
        let (leaf, leaf_data) = ipld_block(0);
        store.put(&leaf, &leaf_data).await.unwrap();
        let mut root = leaf;
        for value in 1..=200 {
            let data =
                DagCborCodec::encode_to_vec(&ipld!({"child": root, "value": value})).unwrap();
            root = Cid::new_v1(0x71, Code::Sha2_256.digest(&data));
            store.put(&root, &data).await.unwrap();
        }

        let progress = RootedAuthorizationProgress::new();
        let requested = RapidHashSet::from_iter([leaf]);
        // Zero budget: the walk suspends at its first 128-node yield while the
        // popped root is in flight.
        let err = progress
            .walk_with_limits(&store, root, requested.clone(), Duration::ZERO, 100)
            .await
            .unwrap_err();
        assert!(matches!(err, Error::ResponseTimeout));

        // The resumed walk completes and reaches the leaf: the root popped by
        // the exhausted pass was not lost to `seen`.
        let authorized = progress
            .walk_with_limits(&store, root, requested, Duration::from_secs(30), 500)
            .await
            .unwrap();
        assert_eq!(authorized, RapidHashSet::from_iter([leaf]));
    }

    /// A second request with a different want-list does not inherit the first
    /// walk's `seen`: its CIDs are re-walked from the root.
    #[tokio::test(start_paused = true)]
    async fn a_different_want_list_starts_a_fresh_walk() {
        let store = DefraBlockstore::new(Arc::new(RegolithStore::in_memory().unwrap()), true);
        let (right, right_data) = ipld_block(20);
        store.put(&right, &right_data).await.unwrap();
        // A deep left arm whose bottom node is the want-list entry: the walk
        // must descend past the budget to reach it, leaving retained state.
        let mut arm = Cid::new_v1(0x71, Code::Sha2_256.digest(&ipld_block(10).1));
        let mut bottom = None;
        for value in 0..200u64 {
            let data = DagCborCodec::encode_to_vec(&ipld!({"child": arm, "depth": value})).unwrap();
            arm = Cid::new_v1(0x71, Code::Sha2_256.digest(&data));
            store.put(&arm, &data).await.unwrap();
            bottom.get_or_insert(arm);
        }
        let bottom = bottom.expect("chain built");
        let root_data = DagCborCodec::encode_to_vec(&ipld!({"left": arm, "right": right})).unwrap();
        let root = Cid::new_v1(0x71, Code::Sha2_256.digest(&root_data));
        store.put(&root, &root_data).await.unwrap();

        let progress = RootedAuthorizationProgress::new();
        // First want-list exhausts its budget descending the arm.
        let err = progress
            .walk_with_limits(
                &store,
                root,
                RapidHashSet::from_iter([bottom]),
                Duration::ZERO,
                100,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, Error::ResponseTimeout));

        // A different want-list walks fresh and authorizes its CID.
        let authorized = progress
            .walk_with_limits(
                &store,
                root,
                RapidHashSet::from_iter([right]),
                Duration::from_secs(30),
                100,
            )
            .await
            .unwrap();
        assert_eq!(authorized, RapidHashSet::from_iter([right]));
    }

    /// An empty want-list authorizes nothing and leaves retained state intact.
    #[tokio::test(start_paused = true)]
    async fn an_empty_want_list_does_not_disturb_retained_progress() {
        let store = DefraBlockstore::new(Arc::new(RegolithStore::in_memory().unwrap()), true);
        let (leaf, leaf_data) = ipld_block(0);
        store.put(&leaf, &leaf_data).await.unwrap();
        let mut root = leaf;
        for value in 1..=200 {
            let data =
                DagCborCodec::encode_to_vec(&ipld!({"child": root, "value": value})).unwrap();
            root = Cid::new_v1(0x71, Code::Sha2_256.digest(&data));
            store.put(&root, &data).await.unwrap();
        }

        let progress = RootedAuthorizationProgress::new();
        let requested = RapidHashSet::from_iter([leaf]);
        let err = progress
            .walk_with_limits(&store, root, requested, Duration::ZERO, 100)
            .await
            .unwrap_err();
        assert!(matches!(err, Error::ResponseTimeout));

        // The serving filter's no-blocks call must not have consumed the walk.
        let empty = progress
            .walk_with_limits(&store, root, RapidHashSet::new(), Duration::ZERO, 100)
            .await
            .unwrap();
        assert!(empty.is_empty());

        let authorized = progress
            .walk_with_limits(
                &store,
                root,
                RapidHashSet::from_iter([leaf]),
                Duration::from_secs(30),
                500,
            )
            .await
            .unwrap();
        assert_eq!(authorized, RapidHashSet::from_iter([leaf]));
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
