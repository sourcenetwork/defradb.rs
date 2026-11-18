use std::num::NonZeroU32;

use prolly::{tree::DEFAULT_TARGET, Item, MemStore, Store, Tree};

fn entry(n: u32) -> (Vec<u8>, Vec<u8>) {
    (n.to_be_bytes().to_vec(), format!("heads-{n}").into_bytes())
}

#[test]
fn an_empty_tree_has_no_root() {
    let mut store = MemStore::new();
    let tree = Tree::build(&mut store, DEFAULT_TARGET, Vec::new()).unwrap();
    assert_eq!(tree.root(), None);
    assert_eq!(tree.get(&store, b"a").unwrap(), None);
    assert!(tree.entries(&store).unwrap().is_empty());
}

#[test]
fn every_entry_reads_back_and_absent_keys_do_not() {
    let mut store = MemStore::new();
    let tree = Tree::build(&mut store, DEFAULT_TARGET, (0..5_000).map(|n| entry(n * 2))).unwrap();
    for n in 0..5_000 {
        let (key, value) = entry(n * 2);
        assert_eq!(tree.get(&store, &key).unwrap(), Some(value));
        assert_eq!(tree.get(&store, &entry(n * 2 + 1).0).unwrap(), None);
    }
    let keys: Vec<Vec<u8>> = tree
        .entries(&store)
        .unwrap()
        .into_iter()
        .map(|item| item.key)
        .collect();
    assert_eq!(keys, (0..5_000).map(|n| entry(n * 2).0).collect::<Vec<_>>());
}

#[test]
fn a_large_tree_has_levels_and_chunks_near_the_target() {
    let mut store = MemStore::new();
    let tree = Tree::build(&mut store, DEFAULT_TARGET, (0..20_000).map(entry)).unwrap();
    let root = store.get(&tree.root().unwrap()).unwrap();
    assert!(
        root.level >= 2,
        "20 000 entries at 32 per chunk need levels"
    );
    let leaves = store.len() as f64;
    let mean = 20_000.0 / leaves;
    assert!((16.0..64.0).contains(&mean), "mean items per node {mean}");
}

#[test]
fn the_last_value_for_a_repeated_key_wins() {
    let mut store = MemStore::new();
    let pairs = vec![
        (b"k".to_vec(), b"old".to_vec()),
        (b"k".to_vec(), b"new".to_vec()),
    ];
    let tree = Tree::build(&mut store, DEFAULT_TARGET, pairs).unwrap();
    assert_eq!(
        tree.entries(&store).unwrap(),
        vec![Item {
            key: b"k".to_vec(),
            value: b"new".to_vec()
        }]
    );
}

#[test]
fn removing_every_entry_empties_the_tree() {
    let mut store = MemStore::new();
    let target = NonZeroU32::new(4).unwrap();
    let mut tree = Tree::build(&mut store, target, (0..500).map(entry)).unwrap();
    for n in 0..500 {
        tree.remove(&mut store, &entry(n).0).unwrap();
    }
    assert_eq!(tree, Tree::empty(target));
}

#[test]
fn a_target_of_one_still_builds_a_tree() {
    let mut store = MemStore::new();
    let target = NonZeroU32::new(1).unwrap();
    let tree = Tree::build(&mut store, target, (0..300).map(entry)).unwrap();
    assert_eq!(tree.entries(&store).unwrap().len(), 300);
}
