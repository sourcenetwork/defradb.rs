use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use prolly::{diff, tree::DEFAULT_TARGET, Cached, Hash, Item, MemStore, Tree};

use super::{read_roots, LiveRoots, LiveRootsFuture, ProllyNodes, ReclaimPage, NODES_PER_SWEEP};
use crate::backends::RegolithStore;
use crate::corekv::Dropable as _;
use crate::corekv::{engine_retry_options, RetryTxn, Store};

/// The cache's budget: far below the tree's size.
const BUDGET: usize = 64 * 1024;

pub(super) fn entry(n: u32) -> (Vec<u8>, Vec<u8>) {
    (n.to_be_bytes().to_vec(), format!("heads-{n}").into_bytes())
}

pub(super) async fn entries<S>(tree: &Tree, store: &S) -> Vec<Item>
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
    let nodes = ProllyNodes::new(Arc::clone(store), engine_retry_options(5));
    Cached::new(nodes, BUDGET)
}

pub(super) async fn node_keys(store: &RegolithStore) -> Vec<Vec<u8>> {
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

pub(super) async fn publish<S>(nodes: &mut ProllyNodes<S>, roots: &[Hash])
where
    S: Store + 'static,
{
    nodes.publish(roots).await.unwrap();
}

/// The roots under `ROOTS_KEY`, with a writer that publishes once, right
/// after the sweep's first read of them.
struct PublishedAfterTheRead {
    store: Arc<RegolithStore>,
    root: Hash,
    reads: AtomicUsize,
}

impl LiveRoots for PublishedAfterTheRead {
    fn read<'txn>(&'txn self, txn: &'txn mut dyn RetryTxn) -> LiveRootsFuture<'txn> {
        Box::pin(async move {
            let roots = read_roots(txn).await?;
            if self.reads.fetch_add(1, Ordering::SeqCst) == 0 {
                let mut writer = ProllyNodes::new(Arc::clone(&self.store), engine_retry_options(5));
                publish(&mut writer, &[self.root]).await;
            }
            Ok(roots)
        })
    }
}

pub(super) async fn sweep(store: &RegolithStore) -> ReclaimPage {
    ProllyNodes::reclaim(store, None, NODES_PER_SWEEP)
        .await
        .unwrap()
}

#[test]
fn a_reclaim_keeps_only_the_nodes_the_live_roots_name() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let mut regolith = ProllyNodes::new(Arc::clone(&store), engine_retry_options(5));
        let built = Tree::build(&mut regolith, DEFAULT_TARGET, (0..3).map(entry))
            .await
            .unwrap();
        publish(&mut regolith, &[built.root().unwrap()]).await;
        let stored = node_keys(&store).await.len();
        let report = sweep(&store).await;
        assert_eq!(report.scanned, stored);
        assert_eq!(report.marked, stored);
        assert_eq!(report.deleted, 0);
        assert_eq!(report.next_start, None, "the page reached the end of `t`");

        let mut edited_tree = built;
        edited_tree
            .remove(&mut regolith, &entry(1).0)
            .await
            .unwrap();
        publish(&mut regolith, &[edited_tree.root().unwrap()]).await;
        let report = sweep(&store).await;
        assert_eq!(report.deleted, 1);
        assert_eq!(node_keys(&store).await.len(), report.marked);
        assert_eq!(entries(&edited_tree, &regolith).await.len(), 2);
    })
    .unwrap()
}

#[test]
fn a_multi_level_tree_reclaims_only_the_nodes_the_live_root_reaches() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let mut regolith = ProllyNodes::new(Arc::clone(&store), engine_retry_options(5));
        let mut tree = Tree::build(&mut regolith, DEFAULT_TARGET, (0..5_000).map(entry))
            .await
            .unwrap();
        publish(&mut regolith, &[tree.root().unwrap()]).await;
        let built = node_keys(&store).await.len();
        tree.remove(&mut regolith, &entry(2_499).0).await.unwrap();
        publish(&mut regolith, &[tree.root().unwrap()]).await;

        let report = sweep(&store).await;
        assert!(report.marked > 2, "the root has children: {report:?}");
        assert!(report.deleted > 0, "the edit left old nodes: {report:?}");
        assert_eq!(report.deleted, report.scanned - report.marked);
        assert!(report.marked <= built);
        assert_eq!(node_keys(&store).await.len(), report.marked);
        assert_eq!(entries(&tree, &regolith).await.len(), 4_999);
    })
    .unwrap()
}

#[test]
fn a_root_published_while_a_page_is_swept_keeps_the_old_nodes_it_names() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let mut regolith = ProllyNodes::new(Arc::clone(&store), engine_retry_options(5));
        let first = Tree::build(&mut regolith, DEFAULT_TARGET, (0..5_000).map(entry))
            .await
            .unwrap();
        publish(&mut regolith, &[first.root().unwrap()]).await;
        let mut tree = first;
        tree.remove(&mut regolith, &entry(2_499).0).await.unwrap();
        let edited = tree;
        publish(&mut regolith, &[edited.root().unwrap()]).await;

        // The writer brings the removed entry back: the tree is the first
        // one again, and the nodes only it names sit unmarked on the page
        // until its root is published.
        let (key, value) = entry(2_499);
        tree.insert(&mut regolith, key, value).await.unwrap();
        assert_eq!(tree, first);
        let roots = Arc::new(PublishedAfterTheRead {
            store: Arc::clone(&store),
            root: first.root().unwrap(),
            reads: AtomicUsize::new(0),
        });

        let report =
            ProllyNodes::reclaim_with_roots(&*store, Arc::clone(&roots), None, NODES_PER_SWEEP)
                .await
                .unwrap();
        assert_eq!(
            roots.reads.load(Ordering::SeqCst),
            2,
            "the publication re-ran the delete"
        );
        assert!(report.deleted > 0, "the edited tree's own nodes go");
        assert_eq!(node_keys(&store).await.len(), report.marked);
        let fresh = ProllyNodes::new(Arc::clone(&store), engine_retry_options(5));
        assert_eq!(entries(&first, &fresh).await.len(), 5_000);
    })
    .unwrap()
}

#[test]
fn a_sweep_walks_one_bounded_page_at_a_time() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let mut regolith = ProllyNodes::new(Arc::clone(&store), engine_retry_options(5));
        let tree = Tree::build(&mut regolith, DEFAULT_TARGET, (0..5_000).map(entry))
            .await
            .unwrap();
        publish(&mut regolith, &[tree.root().unwrap()]).await;
        let total = node_keys(&store).await.len();
        let limit = total / 3;
        let mut start = None;
        let mut scanned = 0;
        let mut pages = 0;
        loop {
            let report = ProllyNodes::reclaim(&*store, start, limit).await.unwrap();
            assert!(report.scanned <= limit);
            assert_eq!(report.deleted, 0);
            scanned += report.scanned;
            pages += 1;
            start = report.next_start;
            if start.is_none() {
                break;
            }
        }
        assert_eq!(scanned, total);
        assert!(pages >= 3, "{pages} pages of {limit} over {total} keys");
    })
    .unwrap()
}

#[test]
fn a_sweep_with_no_live_roots_reclaims_every_node_page_by_page() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let mut regolith = ProllyNodes::new(Arc::clone(&store), engine_retry_options(5));
        let tree = Tree::build(&mut regolith, DEFAULT_TARGET, (0..5_000).map(entry))
            .await
            .unwrap();
        publish(&mut regolith, &[tree.root().unwrap()]).await;
        publish(&mut regolith, &[]).await;
        let total = node_keys(&store).await.len();
        let limit = total / 3;
        let mut start = None;
        let mut deleted = 0;
        loop {
            let report = ProllyNodes::reclaim(&*store, start, limit).await.unwrap();
            assert!(report.scanned <= limit);
            deleted += report.deleted;
            start = report.next_start;
            if start.is_none() {
                break;
            }
        }
        assert_eq!(deleted, total);
        assert!(node_keys(&store).await.is_empty());
    })
    .unwrap()
}

#[test]
fn a_purge_leaves_no_t_key() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let mut regolith = ProllyNodes::new(Arc::clone(&store), engine_retry_options(5));
        let tree = Tree::build(&mut regolith, DEFAULT_TARGET, (0..2).map(entry))
            .await
            .unwrap();
        publish(&mut regolith, &[tree.root().unwrap()]).await;
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
        assert!(
            regolith.inner().pending_bytes() > 0,
            "a build waits for its root"
        );
        publish(regolith.inner_mut(), &[stored.root().unwrap()]).await;
        assert_eq!(regolith.inner().pending_bytes(), 0);

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
        publish(regolith.inner_mut(), &[stored.root().unwrap()]).await;

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

/// The write path: a build and single-entry edits, each followed by the
/// publication of its root, in regolith behind the cache. Run with
/// `--run-ignored only`.
#[test]
#[ignore]
fn bench_build_and_edit_then_publish() {
    const BUILD: u32 = 200_000;
    const EDITS: u32 = 2_000;
    workers::block_on(async {
        for _ in 0..15 {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let mut regolith = nodes(&store);
            let start = web_time::Instant::now();
            let mut tree = Tree::build(&mut regolith, DEFAULT_TARGET, (0..BUILD).map(entry))
                .await
                .unwrap();
            publish(regolith.inner_mut(), &[tree.root().unwrap()]).await;
            let build = start.elapsed();
            let start = web_time::Instant::now();
            for n in 0..EDITS {
                let (key, value) = entry(BUILD + n * 7);
                tree.insert(&mut regolith, key, value).await.unwrap();
                publish(regolith.inner_mut(), &[tree.root().unwrap()]).await;
            }
            let edits = start.elapsed();
            println!(
                "BENCH build+publish {:.0} ns/entry, edit+publish {:.0} ns/edit",
                build.as_nanos() as f64 / f64::from(BUILD),
                edits.as_nanos() as f64 / f64::from(EDITS)
            );
        }
    })
    .unwrap()
}
