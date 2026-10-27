//! Text chunking for `@chunk` fields.
//!
//! `sync` writes chunk documents in the parent's transaction, and [`access`]
//! decides who may read them outside a query. [`window`] is an older
//! fixed-size splitter that only the benchmark fixtures use.

pub use chunk_split::{list, split};

pub mod access;
pub(crate) mod encryption;
pub mod guard;
pub(crate) mod sync;
pub(crate) mod user_writes;
pub mod window;
