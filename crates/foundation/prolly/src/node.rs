//! Tree nodes: one chunk of one level.

use crate::{
    error::{Error, Result},
    hash::Hash,
};

/// One item of a level: an entry at level 0, a child reference above it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    /// The entry's key; for a child, the largest key under it.
    pub key: Vec<u8>,
    /// The entry's value; for a child, its node's hash.
    pub value: Vec<u8>,
}

impl Item {
    /// The bytes this item adds to its node's encoding: two `u64` lengths, the
    /// key and the value.
    pub fn encoded_len(&self) -> usize {
        16 + self.key.len() + self.value.len()
    }
}

/// The encoded item bytes at which an open chunk closes even without a
/// boundary, so an open chunk holds less than this, or a single item. A
/// default-target chunk of items near 100 bytes reaches it with odds near
/// e^-20, so a chunk that closes on a boundary first keeps its hash
/// (`tests/golden.rs`).
pub const MAX_CHUNK_BYTES: usize = 64 * 1024;

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
    /// truncated and distinct nodes never share an encoding
    /// (`ProllyTree.encode_injective`).
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.encoded_len());
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

    /// The length of [`encode`](Self::encode)'s bytes: the level byte, the
    /// count and every item.
    pub fn encoded_len(&self) -> usize {
        9 + self.items.iter().map(Item::encoded_len).sum::<usize>()
    }

    /// The node `bytes` encode, the inverse of [`encode`](Self::encode)
    /// (`ProllyTree.decode_encode`). Bytes that are not exactly one node's
    /// encoding are [`Error::BadEncoding`].
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let (&level, mut rest) = bytes.split_first().ok_or(Error::BadEncoding)?;
        let count = take_len(&mut rest)?;
        // Every item takes at least 16 bytes, so a count the bytes cannot
        // hold allocates nothing.
        if count > rest.len() / 16 {
            return Err(Error::BadEncoding);
        }
        let mut items = Vec::with_capacity(count);
        for _ in 0..count {
            let key = take_bytes(&mut rest)?;
            let value = take_bytes(&mut rest)?;
            items.push(Item { key, value });
        }
        if !rest.is_empty() {
            return Err(Error::BadEncoding);
        }
        Ok(Self { level, items })
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

/// Reads a big-endian `u64` length off the front of `rest`.
fn take_len(rest: &mut &[u8]) -> Result<usize> {
    let (head, tail) = rest.split_first_chunk::<8>().ok_or(Error::BadEncoding)?;
    *rest = tail;
    usize::try_from(u64::from_be_bytes(*head)).map_err(|_| Error::BadEncoding)
}

/// Reads a length and that many bytes off the front of `rest`.
fn take_bytes(rest: &mut &[u8]) -> Result<Vec<u8>> {
    let len = take_len(rest)?;
    let (bytes, tail) = rest.split_at_checked(len).ok_or(Error::BadEncoding)?;
    *rest = tail;
    Ok(bytes.to_vec())
}

#[cfg(test)]
mod tests {
    use super::{Item, Node};
    use crate::error::Error;

    #[test]
    fn decode_reads_back_what_encode_writes_and_refuses_the_rest() {
        let node = Node {
            level: 3,
            items: vec![
                Item {
                    key: b"a".to_vec(),
                    value: vec![],
                },
                Item {
                    key: b"bc".to_vec(),
                    value: b"xyz".to_vec(),
                },
            ],
        };
        let bytes = node.encode();
        assert_eq!(bytes.len(), node.encoded_len());
        assert_eq!(Node::decode(&bytes), Ok(node));
        for len in 0..bytes.len() {
            assert_eq!(Node::decode(&bytes[..len]), Err(Error::BadEncoding));
        }
        let mut longer = bytes.clone();
        longer.push(0);
        assert_eq!(Node::decode(&longer), Err(Error::BadEncoding));
        let mut huge = vec![0];
        huge.extend_from_slice(&u64::MAX.to_be_bytes());
        assert_eq!(Node::decode(&huge), Err(Error::BadEncoding));
    }

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
