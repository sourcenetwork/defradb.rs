//! Inserting and removing one entry, re-cutting the chunks from it on until
//! the cut lines up with the old chunks again.
//!
//! At each level the edit replaces a run of items with new ones. The chunks
//! holding that run are cut again, from the start of the first, and the cut
//! goes on one whole old chunk at a time until it ends an old chunk with
//! nothing open. Usually that is the next chunk or two. It need not happen at
//! all: chunks of two large items closed by the byte cap, with one more such
//! item added in front, leave one item open after every old chunk, and the
//! edit re-cuts the rest of the level. Everything before the
//! first chunk closed before it, and everything after the last starts from a
//! closed chunk, so the result is the cut of the whole level
//! (`ProllyTree.rewrite_level_refines`). The new chunks then replace the old
//! ones' items in the level above (`ProllyTree.rewrite_level_up`), and so on
//! up to the root.

use std::sync::Arc;

use crate::{
    chunk::Chunker,
    error::{Error, Result},
    hash::Hash,
    node::{Item, Node},
    store::Store,
    tree::{child, load, raise, Tree},
};

impl Tree {
    /// Sets `key` to `value`.
    pub async fn insert<S>(&mut self, store: &mut S, key: Vec<u8>, value: Vec<u8>) -> Result<()>
    where
        S: Store,
    {
        self.edit(store, key, Some(value)).await
    }

    /// Removes `key`, if it is there.
    pub async fn remove<S>(&mut self, store: &mut S, key: &[u8]) -> Result<()>
    where
        S: Store,
    {
        self.edit(store, key.to_vec(), None).await
    }

    async fn edit<S>(&mut self, store: &mut S, key: Vec<u8>, value: Option<Vec<u8>>) -> Result<()>
    where
        S: Store,
    {
        let target = self.target.get();
        let Some(root_hash) = self.root else {
            if let Some(value) = value {
                self.root = raise(store, vec![Item { key, value }], 0, target).await?;
            }
            return Ok(());
        };
        let root = load(store, &root_hash).await?;
        let mut low = key.clone();
        let mut high = key.clone();
        let mut replacement: Vec<Item> =
            value.map(|value| Item { key, value }).into_iter().collect();
        for level in 0..root.level {
            let window =
                rewrite_level(store, &root, level, &low, &high, replacement, target).await?;
            replacement = Vec::with_capacity(window.chunks.len());
            for items in window.chunks {
                replacement.push(child(store, level, items).await?);
            }
            low = window.first;
            high = window.last;
        }
        let items = splice(root.items.clone(), &low, &high, replacement);
        self.root = raise(store, items, root.level, target).await?;
        Ok(())
    }
}

/// The old chunks one edit consumed at a level, by their last keys, and the
/// chunks that replace them.
struct Window {
    first: Vec<u8>,
    last: Vec<u8>,
    chunks: Vec<Vec<Item>>,
}

/// Replaces the items of `level` with keys in `low..=high` by `replacement`,
/// and cuts the chunks around them again.
async fn rewrite_level<S>(
    store: &S,
    root: &Arc<Node>,
    level: u8,
    low: &[u8],
    high: &[u8],
    replacement: Vec<Item>,
    target: u32,
) -> Result<Window>
where
    S: Store,
{
    let start = seek_at_least(store, root, level, low).await?;
    let first = start.last_key().to_vec();
    let mut last = first.clone();
    let mut items = start.items.clone();
    while last.as_slice() < high {
        let Some(next) = seek_after(store, root, level, &last).await? else {
            break;
        };
        items.extend(next.items.iter().cloned());
        last = next.last_key().to_vec();
    }
    let mut chunker = Chunker::new(target);
    splice(items, low, high, replacement)
        .into_iter()
        .for_each(|item| chunker.push(item));
    while !chunker.is_closed() {
        let Some(next) = seek_after(store, root, level, &last).await? else {
            break;
        };
        next.items
            .iter()
            .cloned()
            .for_each(|item| chunker.push(item));
        last = next.last_key().to_vec();
    }
    Ok(Window {
        first,
        last,
        chunks: chunker.finish(),
    })
}

/// `items` with those keyed in `low..=high` replaced by `replacement`.
fn splice(items: Vec<Item>, low: &[u8], high: &[u8], replacement: Vec<Item>) -> Vec<Item> {
    let mut spliced = Vec::with_capacity(items.len() + replacement.len());
    let mut rest = items.into_iter().peekable();
    while let Some(item) = rest.next_if(|item| item.key.as_slice() < low) {
        spliced.push(item);
    }
    while rest.next_if(|item| item.key.as_slice() <= high).is_some() {}
    spliced.extend(replacement);
    spliced.extend(rest);
    spliced
}

/// The chunk of `level` holding `key`: the first whose last key is at least
/// `key`, or the level's last chunk when every key is smaller.
async fn seek_at_least<S>(store: &S, root: &Arc<Node>, level: u8, key: &[u8]) -> Result<Arc<Node>>
where
    S: Store,
{
    let mut node = Arc::clone(root);
    while node.level > level {
        let index = node.items.partition_point(|item| item.key.as_slice() < key);
        let item = node
            .items
            .get(index)
            .or(node.items.last())
            .ok_or(Error::EmptyNode(node.level))?;
        node = load(store, &Hash::from_slice(&item.value)?).await?;
    }
    Ok(node)
}

/// The chunk of `level` right after the one whose last key is `key`.
async fn seek_after<S>(
    store: &S,
    root: &Arc<Node>,
    level: u8,
    key: &[u8],
) -> Result<Option<Arc<Node>>>
where
    S: Store,
{
    let mut node = Arc::clone(root);
    while node.level > level {
        let index = node
            .items
            .partition_point(|item| item.key.as_slice() <= key);
        let Some(item) = node.items.get(index) else {
            return Ok(None);
        };
        node = load(store, &Hash::from_slice(&item.value)?).await?;
    }
    Ok(Some(node))
}
