//! A prolly tree: a Merkle search tree whose chunk boundaries come from the
//! content, so the same entries always give the same root hash, whatever order
//! they were written in.
//!
//! Entries are byte keys and byte values, kept sorted by key. The bottom level
//! is cut into chunks after each item whose hash says so ([`hash::is_boundary`]);
//! each chunk becomes one item of the level above (its last key and its hash),
//! and so on until a level is one chunk: the root. An insert or a remove re-cuts
//! only the chunks around it. [`diff`] walks two trees down only where their
//! hashes differ.
//!
//! The invariants are proven in `proofs/lean/ProllyTree.lean`; the sync
//! protocol this tree serves is modeled in `proofs/tla/ProllySync.tla`.

mod chunk;
pub mod diff;
mod edit;
pub mod error;
pub mod hash;
pub mod node;
pub mod store;
pub mod tree;

pub use diff::{diff, Difference};
pub use error::{Error, Result};
pub use hash::Hash;
pub use node::{Item, Node};
pub use store::{MemStore, Store};
pub use tree::Tree;
