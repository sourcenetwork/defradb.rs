//! Cutting a level into chunks (`ProllyTree.step`, `scan` and `chunks`).

use crate::{
    hash::is_boundary,
    node::{Item, MAX_CHUNK_BYTES},
};

/// The cutter's state: the chunks closed so far and the open one.
pub(crate) struct Chunker {
    target: u32,
    pub(crate) closed: Vec<Vec<Item>>,
    open: Vec<Item>,
    /// The encoded bytes of the open chunk's items.
    open_bytes: usize,
}

impl Chunker {
    /// A cutter with nothing closed and nothing open.
    pub(crate) fn new(target: u32) -> Self {
        Self {
            target,
            closed: Vec::new(),
            open: Vec::new(),
            open_bytes: 0,
        }
    }

    /// Adds `item` to the open chunk, and closes it when `item` is not alone
    /// and is a boundary or brings the chunk to [`MAX_CHUNK_BYTES`]. A closed
    /// chunk holds at least two items, so every level is shorter than the one
    /// below it (`ProllyTree.chunks_shrink`), and an open chunk holds under
    /// the cap or a single item (`ProllyTree.open_bounded`).
    pub(crate) fn push(&mut self, item: Item) {
        let bytes = self.open_bytes + item.encoded_len();
        let closes = !self.open.is_empty()
            && (bytes >= MAX_CHUNK_BYTES || is_boundary(&item.key, &item.value, self.target));
        self.open.push(item);
        self.open_bytes = bytes;
        if closes {
            self.closed.push(std::mem::take(&mut self.open));
            self.open_bytes = 0;
        }
    }

    /// Whether the open chunk holds nothing.
    pub(crate) fn is_closed(&self) -> bool {
        self.open.is_empty()
    }

    /// The encoded bytes of the open chunk.
    pub(crate) fn open_bytes(&self) -> usize {
        self.open_bytes
    }

    /// Takes the open chunk, leaving the cutter with nothing open.
    pub(crate) fn take_open(&mut self) -> Vec<Item> {
        self.open_bytes = 0;
        std::mem::take(&mut self.open)
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
