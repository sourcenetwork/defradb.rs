mod common;

use std::{collections::BTreeMap, num::NonZeroU32};

use common::{run, Counting};
use prolly::{diff, tree::DEFAULT_TARGET, Difference, MemStore, Tree};
use proptest::prelude::*;

fn key(k: u16) -> Vec<u8> {
    k.to_be_bytes().to_vec()
}

/// The differences found by comparing every key, the answer `diff` must give.
fn compare(left: &BTreeMap<u16, u8>, right: &BTreeMap<u16, u8>) -> Vec<Difference> {
    let keys: std::collections::BTreeSet<u16> = left.keys().chain(right.keys()).copied().collect();
    keys.into_iter()
        .filter(|k| left.get(k) != right.get(k))
        .map(|k| Difference {
            key: key(k),
            left: left.get(&k).map(|v| vec![*v]),
            right: right.get(&k).map(|v| vec![*v]),
        })
        .collect()
}

async fn build(store: &mut MemStore, target: NonZeroU32, map: &BTreeMap<u16, u8>) -> Tree {
    Tree::build(store, target, map.iter().map(|(k, v)| (key(*k), vec![*v])))
        .await
        .unwrap()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn diff_finds_exactly_the_keys_that_differ(
        target in 2u32..8,
        left in prop::collection::btree_map(0u16..300, 0u8..4, 0..300),
        right in prop::collection::btree_map(0u16..300, 0u8..4, 0..300),
    ) {
        run(async {
            let target = NonZeroU32::new(target).unwrap();
            let mut left_store = MemStore::new();
            let mut right_store = MemStore::new();
            let lhs = build(&mut left_store, target, &left).await;
            let rhs = build(&mut right_store, target, &right).await;
            let found = diff(&left_store, &lhs, &right_store, &rhs).await.unwrap();
            prop_assert_eq!(found, compare(&left, &right));
            Ok(())
        })?;
    }

    #[test]
    fn diff_after_a_few_edits_finds_those_edits(
        base in prop::collection::btree_map(0u16..2000, 0u8..4, 500..1000),
        edits in prop::collection::btree_map(0u16..2000, prop::option::of(0u8..4), 1..5),
    ) {
        run(async {
            let mut store = MemStore::new();
            let before = build(&mut store, DEFAULT_TARGET, &base).await;
            let mut after = before;
            let mut changed = base.clone();
            for (k, v) in &edits {
                match v {
                    Some(v) => {
                        after.insert(&mut store, key(*k), vec![*v]).await.unwrap();
                        changed.insert(*k, *v);
                    }
                    None => {
                        after.remove(&mut store, &key(*k)).await.unwrap();
                        changed.remove(k);
                    }
                }
            }
            let found = diff(&store, &before, &store, &after).await.unwrap();
            prop_assert_eq!(found, compare(&base, &changed));
            Ok(())
        })?;
    }
}

#[test]
fn unequal_roots_are_each_read_once() {
    let mut store = MemStore::new();
    let left = build(
        &mut store,
        DEFAULT_TARGET,
        &(0..4).map(|k| (k, 0)).collect(),
    );
    let right = build(
        &mut store,
        DEFAULT_TARGET,
        &(0..4).map(|k| (k, 1)).collect(),
    );
    assert_eq!(store.get(&left.root().unwrap()).unwrap().level, 0);
    assert_eq!(store.get(&right.root().unwrap()).unwrap().level, 0);
    let counting = Counting {
        inner: &store,
        reads: std::cell::Cell::new(0),
    };
    assert_eq!(diff(&counting, &left, &counting, &right).unwrap().len(), 4);
    assert_eq!(counting.reads.get(), 2, "one read per root");
}

#[test]
fn equal_trees_differ_nowhere_and_read_nothing() {
    run(async {
        let mut store = MemStore::new();
        let map: BTreeMap<u16, u8> = (0..5_000).map(|k| (k, (k % 7) as u8)).collect();
        let tree = build(&mut store, DEFAULT_TARGET, &map).await;
        let counting = Counting::new(&store);
        assert!(diff(&counting, &tree, &counting, &tree)
            .await
            .unwrap()
            .is_empty());
        assert_eq!(counting.reads.get(), 0);
    })
}

#[test]
fn one_changed_entry_reads_a_path_not_the_tree() {
    run(async {
        let mut store = MemStore::new();
        let map: BTreeMap<u16, u8> = (0..20_000).map(|k| (k, 0)).collect();
        let before = build(&mut store, DEFAULT_TARGET, &map).await;
        let mut after = before;
        after
            .insert(&mut store, key(12_345), vec![1])
            .await
            .unwrap();
        let counting = Counting::new(&store);
        let found = diff(&counting, &before, &counting, &after).await.unwrap();
        assert_eq!(
            found,
            vec![Difference {
                key: key(12_345),
                left: Some(vec![0]),
                right: Some(vec![1])
            }]
        );
        let reads = counting.reads.get();
        assert!(
            reads < 40,
            "read {reads} nodes of a {}-node tree",
            store.len()
        );
    })
}
