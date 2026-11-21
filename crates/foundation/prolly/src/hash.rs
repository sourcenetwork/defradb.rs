//! Node hashes and the boundary test.

use std::fmt;

use sha2::{Digest, Sha256};

use crate::error::{Error, Result};

/// The SHA-256 hash of a node's encoding.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Hash(pub [u8; 32]);

impl Hash {
    /// The hash of `bytes`.
    pub fn of(bytes: &[u8]) -> Self {
        Self(Sha256::digest(bytes).into())
    }

    /// Reads a hash from an internal item's value.
    pub fn from_slice(bytes: &[u8]) -> Result<Self> {
        let array: [u8; 32] = bytes
            .try_into()
            .map_err(|_| Error::BadChildHash(bytes.len()))?;
        Ok(Self(array))
    }
}

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|byte| write!(f, "{byte:02x}"))
    }
}

impl fmt::Debug for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Hash({self})")
    }
}

/// Whether the item `(key, value)` may close its chunk: true for about one item
/// in `target`. It reads only the item, which is what keeps an edit's re-cut
/// local (`ProllyTree.chunks_append`).
pub fn is_boundary(key: &[u8], value: &[u8], target: u32) -> bool {
    let mut hasher = Sha256::new();
    hasher.update((key.len() as u64).to_be_bytes());
    hasher.update(key);
    hasher.update(value);
    let digest = hasher.finalize();
    let roll = u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]);
    roll < u32::MAX / target.max(1)
}
