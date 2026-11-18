//! Cutting a level into chunks (`ProllyTree.step`, `scan` and `chunks`).

use crate::{hash::is_boundary, node::Item};

/// The cutter's state: the chunks closed so far and the open one.
pub(crate) struct Chunker {
    target: u32,
    pub(crate) closed: Vec<Vec<Item>>,
    pub(crate) open: Vec<Item>,
}

impl Chunker {
    /// A cutter with nothing closed and nothing open.
    pub(crate) fn new(target: u32) -> Self {
        Self {
            target,
            closed: Vec::new(),
            open: Vec::new(),
        }
    }

    /// Adds `item` to the open chunk, and closes it when `item` is a boundary
    /// that is not alone, so a closed chunk holds at least two items and every
    /// level is shorter than the one below it (`ProllyTree.chunks_shrink`).
    pub(crate) fn push(&mut self, item: Item) {
        let closes = !self.open.is_empty() && is_boundary(&item.key, &item.value, self.target);
        self.open.push(item);
        if closes {
            self.closed.push(std::mem::take(&mut self.open));
        }
    }

    /// The chunks: the closed ones, then the open one if it holds anything.
    pub(crate) fn finish(mut self) -> Vec<Vec<Item>> {
        if !self.open.is_empty() {
            self.closed.push(self.open);
        }
        self.closed
    }
}

/// The chunks of `items`.
pub(crate) fn chunk(items: Vec<Item>, target: u32) -> Vec<Vec<Item>> {
    let mut chunker = Chunker::new(target);
    items.into_iter().for_each(|item| chunker.push(item));
    chunker.finish()
}
