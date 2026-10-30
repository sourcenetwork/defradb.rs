//! Text chunking: cutting a `@chunk` field into chunk documents.
//!
//! [`split`] cuts a text into chunks and [`list`] builds the parent's chunk
//! list from them, [`access`] decides who a chunk's blocks and keys belong
//! to outside a query plan, and [`window`] is the older fixed-size splitter
//! the benchmark fixtures use.

pub use chunk_split::{list, split};

pub mod access;
pub(crate) mod user_writes;
pub mod window;
