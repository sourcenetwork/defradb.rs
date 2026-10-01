struct CountingReads {
    inner: DefraBlockstore<RegolithStore>,
    reads: std::sync::atomic::AtomicUsize,
    block_read: tokio::sync::Semaphore,
    reading: tokio::sync::Notify,
    delay: Duration,
    fail_size_once: std::sync::atomic::AtomicBool,
    fail_read_once: std::sync::atomic::AtomicBool,
}

#[async_trait::async_trait]
impl Blockstore for CountingReads {
    async fn get(&self, cid: &Cid) -> blockstore::Result<Option<bytes::Bytes>> {
        self.reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if self
            .fail_read_once
            .swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            return Err(blockstore::Error::Internal("injected read failure".into()));
        }
        self.reading.notify_one();
        self.block_read.acquire().await.unwrap().forget();
        if !self.delay.is_zero() {
            tokio::time::sleep(self.delay).await;
        }
        self.inner.get(cid).await
    }

    async fn get_size(&self, cid: &Cid) -> blockstore::Result<Option<usize>> {
        if self
            .fail_size_once
            .swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            return Err(blockstore::Error::Internal("injected size failure".into()));
        }
        if !self.delay.is_zero() {
            tokio::time::sleep(self.delay).await;
        }
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

impl CountingReads {
    fn new(permits: usize) -> Self {
        Self {
            inner: DefraBlockstore::new(Arc::new(RegolithStore::in_memory().unwrap()), true),
            reads: std::sync::atomic::AtomicUsize::new(0),
            block_read: tokio::sync::Semaphore::new(permits),
            reading: tokio::sync::Notify::new(),
            delay: Duration::ZERO,
            fail_size_once: std::sync::atomic::AtomicBool::new(false),
            fail_read_once: std::sync::atomic::AtomicBool::new(false),
        }
    }
}

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
        let data = DagCborCodec::encode_to_vec(&ipld!({"child": root, "value": value})).unwrap();
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

/// Shared children and duplicate links are expanded once.
#[tokio::test]
async fn duplicate_frontier_links_expand_each_block_once() {
    let store = CountingReads::new(1000);
    let count = 16usize;
    let mut leaves = Vec::new();
    for value in 0..count {
        let (cid, data) = ipld_block(value as u64);
        store.put(&cid, &data).await.unwrap();
        leaves.push(cid);
    }
    let mut requested: RapidHashSet<Cid> = leaves.iter().copied().collect();
    let mut root_links = Vec::new();
    for index in 0..count {
        let data = DagCborCodec::encode_to_vec(&ipld!({
            "prev": leaves[(index + count - 1) % count],
            "same": [leaves[index], leaves[index]],
            "next": leaves[(index + 1) % count],
        }))
        .unwrap();
        let cid = Cid::new_v1(0x71, Code::Sha2_256.digest(&data));
        store.put(&cid, &data).await.unwrap();
        requested.insert(cid);
        root_links.push(Ipld::Link(cid));
    }
    let root_data = DagCborCodec::encode_to_vec(&ipld!({"nodes": root_links})).unwrap();
    let root = Cid::new_v1(0x71, Code::Sha2_256.digest(&root_data));
    store.put(&root, &root_data).await.unwrap();
    let progress = RootedAuthorizationProgress::new();
    assert!(progress
        .walk_with_limits(
            &store,
            root,
            requested.clone(),
            Duration::from_secs(30),
            count + 1
        )
        .await
        .is_err());
    let cursor = progress
        .cursors
        .lock()
        .unwrap()
        .entries
        .values()
        .next()
        .unwrap()
        .clone();
    assert_eq!(cursor.lock().await.queue.len(), count);
    let authorized = progress.walk(&store, root, requested).await.unwrap();
    assert_eq!(
        authorized.len(),
        count * 2,
        "every requested block must be reached"
    );
    assert_eq!(
        store.reads.load(std::sync::atomic::Ordering::SeqCst),
        count * 2 + 1,
        "each distinct block is payload-read exactly once, root included"
    );
}

/// The node gate preserves the next CID for a later pass.
#[tokio::test(start_paused = true)]
async fn node_budget_preserves_the_unexpanded_frontier() {
    let store = DefraBlockstore::new(Arc::new(RegolithStore::in_memory().unwrap()), true);
    let (leaf, leaf_data) = ipld_block(0);
    store.put(&leaf, &leaf_data).await.unwrap();
    let mut root = leaf;
    for value in 1..=200 {
        let data = DagCborCodec::encode_to_vec(&ipld!({"child": root, "value": value})).unwrap();
        root = Cid::new_v1(0x71, Code::Sha2_256.digest(&data));
        store.put(&root, &data).await.unwrap();
    }

    let progress = RootedAuthorizationProgress::new();
    let requested = RapidHashSet::from_iter([leaf]);
    let err = progress
        .walk_with_limits(
            &store,
            root,
            requested.clone(),
            Duration::from_secs(30),
            100,
        )
        .await
        .unwrap_err();
    assert!(matches!(err, Error::ResponseTimeout));

    // A fresh walk cannot finish 201 nodes within this budget.
    let authorized = progress
        .walk_with_limits(&store, root, requested, Duration::from_secs(30), 101)
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
        let data = DagCborCodec::encode_to_vec(&ipld!({"child": root, "value": value})).unwrap();
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
            101,
        )
        .await
        .unwrap();
    assert_eq!(authorized, RapidHashSet::from_iter([leaf]));
}

/// An oversized walk node is authorized when requested but never read for
/// links: its descendants stay ungranted, failing closed.
#[tokio::test]
async fn oversized_walk_node_is_authorized_but_not_traversed() {
    let store = CountingReads::new(100);
    let (leaf, leaf_data) = ipld_block(0);
    store.put(&leaf, &leaf_data).await.unwrap();
    let oversized_data = DagCborCodec::encode_to_vec(&ipld!({
        "child": leaf,
        "padding": Ipld::Bytes(vec![0; crate::sync::car::CAR_MAX_BYTES]),
    }))
    .unwrap();
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
    assert_eq!(store.reads.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[tokio::test]
async fn interleaved_want_lists_keep_independent_progress() {
    let store = DefraBlockstore::new(Arc::new(RegolithStore::in_memory().unwrap()), true);
    let (leaf, data) = ipld_block(77);
    store.put(&leaf, &data).await.unwrap();
    let mut root = leaf;
    for value in 0..10 {
        let data = DagCborCodec::encode_to_vec(&ipld!({"child": root, "value": value})).unwrap();
        root = Cid::new_v1(0x71, Code::Sha2_256.digest(&data));
        store.put(&root, &data).await.unwrap();
    }
    let progress = RootedAuthorizationProgress::new();
    let mut finished = false;
    for _ in 0..6 {
        if let Ok(result) = progress
            .walk_with_limits(
                &store,
                root,
                RapidHashSet::from_iter([leaf]),
                Duration::from_secs(30),
                2,
            )
            .await
        {
            assert_eq!(result, RapidHashSet::from_iter([leaf]));
            finished = true;
            break;
        }
        assert_eq!(
            progress
                .walk(&store, root, RapidHashSet::from_iter([root]))
                .await
                .unwrap(),
            RapidHashSet::from_iter([root])
        );
    }
    assert!(finished, "another want-list must not reset the long walk");
}

#[tokio::test]
async fn a_missing_intermediate_is_revisited_after_delivery() {
    let store = DefraBlockstore::new(Arc::new(RegolithStore::in_memory().unwrap()), true);
    let (leaf, leaf_data) = ipld_block(88);
    store.put(&leaf, &leaf_data).await.unwrap();
    let middle_data = DagCborCodec::encode_to_vec(&ipld!({"child": leaf})).unwrap();
    let middle = Cid::new_v1(0x71, Code::Sha2_256.digest(&middle_data));
    let root_data = DagCborCodec::encode_to_vec(&ipld!({"child": middle})).unwrap();
    let root = Cid::new_v1(0x71, Code::Sha2_256.digest(&root_data));
    store.put(&root, &root_data).await.unwrap();
    let progress = RootedAuthorizationProgress::new();
    assert!(progress
        .walk(&store, root, RapidHashSet::from_iter([leaf]))
        .await
        .is_err());
    store.put(&middle, &middle_data).await.unwrap();
    assert_eq!(
        progress
            .walk(&store, root, RapidHashSet::from_iter([leaf]))
            .await
            .unwrap(),
        RapidHashSet::from_iter([leaf])
    );
}

#[tokio::test]
async fn storage_errors_preserve_the_interrupted_node() {
    for fail_size in [false, true] {
        let store = CountingReads::new(2);
        let (leaf, data) = ipld_block(300);
        store.put(&leaf, &data).await.unwrap();
        let data = DagCborCodec::encode_to_vec(&ipld!({"child": leaf})).unwrap();
        let root = Cid::new_v1(0x71, Code::Sha2_256.digest(&data));
        store.put(&root, &data).await.unwrap();
        let failure = if fail_size {
            &store.fail_size_once
        } else {
            &store.fail_read_once
        };
        failure.store(true, std::sync::atomic::Ordering::SeqCst);
        let progress = RootedAuthorizationProgress::new();
        let requested = RapidHashSet::from_iter([leaf]);
        assert!(progress
            .walk(&store, root, requested.clone())
            .await
            .is_err());
        assert_eq!(
            progress
                .walk(&store, root, requested.clone())
                .await
                .unwrap(),
            requested
        );
    }
}

#[tokio::test]
async fn cancellation_during_a_payload_read_keeps_the_cursor() {
    let store = CountingReads::new(0);
    let (leaf, data) = ipld_block(99);
    store.put(&leaf, &data).await.unwrap();
    let data = DagCborCodec::encode_to_vec(&ipld!({"child": leaf})).unwrap();
    let root = Cid::new_v1(0x71, Code::Sha2_256.digest(&data));
    store.put(&root, &data).await.unwrap();
    let progress = RootedAuthorizationProgress::new();
    let mut request = Box::pin(progress.walk(&store, root, RapidHashSet::from_iter([leaf])));
    tokio::select! {
        _ = store.reading.notified() => {},
        result = &mut request => panic!("read must be gated: {result:?}"),
    }
    drop(request);
    store.block_read.add_permits(2);
    assert_eq!(
        progress
            .walk(&store, root, RapidHashSet::from_iter([leaf]))
            .await
            .unwrap(),
        RapidHashSet::from_iter([leaf])
    );
    assert_eq!(store.reads.load(std::sync::atomic::Ordering::SeqCst), 3);
}

#[tokio::test]
async fn missing_unrequested_branch_does_not_block_a_complete_grant() {
    let store = CountingReads::new(10);
    let (leaf, data) = ipld_block(200);
    store.put(&leaf, &data).await.unwrap();
    let (missing, _) = ipld_block(201);
    let data = DagCborCodec::encode_to_vec(&ipld!({"a": missing, "b": leaf})).unwrap();
    let root = Cid::new_v1(0x71, Code::Sha2_256.digest(&data));
    store.put(&root, &data).await.unwrap();
    assert_eq!(
        RootedAuthorizationProgress::new()
            .walk(&store, root, RapidHashSet::from_iter([leaf]))
            .await
            .unwrap(),
        RapidHashSet::from_iter([leaf])
    );
}

#[tokio::test]
async fn concurrent_requests_share_the_same_cursor() {
    let store = CountingReads::new(0);
    let (leaf, data) = ipld_block(101);
    store.put(&leaf, &data).await.unwrap();
    let data = DagCborCodec::encode_to_vec(&ipld!({"child": leaf})).unwrap();
    let root = Cid::new_v1(0x71, Code::Sha2_256.digest(&data));
    store.put(&root, &data).await.unwrap();
    let progress = RootedAuthorizationProgress::new();
    let mut requests = Vec::new();
    for _ in 0..8 {
        let mut request = Box::pin(progress.walk(&store, root, RapidHashSet::from_iter([leaf])));
        assert!(futures::poll!(request.as_mut()).is_pending());
        requests.push(request);
    }
    store.block_read.add_permits(2);
    for result in futures::future::join_all(requests).await {
        assert_eq!(result.unwrap(), RapidHashSet::from_iter([leaf]));
    }
    assert_eq!(store.reads.load(std::sync::atomic::Ordering::SeqCst), 2);
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
        let data = DagCborCodec::encode_to_vec(&ipld!({"child": root, "value": value})).unwrap();
        root = Cid::new_v1(0x71, Code::Sha2_256.digest(&data));
        store.put(&root, &data).await.unwrap();
    }
    let (absent, _) = ipld_block(9999);
    let progress = RootedAuthorizationProgress::new();
    for _ in 0..3 {
        let error = progress
            .walk_with_limits(
                &store,
                root,
                RapidHashSet::from_iter([absent]),
                Duration::from_secs(3600),
                16,
            )
            .await
            .expect_err("exploration must stop on each pass's node budget");
        assert!(matches!(error, Error::ResponseTimeout));
    }
}

/// A history deeper than one budget converges across retries: the second
/// pass resumes from the retained frontier instead of restarting, and the
/// grant still only contains reached CIDs.
#[tokio::test(start_paused = true)]
async fn exhausted_walk_resumes_and_converges_on_retry() {
    let mut store = CountingReads::new(1000);
    store.delay = Duration::from_millis(20);
    // Twelve nodes at 20ms per read (40ms with the size preflight)
    // against a 200ms budget: no single pass can walk the whole chain, so
    // a walk that restarts at the root every retry never finishes. Only
    // resuming from the retained frontier converges.
    let (deep, deep_data) = ipld_block(0);
    store.inner.put(&deep, &deep_data).await.unwrap();
    let mut root = deep;
    for value in 1..=11 {
        let data = DagCborCodec::encode_to_vec(&ipld!({"child": root, "value": value})).unwrap();
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
