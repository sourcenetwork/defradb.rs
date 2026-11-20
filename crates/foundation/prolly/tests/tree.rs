use std::{cell::Cell, num::NonZeroU32, sync::Arc};

use prolly::{tree::DEFAULT_TARGET, Builder, Item, MemStore, Node, Store, Tree};

fn entry(n: u32) -> (Vec<u8>, Vec<u8>) {
    (n.to_be_bytes().to_vec(), format!("heads-{n}").into_bytes())
}

#[test]
fn an_empty_tree_has_no_root() {
    let mut store = MemStore::new();
    let tree = Tree::build(&mut store, DEFAULT_TARGET, Vec::new()).unwrap();
    assert_eq!(tree.root(), None);
    assert_eq!(tree.get(&store, b"a").unwrap(), None);
    assert!(tree
        .iter(&store)
        .collect::<prolly::Result<Vec<_>>>()
        .unwrap()
        .is_empty());
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
        .iter(&store)
        .collect::<prolly::Result<Vec<_>>>()
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
        tree.iter(&store)
            .collect::<prolly::Result<Vec<_>>>()
            .unwrap(),
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
    assert_eq!(
        tree.iter(&store)
            .collect::<prolly::Result<Vec<_>>>()
            .unwrap()
            .len(),
        300
    );
}

#[test]
fn a_build_holds_one_open_chunk_per_level_not_the_whole_set() {
    let mut store = MemStore::new();
    let mut builder = Builder::new(DEFAULT_TARGET);
    let mut peak = 0;
    for n in 0..100_000 {
        let (key, value) = entry(n);
        builder.push(&mut store, key, value).unwrap();
        peak = peak.max(builder.held());
    }
    let tree = builder.finish(&mut store).unwrap();
    let levels = usize::from(store.get(&tree.root().unwrap()).unwrap().level) + 1;
    assert!(peak < 1_000, "held {peak} of 100 000 entries");
    assert!(peak <= levels * 400, "held {peak} over {levels} levels");
}

#[test]
fn a_build_refuses_a_key_out_of_order() {
    let mut store = MemStore::new();
    let pairs = vec![entry(2), entry(1)];
    assert_eq!(
        Tree::build(&mut store, DEFAULT_TARGET, pairs),
        Err(prolly::Error::Unsorted)
    );
}

/// A store that counts the nodes read through it.
struct Counting<'a> {
    inner: &'a MemStore,
    reads: Cell<usize>,
}

impl Store for Counting<'_> {
    fn get(&self, hash: &prolly::Hash) -> Option<Arc<Node>> {
        self.reads.set(self.reads.get() + 1);
        self.inner.get(hash)
    }

    fn put(&mut self, _node: Node) -> prolly::Hash {
        unreachable!("reading only")
    }
}

#[test]
fn reading_entries_holds_one_node_per_level_and_reads_each_once() {
    let mut store = MemStore::new();
    let tree = Tree::build(&mut store, DEFAULT_TARGET, (0..20_000).map(entry)).unwrap();
    let height = usize::from(store.get(&tree.root().unwrap()).unwrap().level) + 1;
    let counting = Counting {
        inner: &store,
        reads: Cell::new(0),
    };
    let mut entries = tree.iter(&counting);
    assert_eq!(entries.next().unwrap().unwrap().key, entry(0).0);
    assert_eq!(
        counting.reads.get(),
        height,
        "the first entry reads one path"
    );
    let mut count = 1;
    while let Some(item) = entries.next() {
        item.unwrap();
        count += 1;
        assert!(entries.held() <= height);
    }
    assert_eq!(count, 20_000);
    assert_eq!(counting.reads.get(), store.len(), "every node read once");
}
