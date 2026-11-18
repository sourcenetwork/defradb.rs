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
}

/// The crate's result.
pub type Result<T> = std::result::Result<T, Error>;
