//! KV snapshot import: a collection's documents as a stream of key-value
//! frames in Go Shinzo's layout, exported by a Go Shinzo node and imported
//! here (`go.shinzo.kv_snapshot`) by replaying the documents' blocks through
//! the merge.

mod error;
pub mod frame;
mod go_keys;
mod import;
mod mapping;

pub use error::KvSnapshotError;
pub use import::Imported;
pub use mapping::{FieldEntry, FieldMapping};
