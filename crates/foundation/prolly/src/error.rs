//! Errors of the prolly tree.

use crate::hash::Hash;

/// What can go wrong reading a tree.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    /// The store has no node for a hash the tree refers to.
    #[error("prolly tree node {0} is missing from the store")]
    MissingNode(Hash),
    /// An internal item's value is not a 32-byte hash.
    #[error("prolly tree internal item holds {0} bytes, not a 32-byte hash")]
    BadChildHash(usize),
    /// A node holds no item; the tree never writes one.
    #[error("prolly tree holds an empty node at level {0}")]
    EmptyNode(u8),
    /// A build got a key smaller than the one before it.
    #[error("prolly tree build needs entries in ascending key order")]
    Unsorted,
    /// Stored bytes are not exactly one node's encoding.
    #[error("prolly tree node bytes are not a node encoding")]
    BadEncoding,
    /// A build whose store failed earlier: the chunk that failed is gone, so
    /// finishing would give a tree missing entries. Start a new build.
    #[error("prolly tree build failed earlier; start a new build")]
    BuildFailed,
    /// The store behind the tree failed.
    #[error("prolly tree store: {0}")]
    Store(String),
}

/// The crate's result.
pub type Result<T> = std::result::Result<T, Error>;
