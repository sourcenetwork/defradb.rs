//! Text chunking: cutting a `@chunk` field into chunk documents.
//!
//! [`split`] cuts a text into chunks with the [`chunk_config`] a field's
//! definition names, and [`list`] builds the parent's chunk list from them;
//! [`window`] is the older fixed-size splitter the benchmark fixtures use.

pub use chunk_split::{list, split};

pub use config::chunk_config;
pub use user_writes::ensure_admin_changeable;

mod config;
pub(crate) mod user_writes;
pub mod window;
