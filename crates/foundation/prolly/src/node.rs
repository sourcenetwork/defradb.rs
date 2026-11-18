//! Tree nodes: one chunk of one level.

use crate::hash::Hash;

/// One item of a level: an entry at level 0, a child reference above it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    /// The entry's key; for a child, the largest key under it.
    pub key: Vec<u8>,
    /// The entry's value; for a child, its node's hash.
    pub value: Vec<u8>,
}

/// One chunk of one level, stored under its hash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    /// 0 for a leaf, one more for each level above.
    pub level: u8,
    /// The chunk's items, sorted by key, never empty.
    pub items: Vec<Item>,
}

impl Node {
    /// The canonical bytes the node's hash is taken over.
    pub fn encode(&self) -> Vec<u8> {
        let size: usize = self
            .items
            .iter()
            .map(|item| 8 + item.key.len() + item.value.len())
            .sum();
        let mut bytes = Vec::with_capacity(5 + size);
        bytes.push(self.level);
        bytes.extend_from_slice(&(self.items.len() as u32).to_be_bytes());
        for item in &self.items {
            bytes.extend_from_slice(&(item.key.len() as u32).to_be_bytes());
            bytes.extend_from_slice(&item.key);
            bytes.extend_from_slice(&(item.value.len() as u32).to_be_bytes());
            bytes.extend_from_slice(&item.value);
        }
        bytes
    }

    /// The node's hash.
    pub fn hash(&self) -> Hash {
        Hash::of(&self.encode())
    }

    /// The largest key in the node, which is its last.
    pub fn last_key(&self) -> &[u8] {
        self.items.last().map_or(&[], |item| item.key.as_slice())
    }
}
