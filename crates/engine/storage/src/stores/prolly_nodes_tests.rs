use std::sync::Arc;

use prolly::{diff, tree::DEFAULT_TARGET, Cached, Item, MemStore, Store as _, Tree};

use super::{node_key, ProllyNodes};
use crate::backends::RegolithStore;
use crate::corekv::engine_retry_options;

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
