//! A tree: its root, and building and reading it.

use std::{collections::BTreeMap, num::NonZeroU32, sync::Arc};

use crate::{
    chunk::chunk,
    error::{Error, Result},
    hash::Hash,
    node::{Item, Node},
    store::Store,
};

/// The average number of items per chunk a tree aims at by default.
pub const DEFAULT_TARGET: NonZeroU32 = NonZeroU32::new(32).unwrap();

/// A prolly tree: the hash of its root node, or nothing when it is empty.
///
/// The same entries and target always give the same root
/// (`ProllyTree.build_same_entries`), whichever writes led to them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tree {
    pub(crate) root: Option<Hash>,
    pub(crate) target: NonZeroU32,
}

impl Tree {
    /// An empty tree whose chunks hold `target` items on average.
    pub fn empty(target: NonZeroU32) -> Self {
        Self { root: None, target }
    }

    /// The tree of `entries`; for a repeated key the last value wins.
    pub fn build<S, I>(store: &mut S, target: NonZeroU32, entries: I) -> Result<Self>
    where
        S: Store,
        I: IntoIterator<Item = (Vec<u8>, Vec<u8>)>,
    {
        let sorted: BTreeMap<Vec<u8>, Vec<u8>> = entries.into_iter().collect();
        let items = sorted
            .into_iter()
            .map(|(key, value)| Item { key, value })
            .collect();
        let root = raise(store, items, 0, target.get())?;
        Ok(Self { root, target })
    }

    /// The root hash, `None` for an empty tree.
    pub fn root(&self) -> Option<Hash> {
        self.root
    }

    /// The average number of items per chunk.
    pub fn target(&self) -> NonZeroU32 {
        self.target
    }

    /// The value stored for `key`.
    pub fn get<S>(&self, store: &S, key: &[u8]) -> Result<Option<Vec<u8>>>
    where
        S: Store,
    {
        let Some(mut hash) = self.root else {
            return Ok(None);
        };
        loop {
            let node = load(store, &hash)?;
            let index = node.items.partition_point(|item| item.key.as_slice() < key);
            let Some(item) = node.items.get(index) else {
                return Ok(None);
            };
            if node.level == 0 {
                return Ok((item.key == key).then(|| item.value.clone()));
            }
            hash = Hash::from_slice(&item.value)?;
        }
    }

    /// Every entry, in key order.
    pub fn entries<S>(&self, store: &S) -> Result<Vec<Item>>
    where
        S: Store,
    {
        let mut entries = Vec::new();
        let mut pending: Vec<Hash> = self.root.into_iter().collect();
        while let Some(hash) = pending.pop() {
            let node = load(store, &hash)?;
            if node.level == 0 {
                entries.extend(node.items.iter().cloned());
                continue;
            }
            for item in node.items.iter().rev() {
                pending.push(Hash::from_slice(&item.value)?);
            }
        }
        Ok(entries)
    }
}

/// The node stored under `hash`.
pub(crate) fn load<S>(store: &S, hash: &Hash) -> Result<Arc<Node>>
where
    S: Store,
{
    store.get(hash).ok_or(Error::MissingNode(*hash))
}

/// Stores the chunk `items` of `level` and returns its item for the level above.
pub(crate) fn child<S>(store: &mut S, level: u8, items: Vec<Item>) -> Item
where
    S: Store,
{
    let key = items
        .last()
        .map(|item| item.key.clone())
        .unwrap_or_default();
    let hash = store.put(Node { level, items });
    Item {
        key,
        value: hash.0.to_vec(),
    }
}

/// Cuts `items` of `level` and goes up until a level is one chunk
/// (`ProllyTree.build`), then drops any root above it that holds a single
/// child, so the root is the lowest level that is one chunk.
pub(crate) fn raise<S>(
    store: &mut S,
    mut items: Vec<Item>,
    mut level: u8,
    target: u32,
) -> Result<Option<Hash>>
where
    S: Store,
{
    loop {
        let mut chunks = chunk(items, target);
        if chunks.len() <= 1 {
            let Some(items) = chunks.pop() else {
                return Ok(None);
            };
            let mut node = Arc::new(Node { level, items });
            while node.level > 0 && node.items.len() == 1 {
                node = load(store, &Hash::from_slice(&node.items[0].value)?)?;
            }
            return Ok(Some(store.put(Arc::unwrap_or_clone(node))));
        }
        items = chunks
            .into_iter()
            .map(|items| child(store, level, items))
            .collect();
        level += 1;
    }
}
