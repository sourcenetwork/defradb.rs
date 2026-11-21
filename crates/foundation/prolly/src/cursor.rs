//! Reading a tree's entries in key order, one node per level at a time.

use std::sync::Arc;

use crate::{
    error::Result,
    hash::Hash,
    node::{Item, Node},
    store::Store,
    tree::load,
};

/// A tree's entries in key order, read as they are asked for.
///
/// It holds the path from the root to the current leaf, one node per level,
/// and loads each node once, when the walk reaches it. After an error it ends.
pub struct Entries<'s, S> {
    store: &'s S,
    root: Option<Hash>,
    path: Vec<(Arc<Node>, usize)>,
}

impl<'s, S> Entries<'s, S>
where
    S: Store,
{
    pub(crate) fn new(store: &'s S, root: Option<Hash>) -> Self {
        Self {
            store,
            root,
            path: Vec::new(),
        }
    }

    /// How many nodes it holds: at most one per level of the tree.
    pub fn held(&self) -> usize {
        self.path.len()
    }

    /// The next entry, `None` once every entry has been read or after an error.
    pub async fn next(&mut self) -> Option<Result<Item>> {
        let step = self.step().await;
        if step.is_err() {
            self.path.clear();
        }
        step.transpose()
    }

    // The position moves only after a load returns, so a `next` dropped while
    // a load is pending loses nothing: the next call loads it again.
    async fn step(&mut self) -> Result<Option<Item>> {
        if let Some(root) = self.root {
            let node = load(self.store, &root).await?;
            self.root = None;
            self.path.push((node, 0));
        }
        while let Some((node, next)) = self.path.last_mut() {
            let Some(item) = node.items.get(*next) else {
                self.path.pop();
                continue;
            };
            if node.level == 0 {
                *next += 1;
                return Ok(Some(item.clone()));
            }
            let hash = Hash::from_slice(&item.value)?;
            let child = load(self.store, &hash).await?;
            if let Some((_, next)) = self.path.last_mut() {
                *next += 1;
            }
            self.path.push((child, 0));
        }
        Ok(None)
    }
}
