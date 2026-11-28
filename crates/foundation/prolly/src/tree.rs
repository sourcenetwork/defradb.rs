//! A tree: its root, and building and reading it.

use std::{num::NonZeroU32, sync::Arc};

use crate::{
    build::Builder,
    chunk::chunk,
    cursor::Entries,
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

    /// The tree of `entries`, given in ascending key order. A key equal to the
    /// one before replaces its value, so the last value wins; a smaller key is
    /// [`Error::Unsorted`]. It holds one open chunk per level ([`Builder`]),
    /// each under [`MAX_CHUNK_BYTES`](crate::node::MAX_CHUNK_BYTES) or a
    /// single item.
    pub async fn build<S, I>(store: &mut S, target: NonZeroU32, entries: I) -> Result<Self>
    where
        S: Store,
        I: IntoIterator<Item = (Vec<u8>, Vec<u8>)>,
    {
        let mut builder = Builder::new(target);
        for (key, value) in entries {
            builder.push(store, key, value).await?;
        }
        builder.finish(store).await
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
    pub async fn get<S>(&self, store: &S, key: &[u8]) -> Result<Option<Vec<u8>>>
    where
        S: Store,
    {
        let Some(mut hash) = self.root else {
            return Ok(None);
        };
        loop {
            let node = load(store, &hash).await?;
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

    /// Every entry, in key order, read one node per level at a time.
    pub fn iter<'s, S>(&self, store: &'s S) -> Entries<'s, S>
    where
        S: Store,
    {
        Entries::new(store, self.root)
    }
}

/// The node stored under `hash`.
pub(crate) async fn load<S>(store: &S, hash: &Hash) -> Result<Arc<Node>>
where
    S: Store,
{
    store.get(hash).await?.ok_or(Error::MissingNode(*hash))
}

/// Stores the chunk `items` of `level` and returns its item for the level above.
pub(crate) async fn child<S>(store: &mut S, level: u8, items: Vec<Item>) -> Result<Item>
where
    S: Store,
{
    let key = items
        .last()
        .map(|item| item.key.clone())
        .unwrap_or_default();
    let hash = store.put(Node { level, items }).await?;
    Ok(Item {
        key,
        value: hash.0.to_vec(),
    })
}

/// Cuts `items` of `level` and goes up until a level is one chunk
/// (`ProllyTree.build`), then stores that chunk as the root ([`collapse`]).
pub(crate) async fn raise<S>(
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
            return collapse(store, level, chunks.pop().unwrap_or_default()).await;
        }
        items = Vec::with_capacity(chunks.len());
        for chunk in chunks {
            items.push(child(store, level, chunk).await?);
        }
        level += 1;
    }
}

/// Stores `items` as the root of `level`, after dropping every root above it
/// that holds a single child, so the root is the lowest level that is one
/// chunk (`ProllyTree.collapse`); `None` when there are no items. It writes
/// nothing durable: the store's owner commits the nodes put with the root
/// that names them (`proofs/tla/ProllyNodeReclaim.tla`).
pub(crate) async fn collapse<S>(store: &mut S, level: u8, items: Vec<Item>) -> Result<Option<Hash>>
where
    S: Store,
{
    if items.is_empty() {
        return Ok(None);
    }
    let mut node = Arc::new(Node { level, items });
    while node.level > 0 && node.items.len() == 1 {
        node = load(store, &Hash::from_slice(&node.items[0].value)?).await?;
    }
    let root = store.put(Arc::unwrap_or_clone(node)).await?;
    Ok(Some(root))
}
