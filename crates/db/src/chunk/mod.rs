//! Text chunking: cutting a `@chunk` field into chunk documents.

pub use chunk_split::{list, split};

pub use config::chunk_config;
pub use user_writes::ensure_admin_changeable;

pub mod access;
mod config;
pub mod guard;
pub(crate) mod peer_delete;
pub(crate) mod user_writes;
pub mod window;
