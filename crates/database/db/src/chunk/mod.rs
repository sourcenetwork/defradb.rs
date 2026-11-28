//! Text chunking for `@chunk` fields.
//!
//! `sync` writes chunk documents in the parent's transaction, and [`access`]
//! decides who may read them outside a query. [`window`] is an older
//! fixed-size splitter that only the benchmark fixtures use.

pub use chunk_split::{hash, split};

pub use config::chunk_config;
pub use owner::ChunkOwner;
pub use user_writes::ensure_admin_changeable;

pub mod access;
mod config;
pub(crate) mod follow;
pub mod guard;
mod owner;
pub(crate) mod peer_delete;
pub(crate) mod purge;
pub(crate) mod rebuild;
pub(crate) mod sync;
pub(crate) mod user_writes;
pub mod window;
