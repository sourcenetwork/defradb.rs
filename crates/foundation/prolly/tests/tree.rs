mod common;

use std::{
    cell::Cell,
    future::Future,
    num::NonZeroU32,
    sync::Arc,
    task::{Context, Poll, Waker},
};

use common::{entries, run, Counting};
use prolly::{tree::DEFAULT_TARGET, Builder, Item, MemStore, Node, Store, Tree, MAX_CHUNK_BYTES};

fn entry(n: u32) -> (Vec<u8>, Vec<u8>) {
    (n.to_be_bytes().to_vec(), format!("heads-{n}").into_bytes())
}

async fn level_of(store: &MemStore, tree: &Tree) -> usize {
    let root = store.get(&tree.root().unwrap()).await.unwrap().unwrap();
    usize::from(root.level)
}

#[test]
fn an_empty_tree_has_no_root() {
    run(async {
        let mut store = MemStore::new();
        let tree = Tree::build(&mut store, DEFAULT_TARGET, Vec::new())
            .await
            .unwrap();
        assert_eq!(tree.root(), None);
        assert_eq!(tree.get(&store, b"a").await.unwrap(), None);
        assert!(entries(&tree, &store).await.unwrap().is_empty());
    })
}

#[test]
fn every_entry_reads_back_and_absent_keys_do_not() {
    run(async {
        let mut store = MemStore::new();
        let tree = Tree::build(&mut store, DEFAULT_TARGET, (0..5_000).map(|n| entry(n * 2)))
            .await
            .unwrap();
        for n in 0..5_000 {
            let (key, value) = entry(n * 2);
            assert_eq!(tree.get(&store, &key).await.unwrap(), Some(value));
            assert_eq!(tree.get(&store, &entry(n * 2 + 1).0).await.unwrap(), None);
        }
        let keys: Vec<Vec<u8>> = entries(&tree, &store)
            .await
            .unwrap()
            .into_iter()
            .map(|item| item.key)
            .collect();
        assert_eq!(keys, (0..5_000).map(|n| entry(n * 2).0).collect::<Vec<_>>());
    })
}

#[test]
fn a_large_tree_has_levels_and_chunks_near_the_target() {
    run(async {
        let mut store = MemStore::new();
        let tree = Tree::build(&mut store, DEFAULT_TARGET, (0..20_000).map(entry))
            .await
            .unwrap();
        assert!(
            level_of(&store, &tree).await >= 2,
            "20 000 entries at 32 per chunk need levels"
        );
        let mean = 20_000.0 / store.len() as f64;
        assert!((16.0..64.0).contains(&mean), "mean items per node {mean}");
    })
}

#[test]
fn the_last_value_for_a_repeated_key_wins() {
    run(async {
        let mut store = MemStore::new();
        let pairs = vec![
            (b"k".to_vec(), b"old".to_vec()),
            (b"k".to_vec(), b"new".to_vec()),
        ];
        let tree = Tree::build(&mut store, DEFAULT_TARGET, pairs)
            .await
            .unwrap();
        assert_eq!(
            entries(&tree, &store).await.unwrap(),
            vec![Item {
                key: b"k".to_vec(),
                value: b"new".to_vec()
            }]
        );
    })
}

#[test]
fn removing_every_entry_empties_the_tree() {
    run(async {
        let mut store = MemStore::new();
        let target = NonZeroU32::new(4).unwrap();
        let mut tree = Tree::build(&mut store, target, (0..500).map(entry))
            .await
            .unwrap();
        for n in 0..500 {
            tree.remove(&mut store, &entry(n).0).await.unwrap();
        }
        assert_eq!(tree, Tree::empty(target));
    })
}

#[test]
fn a_target_of_one_still_builds_a_tree() {
    run(async {
        let mut store = MemStore::new();
        let target = NonZeroU32::new(1).unwrap();
        let tree = Tree::build(&mut store, target, (0..300).map(entry))
            .await
            .unwrap();
        assert_eq!(entries(&tree, &store).await.unwrap().len(), 300);
    })
}

#[test]
fn a_build_holds_one_open_chunk_per_level_not_the_whole_set() {
    let (peak, levels) = run(build_peak(DEFAULT_TARGET, 100_000));
    assert!(peak < 32 * 1024, "held {peak} bytes of 100 000 entries");
    assert!(
        peak <= levels * 400 * 32,
        "held {peak} bytes over {levels} levels"
    );
}

/// Builds `count` entries through a [`Builder`], returning the most bytes it
/// held at once and the tree's height.
async fn build_peak(target: NonZeroU32, count: u32) -> (usize, usize) {
    let mut store = MemStore::new();
    let mut builder = Builder::new(target);
    let mut peak = 0;
    for n in 0..count {
        let (key, value) = entry(n);
        builder.push(&mut store, key, value).await.unwrap();
        peak = peak.max(builder.held_bytes());
    }
    let tree = builder.finish(&mut store).await.unwrap();
    (peak, level_of(&store, &tree).await + 1)
}

#[test]
fn without_boundaries_chunks_close_at_the_byte_cap() {
    run(async {
        // A target this large makes a boundary about one item in 4 billion, so
        // only the cap closes chunks.
        let target = NonZeroU32::MAX;
        let (peak, levels) = build_peak(target, 100_000).await;
        let total: usize = (0..100_000)
            .map(|n| {
                let (key, value) = entry(n);
                16 + key.len() + value.len()
            })
            .sum();
        assert!(levels >= 2, "the cap cuts the leaves into chunks");
        assert!(
            peak <= levels * MAX_CHUNK_BYTES,
            "held {peak} bytes over {levels} levels"
        );
        assert!(peak * 10 < total, "held {peak} of {total} bytes");

        let mut store = MemStore::new();
        let mut edited = Tree::build(&mut store, target, (0..100_000).map(entry))
            .await
            .unwrap();
        let mut model: Vec<(Vec<u8>, Vec<u8>)> = (0..100_000).map(entry).collect();
        for n in [7, 50_000, 99_999] {
            edited.remove(&mut store, &entry(n).0).await.unwrap();
            model.retain(|(key, _)| *key != entry(n).0);
        }
        let fresh = Tree::build(&mut MemStore::new(), target, model)
            .await
            .unwrap();
        assert_eq!(edited, fresh);
        assert_eq!(entries(&edited, &store).await.unwrap().len(), 100_000 - 3);
    })
}

#[test]
fn every_stored_node_stays_near_the_byte_cap() {
    /// A store that records the largest node encoding put into it.
    #[derive(Default)]
    struct Largest {
        inner: MemStore,
        largest: usize,
    }

    impl Store for Largest {
        async fn get(&self, hash: &prolly::Hash) -> prolly::Result<Option<Arc<Node>>> {
            self.inner.get(hash).await
        }

        async fn put(&mut self, node: Node) -> prolly::Result<prolly::Hash> {
            self.largest = self.largest.max(node.encode().len());
            self.inner.put(node).await
        }

        async fn flush(&mut self) -> prolly::Result<()> {
            Ok(())
        }
    }

    run(async {
        let mut store = Largest::default();
        Tree::build(&mut store, NonZeroU32::MAX, (0..100_000).map(entry))
            .await
            .unwrap();
        // The cap, plus the item that reached it and the node header.
        let largest = store.largest;
        assert!(
            largest <= 9 + MAX_CHUNK_BYTES + 64,
            "a node of {largest} bytes"
        );
        assert!(largest >= MAX_CHUNK_BYTES, "the cap closed the chunks");
    })
}

#[test]
fn a_build_refuses_a_key_out_of_order() {
    run(async {
        let mut store = MemStore::new();
        let pairs = vec![entry(2), entry(1)];
        assert_eq!(
            Tree::build(&mut store, DEFAULT_TARGET, pairs).await,
            Err(prolly::Error::Unsorted)
        );
    })
}

#[test]
fn reading_entries_holds_one_node_per_level_and_reads_each_once() {
    run(async {
        let mut store = MemStore::new();
        let tree = Tree::build(&mut store, DEFAULT_TARGET, (0..20_000).map(entry))
            .await
            .unwrap();
        let height = level_of(&store, &tree).await + 1;
        let counting = Counting::new(&store);
        let mut cursor = tree.iter(&counting);
        assert_eq!(cursor.next().await.unwrap().unwrap().key, entry(0).0);
        assert_eq!(
            counting.reads.get(),
            height,
            "the first entry reads one path"
        );
        let mut count = 1;
        while let Some(item) = cursor.next().await {
            item.unwrap();
            count += 1;
            assert!(cursor.held() <= height);
        }
        assert_eq!(count, 20_000);
        assert_eq!(counting.reads.get(), store.len(), "every node read once");
    })
}

/// A store whose first `put` fails.
struct FailFirstPut {
    inner: MemStore,
    failed: bool,
}

impl Store for FailFirstPut {
    async fn get(&self, hash: &prolly::Hash) -> prolly::Result<Option<Arc<Node>>> {
        self.inner.get(hash).await
    }

    async fn put(&mut self, node: Node) -> prolly::Result<prolly::Hash> {
        if !self.failed {
            self.failed = true;
            return Err(prolly::Error::Store("injected".into()));
        }
        self.inner.put(node).await
    }

    async fn flush(&mut self) -> prolly::Result<()> {
        self.inner.flush().await
    }
}

#[test]
fn a_build_refuses_to_finish_after_a_failed_put() {
    run(async {
        let mut store = FailFirstPut {
            inner: MemStore::new(),
            failed: false,
        };
        let mut builder = Builder::new(NonZeroU32::new(1).unwrap());
        let mut n = 0;
        let error = loop {
            let (key, value) = entry(n);
            n += 1;
            if let Err(error) = builder.push(&mut store, key, value).await {
                break error;
            }
        };
        assert_eq!(error, prolly::Error::Store("injected".into()));
        let (key, value) = entry(n);
        assert_eq!(
            builder.push(&mut store, key, value).await,
            Err(prolly::Error::BuildFailed)
        );
        assert_eq!(
            builder.finish(&mut store).await,
            Err(prolly::Error::BuildFailed),
            "a tree from this build would miss the chunk whose put failed"
        );
    })
}

/// Polls `future` once, then drops it: a caller that gave up while it waited.
fn poll_once<F>(future: F) -> Poll<F::Output>
where
    F: Future,
{
    let mut future = std::pin::pin!(future);
    future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
}

/// A store whose read number `stall_at` waits forever the first time.
struct StallRead<'a> {
    inner: &'a MemStore,
    reads: Cell<usize>,
    stall_at: usize,
}

impl Store for StallRead<'_> {
    async fn get(&self, hash: &prolly::Hash) -> prolly::Result<Option<Arc<Node>>> {
        let read = self.reads.get();
        self.reads.set(read + 1);
        if read == self.stall_at {
            std::future::pending::<()>().await;
        }
        self.inner.get(hash).await
    }

    async fn put(&mut self, _node: Node) -> prolly::Result<prolly::Hash> {
        unreachable!("reading only")
    }

    async fn flush(&mut self) -> prolly::Result<()> {
        Ok(())
    }
}

#[test]
fn a_cursor_dropped_mid_load_skips_nothing() {
    let (store, tree) = run(async {
        let mut store = MemStore::new();
        let tree = Tree::build(&mut store, NonZeroU32::new(4).unwrap(), (0..200).map(entry))
            .await
            .unwrap();
        (store, tree)
    });
    let expected: Vec<Item> = run(entries(&tree, &store)).unwrap();
    assert!(store.len() > 3, "a tree of several levels");
    for stall_at in 0..store.len() {
        let stalling = StallRead {
            inner: &store,
            reads: Cell::new(0),
            stall_at,
        };
        let mut cursor = tree.iter(&stalling);
        let mut items = Vec::new();
        loop {
            match poll_once(cursor.next()) {
                Poll::Pending => continue,
                Poll::Ready(Some(item)) => items.push(item.unwrap()),
                Poll::Ready(None) => break,
            }
        }
        assert_eq!(items, expected, "the load of node {stall_at} was dropped");
    }
}

/// A store whose first `put` waits forever.
struct StallFirstPut {
    inner: MemStore,
    stalled: bool,
}

impl Store for StallFirstPut {
    async fn get(&self, hash: &prolly::Hash) -> prolly::Result<Option<Arc<Node>>> {
        self.inner.get(hash).await
    }

    async fn put(&mut self, node: Node) -> prolly::Result<prolly::Hash> {
        if !self.stalled {
            self.stalled = true;
            std::future::pending::<()>().await;
        }
        self.inner.put(node).await
    }

    async fn flush(&mut self) -> prolly::Result<()> {
        self.inner.flush().await
    }
}

#[test]
fn a_build_refuses_to_finish_after_a_dropped_put() {
    let mut store = StallFirstPut {
        inner: MemStore::new(),
        stalled: false,
    };
    let mut builder = Builder::new(NonZeroU32::new(1).unwrap());
    let mut n = 0;
    loop {
        let (key, value) = entry(n);
        n += 1;
        match poll_once(builder.push(&mut store, key, value)) {
            Poll::Pending => break,
            Poll::Ready(pushed) => pushed.unwrap(),
        }
    }
    assert_eq!(
        run(builder.finish(&mut store)),
        Err(prolly::Error::BuildFailed),
        "a tree from this build would miss the chunk whose put was dropped"
    );
}
