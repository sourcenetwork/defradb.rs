//! History independence: however the writes are ordered, the same entries give
//! the same root as building the tree from them at once.

use std::{collections::BTreeMap, num::NonZeroU32};

use prolly::{MemStore, Tree};
use proptest::prelude::*;

/// One write: set a key, or remove it.
#[derive(Clone, Debug)]
enum Op {
    Insert(u16, u8),
    Remove(u16),
}

fn key(k: u16) -> Vec<u8> {
    k.to_be_bytes().to_vec()
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        3 => (0u16..400, any::<u8>()).prop_map(|(k, v)| Op::Insert(k, v)),
        1 => (0u16..400).prop_map(Op::Remove),
    ]
}

fn built(store: &mut MemStore, target: NonZeroU32, model: &BTreeMap<u16, u8>) -> Tree {
    Tree::build(
        store,
        target,
        model.iter().map(|(k, v)| (key(*k), vec![*v])),
    )
    .unwrap()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn edits_reach_the_root_a_fresh_build_gives(
        target in 1u32..8,
        ops in prop::collection::vec(op(), 1..600),
    ) {
        let target = NonZeroU32::new(target).unwrap();
        let mut store = MemStore::new();
        let mut tree = Tree::empty(target);
        let mut model = BTreeMap::new();
        for (step, op) in ops.iter().enumerate() {
            match op {
                Op::Insert(k, v) => {
                    tree.insert(&mut store, key(*k), vec![*v]).unwrap();
                    model.insert(*k, *v);
                }
                Op::Remove(k) => {
                    tree.remove(&mut store, &key(*k)).unwrap();
                    model.remove(k);
                }
            }
            if step % 37 == 0 {
                prop_assert_eq!(tree, built(&mut store, target, &model));
            }
        }
        prop_assert_eq!(tree, built(&mut store, target, &model));
        let entries: Vec<(Vec<u8>, Vec<u8>)> = tree
            .entries(&store)
            .unwrap()
            .into_iter()
            .map(|item| (item.key, item.value))
            .collect();
        let expected: Vec<(Vec<u8>, Vec<u8>)> = model.iter().map(|(k, v)| (key(*k), vec![*v])).collect();
        prop_assert_eq!(entries, expected);
    }

    #[test]
    fn insert_order_does_not_change_the_root(
        entries in prop::collection::btree_map(any::<u16>(), any::<u8>(), 0..500)
            .prop_flat_map(|map| {
                let pairs: Vec<(u16, u8)> = map.into_iter().collect();
                (Just(pairs.clone()), Just(pairs).prop_shuffle())
            }),
    ) {
        let (sorted, shuffled) = entries;
        let target = NonZeroU32::new(4).unwrap();
        let mut store = MemStore::new();
        let mut forward = Tree::empty(target);
        for (k, v) in &sorted {
            forward.insert(&mut store, key(*k), vec![*v]).unwrap();
        }
        let mut other = Tree::empty(target);
        for (k, v) in &shuffled {
            other.insert(&mut store, key(*k), vec![*v]).unwrap();
        }
        let mut backward = Tree::empty(target);
        for (k, v) in sorted.iter().rev() {
            backward.insert(&mut store, key(*k), vec![*v]).unwrap();
        }
        prop_assert_eq!(forward, other);
        prop_assert_eq!(forward, backward);
    }
}
