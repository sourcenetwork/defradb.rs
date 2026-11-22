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

    fn step(&mut self) -> Result<Option<Item>> {
        if let Some(root) = self.root.take() {
            self.path.push((load(self.store, &root)?, 0));
        }
        while let Some((node, next)) = self.path.last_mut() {
            let Some(item) = node.items.get(*next) else {
                self.path.pop();
                continue;
            };
            *next += 1;
            if node.level == 0 {
                return Ok(Some(item.clone()));
            }
            let child = load(self.store, &Hash::from_slice(&item.value)?)?;
            self.path.push((child, 0));
        }
        Ok(None)
    }
}

impl<S> Iterator for Entries<'_, S>
where
    S: Store,
{
    type Item = Result<Item>;

    fn next(&mut self) -> Option<Self::Item> {
        let step = self.step();
        if step.is_err() {
            self.path.clear();
        }
        step.transpose()
    }
}
