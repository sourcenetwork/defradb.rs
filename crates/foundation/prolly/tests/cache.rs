//! A tree far larger than the cache budget reads the same through the cache,
//! and the cache never holds more than its budget.

mod common;

use common::{entries, run};
use prolly::{diff, tree::DEFAULT_TARGET, Cached, MemStore, Tree};

fn entry(n: u32) -> (Vec<u8>, Vec<u8>) {
    (n.to_be_bytes().to_vec(), format!("heads-{n}").into_bytes())
}

const BUDGET: usize = 64 * 1024;

#[test]
fn a_tree_far_larger_than_the_budget_reads_the_same_and_stays_under_it() {
    run(async {
        let mut plain = MemStore::new();
        let mut cached = Cached::new(MemStore::new(), BUDGET);
        let tree = Tree::build(&mut plain, DEFAULT_TARGET, (0..50_000).map(entry))
            .await
            .unwrap();
        let built = Tree::build(&mut cached, DEFAULT_TARGET, (0..50_000).map(entry))
            .await
            .unwrap();
        assert_eq!(built, tree);

        let mut edited = tree;
        let mut through = built;
        for n in [3, 25_000, 49_999] {
            edited.remove(&mut plain, &entry(n).0).await.unwrap();
            through.remove(&mut cached, &entry(n).0).await.unwrap();
        }
        assert_eq!(through, edited);

        for n in (0..50_000).step_by(97) {
            let key = entry(n).0;
            assert_eq!(
                through.get(&cached, &key).await.unwrap(),
                edited.get(&plain, &key).await.unwrap()
            );
        }
        assert_eq!(
            entries(&through, &cached).await.unwrap(),
            entries(&edited, &plain).await.unwrap()
        );
        assert_eq!(
            diff(&cached, &built, &cached, &through).await.unwrap(),
            diff(&plain, &tree, &plain, &edited).await.unwrap()
        );

        let size: usize = entries(&tree, &plain)
            .await
            .unwrap()
            .iter()
            .map(|item| item.encoded_len())
            .sum();
        let stats = cached.stats();
        assert!(
            size > 20 * BUDGET,
            "the tree ({size} bytes) dwarfs the budget"
        );
        assert!(
            stats.peak <= BUDGET,
            "held {} of a {BUDGET}-byte budget",
            stats.peak
        );
        assert!(
            stats.evictions > 0 && stats.hits > 0 && stats.misses > 0,
            "{stats:?}"
        );
    })
}

#[test]
fn repeated_reads_of_a_small_tree_hit_the_cache() {
    run(async {
        let mut cached = Cached::new(MemStore::new(), BUDGET);
        let tree = Tree::build(&mut cached, DEFAULT_TARGET, (0..500).map(entry))
            .await
            .unwrap();
        for _ in 0..3 {
            for n in 0..500 {
                assert_eq!(
                    tree.get(&cached, &entry(n).0).await.unwrap(),
                    Some(entry(n).1)
                );
            }
        }
        let stats = cached.stats();
        assert_eq!(stats.evictions, 0, "{stats:?}");
        assert!(stats.misses < 40 && stats.hits > 2_900, "{stats:?}");
    })
}
