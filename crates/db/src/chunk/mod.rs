//! Text chunking: cutting a `@chunk` field into chunk documents.
//!
//! [`split`] cuts a text into chunks and [`list`] builds the parent's chunk
//! list from them; [`window`] is the older fixed-size splitter the benchmark
//! fixtures use.

pub use chunk_split::{list, split};

pub mod window;
