//! Building a tree from entries in key order, holding one open chunk per level.

use std::num::NonZeroU32;

use crate::{
    chunk::Chunker,
    error::{Error, Result},
    node::Item,
    store::Store,
    tree::{child, collapse, Tree},
};

/// Builds a tree from entries pushed in ascending key order.
///
/// Each level holds only its open chunk: a chunk is stored as soon as it
/// closes, and its reference goes to the level above. Working memory is one
/// open chunk per level and the last entry pushed, never the whole set. The
/// chunks are those `raise` cuts from the same entries, level by level, so
/// the root is the same; `tests/history.rs` checks it against edits, which go
/// through `raise`.
pub struct Builder {
    target: u32,
    levels: Vec<Level>,
    last: Option<Item>,
}

/// One level being cut: its open chunk, and how many chunks it has closed.
struct Level {
    chunker: Chunker,
    closed: usize,
}

impl Builder {
    /// A builder for chunks of `target` items on average.
    pub fn new(target: NonZeroU32) -> Self {
        Self {
            target: target.get(),
            levels: Vec::new(),
            last: None,
        }
    }

    /// Adds `key`. A key equal to the last one replaces its value, so the last
    /// value wins; a smaller key is [`Error::Unsorted`].
    pub fn push<S>(&mut self, store: &mut S, key: Vec<u8>, value: Vec<u8>) -> Result<()>
    where
        S: Store,
    {
        match &mut self.last {
            Some(last) if last.key == key => {
                last.value = value;
                return Ok(());
            }
            Some(last) if key < last.key => return Err(Error::Unsorted),
            _ => {}
        }
        if let Some(previous) = self.last.replace(Item { key, value }) {
            self.feed(store, 0, previous);
        }
        Ok(())
    }

    /// How many items it holds: every level's open chunk and the last entry.
    pub fn held(&self) -> usize {
        let open: usize = self
            .levels
            .iter()
            .map(|level| level.chunker.open.len())
            .sum();
        open + usize::from(self.last.is_some())
    }

    /// The tree of everything pushed.
    pub fn finish<S>(mut self, store: &mut S) -> Result<Tree>
    where
        S: Store,
    {
        let target = NonZeroU32::new(self.target).unwrap_or(NonZeroU32::MIN);
        if let Some(last) = self.last.take() {
            self.feed(store, 0, last);
        }
        let mut level = 0;
        while level < self.levels.len() {
            let open = std::mem::take(&mut self.levels[level].chunker.open);
            if self.levels[level].closed == 0 {
                // One chunk is the whole level: it is the root, as in `raise`.
                let root = collapse(store, level as u8, open)?;
                return Ok(Tree { root, target });
            }
            if !open.is_empty() {
                self.close(store, level, open);
            }
            level += 1;
        }
        Ok(Tree::empty(target))
    }

    /// Adds `item` to `level`, storing the chunk it closes.
    fn feed<S>(&mut self, store: &mut S, level: usize, item: Item)
    where
        S: Store,
    {
        if level == self.levels.len() {
            self.levels.push(Level {
                chunker: Chunker::new(self.target),
                closed: 0,
            });
        }
        self.levels[level].chunker.push(item);
        if let Some(chunk) = self.levels[level].chunker.closed.pop() {
            self.close(store, level, chunk);
        }
    }

    /// Stores `chunk` of `level` and gives its reference to the level above.
    fn close<S>(&mut self, store: &mut S, level: usize, chunk: Vec<Item>)
    where
        S: Store,
    {
        self.levels[level].closed += 1;
        let reference = child(store, level as u8, chunk);
        self.feed(store, level + 1, reference);
    }
}
