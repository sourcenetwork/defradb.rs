//! What the tests share: running a future, reading every entry, and a store
//! that counts its reads.

#![allow(dead_code)]

use std::{cell::Cell, future::Future, sync::Arc};

use prolly::{Hash, Item, MemStore, Node, Store, Tree};

/// Runs `future` to completion on this thread.
pub fn run<F>(future: F) -> F::Output
where
    F: Future,
{
    workers::block_on(future).unwrap()
}

/// Every entry of `tree`, in key order.
pub async fn entries<S>(tree: &Tree, store: &S) -> prolly::Result<Vec<Item>>
where
    S: Store,
{
    let mut cursor = tree.iter(store);
    let mut items = Vec::new();
    while let Some(item) = cursor.next().await {
        items.push(item?);
    }
    Ok(items)
}

/// A store that counts the nodes read through it.
pub struct Counting<'a> {
    pub inner: &'a MemStore,
    pub reads: Cell<usize>,
}

impl<'a> Counting<'a> {
    pub fn new(inner: &'a MemStore) -> Self {
        Self {
            inner,
            reads: Cell::new(0),
        }
    }
}

impl Store for Counting<'_> {
    async fn get(&self, hash: &Hash) -> prolly::Result<Option<Arc<Node>>> {
        self.reads.set(self.reads.get() + 1);
        self.inner.get(hash).await
    }

    async fn put(&mut self, _node: Node) -> prolly::Result<Hash> {
        unreachable!("reading only")
    }

    async fn flush(&mut self) -> prolly::Result<()> {
        Ok(())
    }
}
