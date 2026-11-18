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
    ///
    /// The level byte, then the item count, then each key and value behind
    /// its length, every length a big-endian `u64` so that no length is
    /// truncated and distinct nodes never share an encoding.
    pub fn encode(&self) -> Vec<u8> {
        let size: usize = self
            .items
            .iter()
            .map(|item| 16 + item.key.len() + item.value.len())
            .sum();
        let mut bytes = Vec::with_capacity(9 + size);
        bytes.push(self.level);
        bytes.extend_from_slice(&(self.items.len() as u64).to_be_bytes());
        for item in &self.items {
            bytes.extend_from_slice(&(item.key.len() as u64).to_be_bytes());
            bytes.extend_from_slice(&item.key);
            bytes.extend_from_slice(&(item.value.len() as u64).to_be_bytes());
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

#[cfg(test)]
mod tests {
    use super::{Item, Node};

    #[test]
    fn encode_prefixes_every_length_with_a_u64() {
        let node = Node {
            level: 2,
            items: vec![Item {
                key: b"ab".to_vec(),
                value: b"xyz".to_vec(),
            }],
        };
        let mut want = vec![2];
        want.extend_from_slice(&1u64.to_be_bytes());
        want.extend_from_slice(&2u64.to_be_bytes());
        want.extend_from_slice(b"ab");
        want.extend_from_slice(&3u64.to_be_bytes());
        want.extend_from_slice(b"xyz");
        assert_eq!(node.encode(), want);
    }
}
