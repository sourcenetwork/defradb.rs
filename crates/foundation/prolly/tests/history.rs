//! History independence: however the writes are ordered, the same entries give
//! the same root as building the tree from them at once.

mod common;

use std::{collections::BTreeMap, num::NonZeroU32};

use common::{entries, run};
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

async fn built(store: &mut MemStore, target: NonZeroU32, model: &BTreeMap<u16, u8>) -> Tree {
    Tree::build(
        store,
        target,
        model.iter().map(|(k, v)| (key(*k), vec![*v])),
    )
    .await
    .unwrap()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn edits_reach_the_root_a_fresh_build_gives(
        target in 1u32..8,
        ops in prop::collection::vec(op(), 1..600),
    ) {
        run(async {
            let target = NonZeroU32::new(target).unwrap();
            let mut store = MemStore::new();
            let mut tree = Tree::empty(target);
            let mut model = BTreeMap::new();
            for (step, op) in ops.iter().enumerate() {
                match op {
                    Op::Insert(k, v) => {
                        tree.insert(&mut store, key(*k), vec![*v]).await.unwrap();
                        model.insert(*k, *v);
                    }
                    Op::Remove(k) => {
                        tree.remove(&mut store, &key(*k)).await.unwrap();
                        model.remove(k);
                    }
                }
                if step % 37 == 0 {
                    prop_assert_eq!(tree, built(&mut store, target, &model).await);
                }
            }
            prop_assert_eq!(tree, built(&mut store, target, &model).await);
            let read: Vec<(Vec<u8>, Vec<u8>)> = entries(&tree, &store)
                .await
                .unwrap()
                .into_iter()
                .map(|item| (item.key, item.value))
                .collect();
            let expected: Vec<(Vec<u8>, Vec<u8>)> =
                model.iter().map(|(k, v)| (key(*k), vec![*v])).collect();
            prop_assert_eq!(read, expected);
            Ok(())
        })?;
    }

}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(16))]

    /// Values up to 45 000 bytes make `MAX_CHUNK_BYTES` close chunks, so the
    /// byte cap's effect on edits, including a re-cut that runs to the end of a
    /// level, is checked against a fresh build. `u32::MAX` all but disables
    /// boundaries, leaving the cap as the only thing that closes a chunk.
    #[test]
    fn edits_with_large_values_reach_the_root_a_fresh_build_gives(
        target in prop_oneof![1u32..8, Just(u32::MAX)],
        ops in prop::collection::vec(
            prop_oneof![
                3 => (0u16..80, 0usize..45_000).prop_map(|(k, len)| (k, Some(len))),
                1 => (0u16..80).prop_map(|k| (k, None)),
            ],
            1..40,
        ),
    ) {
        run(async {
            let target = NonZeroU32::new(target).unwrap();
            let mut store = MemStore::new();
            let mut tree = Tree::empty(target);
            let mut model = BTreeMap::new();
            for (k, len) in ops {
                match len {
                    Some(len) => {
                        let value = vec![k as u8; len];
                        tree.insert(&mut store, key(k), value.clone()).await.unwrap();
                        model.insert(k, value);
                    }
                    None => {
                        tree.remove(&mut store, &key(k)).await.unwrap();
                        model.remove(&k);
                    }
                }
                let fresh = Tree::build(
                    &mut store,
                    target,
                    model.iter().map(|(k, v)| (key(*k), v.clone())),
                )
                .await
                .unwrap();
                prop_assert_eq!(tree, fresh);
            }
            Ok(())
        })?;
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn insert_order_does_not_change_the_root(
        entries in prop::collection::btree_map(any::<u16>(), any::<u8>(), 0..500)
            .prop_flat_map(|map| {
                let pairs: Vec<(u16, u8)> = map.into_iter().collect();
                (Just(pairs.clone()), Just(pairs).prop_shuffle())
            }),
    ) {
        let (sorted, shuffled) = entries;
        run(async {
            let target = NonZeroU32::new(4).unwrap();
            let mut store = MemStore::new();
            let mut forward = Tree::empty(target);
            for (k, v) in &sorted {
                forward.insert(&mut store, key(*k), vec![*v]).await.unwrap();
            }
            let mut other = Tree::empty(target);
            for (k, v) in &shuffled {
                other.insert(&mut store, key(*k), vec![*v]).await.unwrap();
            }
            let mut backward = Tree::empty(target);
            for (k, v) in sorted.iter().rev() {
                backward.insert(&mut store, key(*k), vec![*v]).await.unwrap();
            }
            prop_assert_eq!(forward, other);
            prop_assert_eq!(forward, backward);
            Ok(())
        })?;
    }
}
