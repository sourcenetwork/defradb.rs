use std::sync::Arc;

use prolly::{diff, tree::DEFAULT_TARGET, Cached, Item, MemStore, Store as _, Tree};

use super::{node_key, ProllyNodes, NODES_PER_SWEEP};
use crate::backends::RegolithStore;
use crate::corekv::engine_retry_options;
use crate::corekv::Dropable as _;

/// The cache's budget: far below the tree's size.
const BUDGET: usize = 64 * 1024;
/// Bytes of puts that wait before one transaction writes them.
const PENDING: usize = 32 * 1024;

fn entry(n: u32) -> (Vec<u8>, Vec<u8>) {
    (n.to_be_bytes().to_vec(), format!("heads-{n}").into_bytes())
}

async fn entries<S>(tree: &Tree, store: &S) -> Vec<Item>
where
    S: prolly::Store,
{
    let mut cursor = tree.iter(store);
    let mut items = Vec::new();
    while let Some(item) = cursor.next().await {
        items.push(item.unwrap());
    }
    items
}

fn nodes(store: &Arc<RegolithStore>) -> Cached<ProllyNodes<RegolithStore>> {
    let nodes = ProllyNodes::new(Arc::clone(store), engine_retry_options(5), PENDING);
    Cached::new(nodes, BUDGET)
}

async fn node_keys(store: &RegolithStore) -> Vec<Vec<u8>> {
    let reader = crate::corekv::Store::new_txn(store, true).await.unwrap();
    let mut scan = reader
        .iterator(crate::corekv::IterOptions::new().with_prefix(b"t".to_vec()))
        .await
        .unwrap();
    let mut keys = Vec::new();
    while let Some(pair) = scan.next().await.unwrap() {
        keys.push(pair.key.to_vec());
    }
    scan.close().await.unwrap();
    reader.discard();
    keys
}

#[test]
fn a_reclaim_keeps_only_the_nodes_the_live_roots_name() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let mut regolith =
            ProllyNodes::new(Arc::clone(&store), engine_retry_options(5), usize::MAX);
        let built = Tree::build(&mut regolith, DEFAULT_TARGET, (0..3).map(entry))
            .await
            .unwrap();
        let stored = node_keys(&store).await.len();
        let report = ProllyNodes::reclaim(&*store, &[built.root().unwrap()], None, NODES_PER_SWEEP)
            .await
            .unwrap();
        assert_eq!(report.scanned, stored);
        assert_eq!(report.marked, stored);
        assert_eq!(report.deleted, 0);
        assert!(report.next_start.is_some());

        let mut edited_tree = built;
        edited_tree
            .remove(&mut regolith, &entry(1).0)
            .await
            .unwrap();
        let edited = edited_tree.root().unwrap();
        let report = ProllyNodes::reclaim(&*store, &[edited], None, NODES_PER_SWEEP)
            .await
            .unwrap();
        assert_eq!(report.deleted, 1);
        assert_eq!(node_keys(&store).await.len(), report.marked);

        let mut cursor = edited_tree.iter(&regolith);
        let mut live = Vec::new();
        while let Some(item) = cursor.next().await {
            live.push(item.unwrap());
        }
        assert_eq!(live.len(), 2);
    })
    .unwrap()
}

#[test]
fn a_root_published_after_the_mark_is_not_deleted_by_the_same_snapshot() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let mut regolith =
            ProllyNodes::new(Arc::clone(&store), engine_retry_options(5), usize::MAX);
        let first = Tree::build(&mut regolith, DEFAULT_TARGET, (0..2).map(entry))
            .await
            .unwrap();
        let second = {
            let mut tree = first;
            tree.insert(&mut regolith, b"after".to_vec(), b"v".to_vec())
                .await
                .unwrap();
            tree.root().unwrap()
        };

        let report = ProllyNodes::reclaim(&*store, &[second], None, NODES_PER_SWEEP)
            .await
            .unwrap();
        assert_eq!(report.deleted, report.scanned - report.marked);
        assert_eq!(node_keys(&store).await.len(), report.marked);
    })
    .unwrap()
}

#[test]
fn a_sweep_walks_one_bounded_page_at_a_time() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let mut regolith =
            ProllyNodes::new(Arc::clone(&store), engine_retry_options(5), usize::MAX);
        let tree = Tree::build(&mut regolith, DEFAULT_TARGET, (0..3).map(entry))
            .await
            .unwrap();
        let total = node_keys(&store).await.len();
        let mut start = None;
        let mut scanned = 0;
        let mut pages = 0;
        while let Some(report) = {
            let report = ProllyNodes::reclaim(&*store, &[tree.root().unwrap()], start, 1)
                .await
                .unwrap();
            assert!(report.scanned <= 1);
            scanned += report.scanned;
            pages += 1;
            let has_next = report.next_start.is_some();
            start = report.next_start;
            has_next.then_some(report.deleted)
        } {
            assert_eq!(report, 0);
        }
        assert_eq!(scanned, total);
        assert!(pages > 1);
    })
    .unwrap()
}

#[test]
fn a_sweep_with_no_live_roots_reclaims_every_node_page_by_page() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let mut regolith =
            ProllyNodes::new(Arc::clone(&store), engine_retry_options(5), usize::MAX);
        Tree::build(&mut regolith, DEFAULT_TARGET, (0..3).map(entry))
            .await
            .unwrap();
        let total = node_keys(&store).await.len();
        let mut start = None;
        let mut deleted = 0;
        while {
            let report = ProllyNodes::reclaim(&*store, &[], start, 1).await.unwrap();
            assert!(report.scanned <= 1);
            deleted += report.deleted;
            let has_next = report.next_start.is_some();
            start = report.next_start;
            has_next
        } {}
        assert_eq!(deleted, total);
        assert!(node_keys(&store).await.is_empty());
    })
    .unwrap()
}

#[test]
fn a_purge_leaves_no_t_key() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let mut regolith =
            ProllyNodes::new(Arc::clone(&store), engine_retry_options(5), usize::MAX);
        Tree::build(&mut regolith, DEFAULT_TARGET, (0..2).map(entry))
            .await
            .unwrap();
        assert!(!node_keys(&store).await.is_empty());
        store.drop_all().await.unwrap();
        assert!(node_keys(&store).await.is_empty());
    })
    .unwrap()
}

#[test]
fn a_tree_far_larger_than_the_budget_matches_memory_and_stays_under_it() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let mut regolith = nodes(&store);
        let mut memory = MemStore::new();

        let mut tree = Tree::build(&mut memory, DEFAULT_TARGET, (0..50_000).map(entry))
            .await
            .unwrap();
        let mut stored = Tree::build(&mut regolith, DEFAULT_TARGET, (0..50_000).map(entry))
            .await
            .unwrap();
        assert_eq!(stored, tree, "the same root, so the golden roots hold here");
        assert_eq!(regolith.inner().pending_bytes(), 0, "a build ends flushed");

        let (before, built) = (tree, stored);
        for n in [3, 25_000, 49_999] {
            tree.remove(&mut memory, &entry(n).0).await.unwrap();
            stored.remove(&mut regolith, &entry(n).0).await.unwrap();
        }
        tree.insert(&mut memory, b"new".to_vec(), b"v".to_vec())
            .await
            .unwrap();
        stored
            .insert(&mut regolith, b"new".to_vec(), b"v".to_vec())
            .await
            .unwrap();
        assert_eq!(stored, tree);

        for n in (0..50_000).step_by(97) {
            let key = entry(n).0;
            assert_eq!(
                stored.get(&regolith, &key).await.unwrap(),
                tree.get(&memory, &key).await.unwrap()
            );
        }
        assert_eq!(
            entries(&stored, &regolith).await,
            entries(&tree, &memory).await
        );
        assert_eq!(
            diff(&regolith, &built, &regolith, &stored).await.unwrap(),
            diff(&memory, &before, &memory, &tree).await.unwrap()
        );

        let size: usize = entries(&tree, &memory)
            .await
            .iter()
            .map(Item::encoded_len)
            .sum();
        let stats = regolith.stats();
        assert!(
            size > 20 * BUDGET,
            "the tree ({size} bytes) dwarfs the budget"
        );
        assert!(
            stats.peak <= BUDGET,
            "held {} of {BUDGET} bytes",
            stats.peak
        );
        assert!(stats.evictions > 0 && stats.misses > 0, "{stats:?}");

        // A fresh store over the same regolith, with nothing cached or
        // pending, reads every node from regolith.
        let fresh = nodes(&store);
        assert_eq!(
            entries(&stored, &fresh).await,
            entries(&tree, &memory).await
        );
    })
    .unwrap()
}

#[test]
fn a_put_is_readable_before_and_after_its_flush() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let mut regolith =
            ProllyNodes::new(Arc::clone(&store), engine_retry_options(5), usize::MAX);
        let node = prolly::Node {
            level: 0,
            items: vec![Item {
                key: b"k".to_vec(),
                value: b"v".to_vec(),
            }],
        };
        let hash = regolith.put(node.clone()).await.unwrap();
        assert!(regolith.pending_bytes() > 0);
        assert_eq!(regolith.get(&hash).await.unwrap().as_deref(), Some(&node));
        regolith.flush().await.unwrap();
        assert_eq!(regolith.pending_bytes(), 0);
        assert_eq!(regolith.get(&hash).await.unwrap().as_deref(), Some(&node));

        let txn = crate::corekv::Store::new_txn(&*store, true).await.unwrap();
        assert_eq!(
            txn.get(&node_key(&hash)).await.unwrap().as_deref(),
            Some(node.encode().as_slice())
        );
        txn.discard();
    })
    .unwrap()
}

/// Build and point-read costs, in memory and in regolith behind a cache far
/// smaller than the tree, so most reads miss. Run with `--run-ignored only`.
#[test]
#[ignore]
fn bench_build_and_get_at_a_size_that_misses_the_cache() {
    const COUNT: u32 = 200_000;
    async fn measure<S>(name: &str, store: &mut S)
    where
        S: prolly::Store,
    {
        let start = web_time::Instant::now();
        let tree = Tree::build(store, DEFAULT_TARGET, (0..COUNT).map(entry))
            .await
            .unwrap();
        let build = start.elapsed();
        let start = web_time::Instant::now();
        for n in (0..COUNT).step_by(10) {
            assert!(tree.get(store, &entry(n).0).await.unwrap().is_some());
        }
        let get = start.elapsed();
        println!(
            "BENCH {name} build {:.0} ns/entry, get {:.0} ns/get",
            build.as_nanos() as f64 / f64::from(COUNT),
            get.as_nanos() as f64 / f64::from(COUNT / 10)
        );
    }
    workers::block_on(async {
        for _ in 0..3 {
            measure("memory-async", &mut MemStore::new()).await;
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let mut regolith = nodes(&store);
            measure("regolith-cached", &mut regolith).await;
            println!("BENCH regolith-cached {:?}", regolith.stats());
        }
    })
    .unwrap()
}

/// A regolith store whose first `transact_retry` fails with an I/O error, or,
/// with `stall`, waits forever.
struct FailFirstWrite {
    inner: Arc<RegolithStore>,
    failed: std::sync::atomic::AtomicBool,
    stall: bool,
}

impl crate::corekv::private::Sealed for FailFirstWrite {}

#[async_trait::async_trait]
impl crate::corekv::Store for FailFirstWrite {
    fn crdts(&self) -> &crdt::Crdts {
        self.inner.crdts()
    }

    async fn allocate(
        &self,
        key: &[u8],
        count: u64,
    ) -> crate::corekv::Result<std::ops::Range<u64>> {
        self.inner.allocate(key, count).await
    }

    async fn new_txn(&self, readonly: bool) -> crate::corekv::Result<Box<dyn crate::corekv::Txn>> {
        self.inner.new_txn(readonly).await
    }

    async fn transact_retry(
        &self,
        options: crate::corekv::RetryOptions,
        attempt: crate::corekv::RetryAttempt,
    ) -> Result<crate::corekv::RetryAttemptValue, crate::corekv::RetryAttemptError> {
        if !self.failed.swap(true, std::sync::atomic::Ordering::SeqCst) {
            if self.stall {
                std::future::pending::<()>().await;
            }
            return Err(crate::corekv::RetryAttemptError::Storage(
                crate::corekv::Error::Other("injected write failure".into()),
            ));
        }
        self.inner.transact_retry(options, attempt).await
    }

    async fn close(&self) -> crate::corekv::Result<()> {
        self.inner.close().await
    }
}

#[test]
fn a_failed_flush_keeps_its_nodes_pending_and_the_next_flush_writes_them() {
    workers::block_on(async {
        let regolith = Arc::new(RegolithStore::in_memory().unwrap());
        let store = Arc::new(FailFirstWrite {
            inner: Arc::clone(&regolith),
            failed: std::sync::atomic::AtomicBool::new(false),
            stall: false,
        });
        let mut nodes = ProllyNodes::new(store, engine_retry_options(5), usize::MAX);
        let node = prolly::Node {
            level: 0,
            items: vec![Item {
                key: b"k".to_vec(),
                value: b"v".to_vec(),
            }],
        };
        let hash = nodes.put(node.clone()).await.unwrap();
        let pending = nodes.pending_bytes();
        assert!(pending > 0);

        let error = nodes.flush().await.unwrap_err();
        assert!(
            error.to_string().contains("injected write failure"),
            "{error}"
        );
        assert_eq!(nodes.pending_bytes(), pending, "nothing was written");
        assert_eq!(nodes.get(&hash).await.unwrap().as_deref(), Some(&node));

        nodes.flush().await.unwrap();
        assert_eq!(nodes.pending_bytes(), 0);
        let txn = crate::corekv::Store::new_txn(&*regolith, true)
            .await
            .unwrap();
        let stored = txn.get(&node_key(&hash)).await.unwrap();
        txn.discard();
        assert_eq!(stored.as_deref(), Some(node.encode().as_slice()));
    })
    .unwrap()
}

#[test]
fn a_flush_dropped_before_its_commit_keeps_its_nodes_pending() {
    use std::future::Future as _;
    use std::task::{Context, Waker};

    workers::block_on(async {
        let regolith = Arc::new(RegolithStore::in_memory().unwrap());
        let store = Arc::new(FailFirstWrite {
            inner: Arc::clone(&regolith),
            failed: std::sync::atomic::AtomicBool::new(false),
            stall: true,
        });
        let mut nodes = ProllyNodes::new(store, engine_retry_options(5), usize::MAX);
        let node = prolly::Node {
            level: 0,
            items: vec![Item {
                key: b"k".to_vec(),
                value: b"v".to_vec(),
            }],
        };
        let hash = nodes.put(node.clone()).await.unwrap();
        let pending = nodes.pending_bytes();

        {
            let mut flush = std::pin::pin!(nodes.flush());
            let polled = flush.as_mut().poll(&mut Context::from_waker(Waker::noop()));
            assert!(polled.is_pending(), "the commit waits");
        }
        assert_eq!(nodes.pending_bytes(), pending, "nothing was written");
        assert_eq!(
            nodes.get(&hash).await.unwrap().as_deref(),
            Some(&node),
            "a dropped flush loses no put"
        );

        nodes.flush().await.unwrap();
        assert_eq!(nodes.pending_bytes(), 0);
        let txn = crate::corekv::Store::new_txn(&*regolith, true)
            .await
            .unwrap();
        let stored = txn.get(&node_key(&hash)).await.unwrap();
        txn.discard();
        assert_eq!(stored.as_deref(), Some(node.encode().as_slice()));
    })
    .unwrap()
}
